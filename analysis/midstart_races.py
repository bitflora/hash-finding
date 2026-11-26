"""Races started from random mid-distance mutants of a mixer (F48).

Each start flips exactly k random bits (never bit 0) of the centre's two
multipliers: bits 1-31 of both ("any") or bits 16-31 ("high"). Writes
CAMPAIGN_DIR/jobs.csv for analysis/campaign.py (kind race, --fix-shifts,
campaign.py's budget, as in F38), and summarises the finished races. Scores
all come from mixlab via campaign.py; this only picks starts and tabulates.

Usage:
  python analysis/midstart_races.py jobs [--per 8] [--seeds 1,2]
  CAMPAIGN_DIR=results/midstart_f30 python analysis/campaign.py run
  python analysis/midstart_races.py summary
"""
import argparse, csv, json, random, re, statistics, sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CAMP = ROOT / "results" / "midstart_f30"
CENTRE = "[16 4b0332d3 15 549a3597 15]"
ARMS = [("any", 4), ("any", 8), ("any", 12), ("any", 16), ("any", 24), ("high", 8), ("high", 16)]
BITS = {"any": range(1, 32), "high": range(16, 32)}


def jobs(per, seeds):
    s0, m0, s1, m1, s2 = re.findall(r"[0-9a-f]+", CENTRE)
    m = [int(m0, 16), int(m1, 16)]
    rng = random.Random(48)
    rows = []
    for region, k in ARMS:
        pos = [(w, b) for w in (0, 1) for b in BITS[region]]
        for i in range(per):
            x = m[:]
            for w, b in rng.sample(pos, k):
                x[w] ^= 1 << b
            start = f"[{s0} {x[0]:08x} {s1} {x[1]:08x} {s2}]"
            for seed in seeds:
                rows.append(dict(id=f"{region}{k}_i{i}_s{seed}", campaign=f"{region}{k}", kind="race",
                                 start=start, seed=seed, args="--fix-shifts"))
    CAMP.mkdir(parents=True, exist_ok=True)
    jf = CAMP / "jobs.csv"
    if jf.exists():
        sys.exit(f"{jf} exists; delete it to regenerate")
    with open(jf, "w", newline="") as f:
        w = csv.DictWriter(f, fieldnames=["id", "campaign", "kind", "start", "seed", "args"])
        w.writeheader(); w.writerows(rows)
    print(f"wrote {len(rows)} jobs to {jf}")


def popcount_dist(a, b):
    """Per-multiplier Hamming distances (all 32 bits, low 16 bits), each the
    smaller of the distance to y and to -y, so negation never counts."""
    pa = [int(x, 16) for x in re.findall(r"[0-9a-f]{8}", a)]
    pb = [int(x, 16) for x in re.findall(r"[0-9a-f]{8}", b)]
    d = lambda x, y, mask: min(bin((x ^ y) & mask).count("1"), bin((x ^ y ^ 0xfffffffe) & mask).count("1"))
    return [d(x, y, 0xffffffff) for x, y in zip(pa, pb)], [d(x, y, 0xffff) for x, y in zip(pa, pb)]


def summary():
    recs = [json.loads(p.read_text()) for p in sorted(CAMP.glob("*.done.json"))]
    by = {}
    for r in recs:
        by.setdefault(r["campaign"], []).append(r)
    print("arm      races  start median  best      median    worst    back-to-F30-core  best spec")
    for region, k in ARMS:
        rs = sorted(by.get(f"{region}{k}", []), key=lambda r: r["bias"])
        if not rs:
            continue
        b = [r["bias"] for r in rs]
        home = sum(sum(popcount_dist(r["best"], CENTRE)[1]) == 0 for r in rs)
        print(f"{region}{k:<5d} {len(rs):5d}  {statistics.median(r['start_bias'] for r in rs):12.4f}  "
              f"{b[0]:.5f}  {statistics.median(b):.5f}  {b[-1]:.5f}  {home:3d}/{len(rs):<3d}  {rs[0]['best']}")
    allr = sorted(recs, key=lambda r: r["bias"])[:8]
    print("best overall:")
    for r in allr:
        d, dl = popcount_dist(r["best"], CENTRE)
        print(f"  {r['best']} {r['bias']:.5f}  from {r['id']}  dist to F30 all={sum(d)} low16={sum(dl)}")


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("cmd", choices=["jobs", "summary"])
    ap.add_argument("--per", type=int, default=8)
    ap.add_argument("--seeds", default="1,2")
    a = ap.parse_args()
    jobs(a.per, [int(s) for s in a.seeds.split(",")]) if a.cmd == "jobs" else summary()
