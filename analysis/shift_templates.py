"""Plan A (plan.md): compare 2-round 32-bit shift templates [s0 _ s1 _ s2]
under an equal search budget. Python only chooses configs and reads mixlab's
output; every score comes from the harness (CLAUDE.md).

Steps (state lives in results/shift_templates/, override with SHIFT_DIR):

  python analysis/shift_templates.py starts [--random 64] [--cores 20] [--seed 1]
      A1 starts: for every template s0 in 13..18, s1 in 12..17, s2 in 12..15
      (s2 <= 15 by F11), N random odd multiplier pairs plus the best known
      low-16-bit core pairs (from results/explored32.csv) with random high
      halves. Writes starts.csv (spec,template,group,core).
  python analysis/shift_templates.py screen [--log2 22] [--seed 1]
      Sampled bias of every start (examples/screen_list, resumable) and the
      F4 surrogate (mixlab surrogate --score-list). Writes screen_p<k>.csv and
      surrogate.csv.
  python analysis/shift_templates.py rank [--log2 22]
      Per-template statistics of the screen; writes templates_ranked.csv.
      Ranked by the 10th-percentile sampled ms over all starts, then best.
  python analysis/shift_templates.py jobs [--top 6] [--per 3] [--seeds 1,2] [--log2 22]
                                          [--templates T,T,...] [--group random|core]
      A2: race jobs for the top templates plus the 16/15/15 control, all with
      --fix-shifts and the same campaign budget. Writes campaign/jobs.csv,
      interleaved round-robin so a paused campaign stays balanced.
      --templates replaces the top-N choice with an explicit list; --group
      restricts every template to one kind of start and takes its --per best
      starts of that kind by the screen (F38: random starts only, so the
      control gets no cores tuned at 16/15/15).
      Run with:  CAMPAIGN_DIR=results/shift_templates/campaign python analysis/campaign.py run [--hours H]
      (campaign.py's defaults: 25 iterations, patience 6, 96 neighbours).
  python analysis/shift_templates.py summary [--bic-seeds 1,2,3,4,5]
      A2/A4 report: per-template race winners (exact), best/median, and BIC at
      2^20 over several seeds for each template's best winner, including the
      worst pair's location. Writes template_summary.csv, and below_10704.csv:
      every exactly scored race incumbent (start or accepted step) below
      0.10704, TheIronBorn's published best, as seeds for later searches.

Builds needed: cargo build --release --bin mixlab --example screen_list
"""
import csv, json, os, random, re, subprocess, sys
from collections import defaultdict
from pathlib import Path
from statistics import median

ROOT = Path(__file__).resolve().parent.parent
# Absolute, because mixlab runs with cwd=harness/ and gets paths under D.
D = Path(os.environ.get("SHIFT_DIR", ROOT / "results" / "shift_templates")).resolve()
# CAMPAIGN_DIR (the variable campaign.py reads) selects another race campaign
# over the same starts; its summary files then go into that directory.
CAMP = Path(os.environ.get("CAMPAIGN_DIR", D / "campaign")).resolve()
OUT = D if CAMP == (D / "campaign").resolve() else CAMP
EXE = ".exe" if os.name == "nt" else ""
MIXLAB = ROOT / "harness" / "target" / "release" / f"mixlab{EXE}"
SCREEN = ROOT / "harness" / "target" / "release" / "examples" / f"screen_list{EXE}"
S0, S1, S2 = range(13, 19), range(12, 18), range(12, 16)
CONTROL = (16, 15, 15)
SPEC_RE = re.compile(r"\[\s*(\d+)\s+([0-9a-fA-F]+)\s+(\d+)\s+([0-9a-fA-F]+)\s+(\d+)\s*\]")


def opt(name, default):
    return sys.argv[sys.argv.index(name) + 1] if name in sys.argv else default


def parse(spec):
    m = SPEC_RE.fullmatch(spec.strip())
    if not m:
        sys.exit(f"bad spec {spec!r}")
    s0, m0, s1, m1, s2 = m.groups()
    return int(s0), int(m0, 16), int(s1), int(m1, 16), int(s2)


def tkey(t):
    return f"{t[0]}/{t[1]}/{t[2]}"


def known_cores(n):
    """Best distinct low-16-bit core pairs among exactly scored mixers,
    one per cluster (pairs within Hamming distance 4 are one cluster)."""
    best = {}
    for r in csv.DictReader(open(ROOT / "results" / "explored32.csv", newline="")):
        _, m0, _, m1, _ = parse(r["spec"])
        k = (m0 & 0xFFFF, m1 & 0xFFFF)
        b = float(r["bias"])
        if k not in best or b < best[k]:
            best[k] = b
    hd = lambda a, b: bin(a[0] ^ b[0]).count("1") + bin(a[1] ^ b[1]).count("1")
    kept = []
    for k, _ in sorted(best.items(), key=lambda kv: kv[1]):
        if not any(hd(k, c) <= 4 for c in kept):
            kept.append(k)
        if len(kept) == n:
            break
    return kept


def starts():
    n_rand, n_cores, seed = int(opt("--random", 64)), int(opt("--cores", 20)), int(opt("--seed", 1))
    D.mkdir(parents=True, exist_ok=True)
    out = D / "starts.csv"
    if out.exists():
        sys.exit(f"{out} exists; delete it to regenerate (screens keyed to it would go stale)")
    cores = known_cores(n_cores)
    rows = []
    for t in [(a, b, c) for a in S0 for b in S1 for c in S2]:
        rng = random.Random(f"{seed}-{tkey(t)}")
        for _ in range(n_rand):
            m0, m1 = rng.getrandbits(32) | 1, rng.getrandbits(32) | 1
            rows.append((f"[{t[0]} {m0:08x} {t[1]} {m1:08x} {t[2]}]", tkey(t), "random", ""))
        for l0, l1 in cores:
            m0, m1 = rng.getrandbits(16) << 16 | l0, rng.getrandbits(16) << 16 | l1
            rows.append((f"[{t[0]} {m0:08x} {t[1]} {m1:08x} {t[2]}]", tkey(t), "core", f"{l0:04x}/{l1:04x}"))
    with open(out, "w", newline="") as f:
        w = csv.writer(f)
        w.writerow(["spec", "template", "group", "core"])
        w.writerows(rows)
    print(f"wrote {len(rows)} starts ({len(S0) * len(S1) * len(S2)} templates, {len(cores)} cores) to {out}")


def screen():
    lg, seed = int(opt("--log2", 22)), int(opt("--seed", 1))
    for b in (MIXLAB, SCREEN):
        if not b.exists():
            sys.exit(f"missing {b}; cargo build --release --bin mixlab --example screen_list")
    subprocess.run([str(SCREEN), str(D / "starts.csv"), str(D / f"screen_p{lg}.csv"), str(lg), str(seed)], check=True)
    subprocess.run([str(MIXLAB), "surrogate", "--bits", "32", "--score-list", str(D / "starts.csv"),
                    "--out", str(D / "surrogate.csv")], check=True)


def q(xs, p):
    xs = sorted(xs)
    return xs[min(len(xs) - 1, int(p * len(xs)))] if xs else float("nan")


def load_screen(lg):
    meta = {r["spec"]: r for r in csv.DictReader(open(D / "starts.csv", newline=""))}
    ms = {r["spec"]: float(r["sampled_ms"]) for r in csv.DictReader(open(D / f"screen_p{lg}.csv", newline=""))}
    sur = {r["spec"]: float(r["surrogate_bias"]) for r in csv.DictReader(open(D / "surrogate.csv", newline=""))}
    missing = len(meta) - len(ms.keys() & meta.keys())
    if missing:
        sys.exit(f"screen incomplete: {missing} starts unscored; rerun `screen`")
    return meta, ms, sur


def rank():
    lg = int(opt("--log2", 22))
    meta, ms, sur = load_screen(lg)
    by = defaultdict(list)
    for s, r in meta.items():
        by[r["template"]].append((ms[s], r["group"], sur[s]))
    rows = []
    for t, v in by.items():
        allm = [m for m, _, _ in v]
        rows.append(dict(template=t, n=len(v), best_ms=min(allm), p10_ms=q(allm, 0.10), median_ms=median(allm),
                         p10_random_ms=q([m for m, g, _ in v if g == "random"], 0.10),
                         p10_core_ms=q([m for m, g, _ in v if g == "core"], 0.10),
                         surrogate_median=median(s for _, _, s in v), surrogate_best=min(s for _, _, s in v)))
    rows.sort(key=lambda r: (r["p10_ms"], r["best_ms"]))
    sr = sorted(rows, key=lambda r: r["surrogate_median"])
    for i, r in enumerate(rows):
        r["rank"] = i + 1
    for i, r in enumerate(sr):
        r["surrogate_rank"] = i + 1
    with open(D / "templates_ranked.csv", "w", newline="") as f:
        w = csv.DictWriter(f, fieldnames=list(rows[0].keys()))
        w.writeheader()
        w.writerows(rows)
    print(f"sampled ms at 2^{lg}; ranked by p10 over all starts (lower is better)")
    print(f"{'rank':>4} {'template':<9} {'best':>10} {'p10':>10} {'p10 rnd':>10} {'p10 core':>10} {'sur med':>8} {'sur rk':>6}")
    ctl = tkey(CONTROL)
    for r in rows:
        if r["rank"] <= 20 or r["template"] == ctl:
            print(f"{r['rank']:>4} {r['template']:<9} {r['best_ms']:10.3e} {r['p10_ms']:10.3e} {r['p10_random_ms']:10.3e} "
                  f"{r['p10_core_ms']:10.3e} {r['surrogate_median']:8.2f} {r['surrogate_rank']:>6}" + ("  <- control" if r["template"] == ctl else ""))


def jobs():
    top, per, lg = int(opt("--top", 6)), int(opt("--per", 3)), int(opt("--log2", 22))
    seeds = [int(s) for s in opt("--seeds", "1,2").split(",")]
    meta, ms, _ = load_screen(lg)
    group = opt("--group", None)
    if "--templates" in sys.argv:
        chosen = opt("--templates", "").split(",")
    else:
        ranked = [r["template"] for r in csv.DictReader(open(D / "templates_ranked.csv", newline=""))]
        ctl = tkey(CONTROL)
        chosen = [t for t in ranked if t != ctl][:top] + [ctl]
    plan = {}
    for t in chosen:
        cand = sorted((s for s, r in meta.items() if r["template"] == t and group in (None, r["group"])),
                      key=lambda s: ms[s])
        if not cand:
            sys.exit(f"no starts for template {t} (group {group})")
        # Best core start, best random start, then the best remaining starts
        # from distinct cores, so one template's races do not share a core.
        picks, used = [], set()
        for want in ("core", "random", None):
            for s in cand:
                g, c = meta[s]["group"], meta[s]["core"] or s
                if (want is None or g == want) and c not in used:
                    picks.append(s); used.add(c)
                    break
        for s in cand:
            if len(picks) >= per:
                break
            c = meta[s]["core"] or s
            if c not in used:
                picks.append(s); used.add(c)
        plan[t] = picks[:per]
    CAMP.mkdir(parents=True, exist_ok=True)
    jf = CAMP / "jobs.csv"
    if jf.exists():
        sys.exit(f"{jf} exists; delete it to regenerate")
    rows = []
    for seed in seeds:  # round-robin: every template advances together
        for k in range(per):
            for t in chosen:
                if k < len(plan[t]):
                    s0, s1, s2 = t.split("/")
                    rows.append(dict(id=f"T{s0}_{s1}_{s2}_k{k}_s{seed}", campaign=f"T{s0}_{s1}_{s2}", kind="race",
                                     start=plan[t][k], seed=seed, args="--fix-shifts"))
    with open(jf, "w", newline="") as f:
        w = csv.DictWriter(f, fieldnames=["id", "campaign", "kind", "start", "seed", "args"])
        w.writeheader()
        w.writerows(rows)
    print(f"wrote {len(rows)} race jobs for {len(chosen)} templates ({', '.join(chosen)}) to {jf}")
    print(f"run: CAMPAIGN_DIR={CAMP} python analysis/campaign.py run [--hours H]")


def bic_runs(spec, seeds):
    out = subprocess.run([str(MIXLAB), "bic", "--bits", "32", "--spec", spec, "--samples", "p20", "--seeds", seeds],
                         capture_output=True, text=True, cwd=ROOT / "harness", check=True).stdout
    rx = re.compile(r"max\|z\|=([0-9.]+).*worst: in (\d+) -> out (\d+),(\d+) corr ([+-][0-9.]+)")
    return [(float(m[1]), int(m[2]), int(m[3]), int(m[4]), float(m[5])) for m in rx.finditer(out)]


KEEP_BELOW = 0.10704  # TheIronBorn's published best; anything under it is kept as a seed


def below_threshold():
    """Every exact incumbent in the campaign's race CSVs (iter,event,config,score;
    with bits 32 and no --verify-samples, start and accepted scores are exact)."""
    best = {}
    for p in sorted(CAMP.glob("T*.csv")):
        for line in p.read_text().splitlines():
            f = line.split(",")
            if len(f) >= 4 and f[1] in ("start", "improved"):
                b = float(f[3])
                if b < KEEP_BELOW and (f[2] not in best or b < best[f[2]][0]):
                    best[f[2]] = (b, p.stem)
    rows = sorted((b, spec, job) for spec, (b, job) in best.items())
    with open(OUT / "below_10704.csv", "w", newline="") as f:
        w = csv.writer(f)
        w.writerow(["spec", "bias", "template", "job"])
        for b, spec, job in rows:
            s0, _, s1, _, s2 = parse(spec)
            w.writerow([spec, f"{b:.10f}", f"{s0}/{s1}/{s2}", job])
    per = defaultdict(int)
    for _, spec, _ in rows:
        s0, _, s1, _, s2 = parse(spec)
        per[f"{s0}/{s1}/{s2}"] += 1
    print()
    print(f"{len(rows)} distinct mixers below {KEEP_BELOW} -> {OUT / 'below_10704.csv'}"
          + ("" if not per else ": " + ", ".join(f"{t} {n}" for t, n in sorted(per.items(), key=lambda kv: -kv[1]))))


def summary():
    seeds = opt("--bic-seeds", "1,2,3,4,5")
    recs = [json.loads(p.read_text()) for p in sorted(CAMP.glob("*.done.json"))]
    if not recs:
        sys.exit(f"no finished races in {CAMP}")
    by = defaultdict(list)
    for r in recs:
        by[r["campaign"]].append(r)
    rows = []
    for c, rs in by.items():
        b = [r["bias"] for r in rs]
        win = min(rs, key=lambda r: r["bias"])
        bic = bic_runs(win["best"], seeds)
        locs = sorted({(i, j, k) for _, i, j, k, _ in bic})
        rows.append(dict(template=c[1:].replace("_", "/"), races=len(rs), best=min(b), median=median(b),
                         worst=max(b), best_spec=win["best"],
                         bic_max_z=max(z for z, *_ in bic), bic_worst_abs_corr=max(abs(x[4]) for x in bic),
                         bic_worst_pairs=" ".join(f"{i}->{j},{k}" for i, j, k in locs)))
    rows.sort(key=lambda r: r["best"])
    with open(OUT / "template_summary.csv", "w", newline="") as f:
        w = csv.DictWriter(f, fieldnames=list(rows[0].keys()))
        w.writeheader()
        w.writerows(rows)
    print(f"exact bias of race winners; BIC at 2^20, seeds {seeds}, for each template's best winner")
    print(f"{'template':<9} {'n':>2} {'best':>9} {'median':>9} {'worst':>9} {'best spec':<30} {'max|z|':>7} {'|corr|':>6}  worst pairs")
    for r in rows:
        print(f"{r['template']:<9} {r['races']:>2} {r['best']:9.5f} {r['median']:9.5f} {r['worst']:9.5f} {r['best_spec']:<30} "
              f"{r['bic_max_z']:7.1f} {r['bic_worst_abs_corr']:6.3f}  {r['bic_worst_pairs']}")
    below_threshold()


if __name__ == "__main__":
    cmds = dict(starts=starts, screen=screen, rank=rank, jobs=jobs, summary=summary)
    if len(sys.argv) < 2 or sys.argv[1] not in cmds:
        sys.exit(__doc__)
    cmds[sys.argv[1]]()
