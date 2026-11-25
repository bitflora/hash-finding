"""Template-native core search (F35): find low-16-bit core pairs for a shift
template other than 16/15/15 from scratch, instead of transplanting cores that
were tuned at 16/15/15. Python only builds specs and reads mixlab output; every
score comes from the harness (CLAUDE.md).

Stages (state in results/native_cores/<template>/, e.g. 15_15_15):

  python analysis/native_cores.py gen      --template 15/15/15 [--pairs 20000] [--seed 1]
      Candidate core pairs (m0 low 16 bits, m1 low 16 bits), deduplicated after
      folding each core with its modular negative (F12):
        random      uniformly random odd cores
        arith       m0 core = q^-1 * r mod 2^16, q in {3,5,7,9,17}, small odd r
                    (the F27 generator), each with random m1 cores
        transplant  the best known 16/15/15 core pairs (explored32.csv), as the
                    control F34 effectively used
      Each pair gets the same H random high halves (paired design). stage1.csv.
  python analysis/native_cores.py screen1  [--log2 22]
      Sampled bias of stage 1 (examples/screen_list).
  python analysis/native_cores.py stage2   [--top 300] [--draws 8] [--log2 22]
      The top pairs by mean stage-1 ms (plus every transplant control) get
      --draws fresh high halves, screened with a new seed. Pairs are ranked by
      the median over those draws: a core-quality estimate free of the stage-1
      selection (F16's statistic).
  python analysis/native_cores.py exact    [--top 40]
      mixlab exact on the best stage-2 extension of the top pairs, and of the
      top 5 transplant controls.
  python analysis/native_cores.py jobs     [--top 10] [--seeds 1,2]
      Race jobs (--fix-shifts) from the best exact extension of the top pairs.
      Run: CAMPAIGN_DIR=results/native_cores/<t>/campaign python analysis/campaign.py run
  python analysis/native_cores.py summary
      Tables of stage 2, exact finalists and race winners; writes
      below_10704.csv with every exactly scored config below 0.10704.

Builds needed: cargo build --release --bin mixlab --example screen_list
"""
import csv, json, os, random, statistics, subprocess, sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
EXE = ".exe" if os.name == "nt" else ""
MIXLAB = ROOT / "harness" / "target" / "release" / f"mixlab{EXE}"
SCREEN = ROOT / "harness" / "target" / "release" / "examples" / f"screen_list{EXE}"
H = 2  # stage-1 high halves per pair
QS = (3, 5, 7, 9, 17)
M16 = 0xFFFF


def opt(name, default):
    return sys.argv[sys.argv.index(name) + 1] if name in sys.argv else default


TEMPLATE = tuple(int(x) for x in opt("--template", "15/15/15").split("/"))
D = ROOT / "results" / "native_cores" / "_".join(map(str, TEMPLATE))


def spec(m0, m1, t=TEMPLATE):
    return f"[{t[0]} {m0:08x} {t[1]} {m1:08x} {t[2]}]"


def fold(c):
    return min(c & M16, (-c) & M16)


def key(c0, c1):
    return f"{fold(c0):04x}/{fold(c1):04x}"


def read(path):
    with open(path, newline="") as f:
        return list(csv.DictReader(f))


def write(path, rows, fields=None):
    with open(path, "w", newline="") as f:
        w = csv.DictWriter(f, fieldnames=fields or list(rows[0]))
        w.writeheader()
        w.writerows(rows)


def known_pairs():
    """Best exact score per sign-folded 16/15/15 core pair in explored32.csv."""
    best = {}
    for r in read(ROOT / "results" / "explored32.csv"):
        if (int(r["s0"]), int(r["s1"]), int(r["s2"])) != (16, 15, 15):
            continue
        m0, m1, b = int(r["m0"], 16), int(r["m1"], 16), float(r["bias"])
        k = key(m0, m1)
        if k not in best or b < best[k][0]:
            best[k] = (b, m0 & M16, m1 & M16)
    return best


def gen():
    n, seed = int(opt("--pairs", 20000)), int(opt("--seed", 1))
    rng = random.Random(seed)
    odd16 = lambda: rng.getrandbits(16) | 1
    known = known_pairs()
    pairs = {}  # key -> (group, c0, c1)
    for k, (b, c0, c1) in sorted(known.items(), key=lambda kv: kv[1][0])[:30]:
        pairs[k] = ("transplant", c0, c1)
    for q in QS:
        qi = pow(q, -1, 1 << 16)
        for r in range(-63, 64, 2):
            c0 = (qi * r) & M16
            for _ in range(5):
                k = key(c0, c1 := odd16())
                if k not in pairs and k not in known:
                    pairs[k] = (f"arith_q{q}", c0, c1)
    added = 0
    while added < n:
        c0, c1 = odd16(), odd16()
        k = key(c0, c1)
        if k not in pairs and k not in known:
            pairs[k] = ("random", c0, c1)
            added += 1
    highs = [(rng.getrandbits(16), rng.getrandbits(16)) for _ in range(H)]
    rows = []
    for k, (g, c0, c1) in pairs.items():
        for h, (h0, h1) in enumerate(highs):
            rows.append(dict(spec=spec(h0 << 16 | c0, h1 << 16 | c1), pair=k, group=g, draw=h))
    D.mkdir(parents=True, exist_ok=True)
    write(D / "stage1.csv", rows)
    groups = {}
    for g, _, _ in pairs.values():
        groups[g.split("_")[0]] = groups.get(g.split("_")[0], 0) + 1
    print(f"{len(pairs)} pairs {groups}, {len(rows)} specs -> {D / 'stage1.csv'}")


def screen(inp, out, lg, seed):
    if not SCREEN.exists():
        sys.exit(f"missing {SCREEN}; cargo build --release --example screen_list")
    subprocess.run([str(SCREEN), str(inp), str(out), str(lg), str(seed)], check=True)


def scores(path):
    return {r["spec"]: float(r["sampled_ms"]) for r in read(path)}


def stage2():
    top, draws, lg = int(opt("--top", 300)), int(opt("--draws", 8)), int(opt("--log2", 22))
    s1 = read(D / "stage1.csv")
    ms = scores(D / "screen1.csv")
    by = {}
    for r in s1:
        by.setdefault(r["pair"], [r["group"], []])[1].append(ms[r["spec"]])
    ranked = sorted(by, key=lambda k: statistics.mean(by[k][1]))
    keep = ranked[:top] + [k for k in ranked[top:] if by[k][0] == "transplant"]
    rng = random.Random(1000 + int(opt("--seed", 1)))
    highs = [(rng.getrandbits(16), rng.getrandbits(16)) for _ in range(draws)]
    cores = {r["pair"]: (int(r["spec"].split()[1], 16) & M16, int(r["spec"].split()[3], 16) & M16) for r in s1}
    rows = []
    for k in keep:
        c0, c1 = cores[k]
        for h, (h0, h1) in enumerate(highs):
            rows.append(dict(spec=spec(h0 << 16 | c0, h1 << 16 | c1), pair=k, group=by[k][0],
                             draw=h, stage1_mean_ms=statistics.mean(by[k][1]),
                             stage1_rank=ranked.index(k) + 1))
    write(D / "stage2.csv", rows)
    screen(D / "stage2.csv", D / "screen2.csv", lg, 2)


def stage2_table():
    ms = scores(D / "screen2.csv")
    by = {}
    for r in read(D / "stage2.csv"):
        e = by.setdefault(r["pair"], dict(pair=r["pair"], group=r["group"],
                                          stage1_rank=int(r["stage1_rank"]), ms=[], specs=[]))
        e["ms"].append(ms[r["spec"]])
        e["specs"].append(r["spec"])
    for e in by.values():
        e["median_ms"] = statistics.median(e["ms"])
        e["best_ms"], e["best_spec"] = min(zip(e["ms"], e["specs"]))
    return sorted(by.values(), key=lambda e: e["median_ms"])


def exact():
    top = int(opt("--top", 40))
    t = stage2_table()
    pick = [e for e in t if e["group"] != "transplant"][:top] + [e for e in t if e["group"] == "transplant"][:5]
    (D / "finalists.txt").write_text("\n".join(e["best_spec"] for e in pick) + "\n")
    subprocess.run([str(MIXLAB), "exact", "--bits", "32", "--list", str(D / "finalists.txt"),
                    "--out", str(D / "exact.csv")], check=True)


def exact_scores():
    out = {}
    for p in [D / "exact.csv"] + sorted((D / "campaign").glob("*.done.json")):
        if p.suffix == ".csv" and p.exists():
            out.update({r["spec"]: float(r["bias"]) for r in read(p)})
    return out


def jobs():
    top, seeds = int(opt("--top", 10)), opt("--seeds", "1,2").split(",")
    ex = exact_scores()
    ntr = int(opt("--transplant", 3))  # same-budget control races from transplanted cores
    fin = sorted((e for e in stage2_table() if e["best_spec"] in ex), key=lambda e: ex[e["best_spec"]])
    nat = [e for e in fin if e["group"] != "transplant"][:top]
    tr = [e for e in fin if e["group"] == "transplant"][:ntr]
    tag = "T" + "_".join(map(str, TEMPLATE))
    t = "_".join(map(str, TEMPLATE))
    rows = []
    for s in seeds:
        for pre, lst in (("N", nat), ("X", tr)):
            for i, e in enumerate(lst):
                rows.append(dict(id=f"{pre}{t}_c{i}_s{s}", campaign=tag, kind="race",
                                 start=e["best_spec"], seed=s, args="--fix-shifts"))
    (D / "campaign").mkdir(exist_ok=True)
    write(D / "campaign" / "jobs.csv", rows)
    print(f"{len(rows)} race jobs -> {D / 'campaign' / 'jobs.csv'}")


def summary():
    t = stage2_table()
    print(f"template {'/'.join(map(str, TEMPLATE))}: {len(t)} pairs in stage 2")
    groups = {}
    for i, e in enumerate(t):
        groups.setdefault(e["group"].split("_")[0], []).append(i + 1)
    for g, ranks in groups.items():
        print(f"  {g:10s} n={len(ranks):3d}  best stage-2 rank {min(ranks)}, in top 40: {sum(r <= 40 for r in ranks)}")
    ex = exact_scores()
    print("\nexact finalists (best 15):")
    fin = [(ex[e["best_spec"]], e) for e in t if e["best_spec"] in ex]
    for b, e in sorted(fin, key=lambda x: x[0])[:15]:
        print(f"  {b:.5f}  {e['best_spec']}  {e['group']:10s} stage2 median ms {e['median_ms']:.3e}")
    tr = sorted(b for b, e in fin if e["group"] == "transplant")
    nat = sorted(b for b, e in fin if e["group"] != "transplant")
    if tr and nat:
        print(f"  best native {nat[0]:.5f} (median of top 10 {statistics.median(nat[:10]):.5f}); "
              f"best transplant {tr[0]:.5f} (median {statistics.median(tr):.5f})")
    done = sorted((D / "campaign").glob("*.done.json"))
    rows, below = [], []
    for p in done:
        j = json.loads(p.read_text())
        rows.append(j)
    if rows:
        for pre, name in (("N", "native"), ("X", "transplant")):
            bs = sorted(r["bias"] for r in rows if r["id"].startswith(pre))
            if bs:
                print(f"\n{name} races: {len(bs)} done, best {bs[0]:.5f}, median {statistics.median(bs):.5f}")
        for r in sorted(rows, key=lambda r: r["bias"])[:10]:
            print(f"  {r['bias']:.5f}  {r['best']}  from {r['start']} ({r['start_bias']:.5f}, {r['seconds']} s)")
        for r in rows:
            write(D / "races.csv", [dict(spec=x["best"], bias=x["bias"], start=x["start"],
                                         start_bias=x["start_bias"], id=x["id"]) for x in rows])
    allx = dict(ex)
    for r in rows:
        allx[r["best"]] = r["bias"]
    below = [dict(spec=s, bias=b) for s, b in sorted(allx.items(), key=lambda x: x[1]) if b < 0.10704]
    write(D / "below_10704.csv", below, ["spec", "bias"])
    print(f"\n{len(below)} exactly scored configs below 0.10704")


if __name__ == "__main__":
    cmd = sys.argv[1]
    if cmd == "gen":
        gen()
    elif cmd == "screen1":
        screen(D / "stage1.csv", D / "screen1.csv", int(opt("--log2", 22)), 1)
    elif cmd == "stage2":
        stage2()
    elif cmd == "exact":
        exact()
    elif cmd == "jobs":
        jobs()
    elif cmd == "summary":
        summary()
    else:
        sys.exit(__doc__)
