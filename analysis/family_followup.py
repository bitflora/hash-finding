"""Multi-seed follow-up of the two new F43 families (FINDINGS F45).

Writes results/family_followup/jobs.csv for analysis/campaign.py. Per family,
three arms, all with --fix-shifts and campaign.py's race budget:
  R  restart from the sweep's original start, seeds 2-9 (how much was luck?)
  C  continue from the sweep's winner, seeds 2-9 (is the winner a local optimum
     for other neighbour samples?)
  K  the winner's low-16-bit core with fresh high halves: campaign.py's "core"
     kind exact-scores DRAWS random high halves and races the best (F16/F19
     statistic); 4 jobs per family, each with its own draws.
Every score comes from mixlab via campaign.py (CLAUDE.md).

Usage:
  python analysis/family_followup.py jobs
  CAMPAIGN_DIR=results/family_followup python analysis/campaign.py run
  python analysis/family_followup.py summary
"""
import csv, json, statistics, sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
D = ROOT / "results" / "family_followup"
FAMILIES = {  # name: (sweep start, sweep winner, winner bias)
    "2ab3_c9a9": ("[16 c29fba97 15 32b2c9ed 15]", "[16 e63f2ab3 15 7291c9a9 15]", 0.1039426216),
    "cab5_ad19": ("[16 0f02d5b5 15 c2b9add9 15]", "[16 9340cab5 15 a2b9ad19 15]", 0.106718619),
}
SEEDS = range(2, 10)
CORE_JOBS = 4


def jobs():
    rows = []
    for fam, (start, win, _) in FAMILIES.items():
        for s in SEEDS:
            rows.append(dict(id=f"R_{fam}_s{s}", campaign=fam, kind="race", start=start, seed=s, args="--fix-shifts"))
            rows.append(dict(id=f"C_{fam}_s{s}", campaign=fam, kind="race", start=win, seed=s, args="--fix-shifts"))
        _, m0, _, m1, _ = win.strip("[]").split()
        for k in range(CORE_JOBS):
            rows.append(dict(id=f"K_{fam}_d{k}", campaign=fam, kind="core",
                             start=f"16 {m0[4:]} 15 {m1[4:]} 15", seed=61 + k, args="--fix-shifts"))
    D.mkdir(parents=True, exist_ok=True)
    jf = D / "jobs.csv"
    if jf.exists():
        sys.exit(f"{jf} exists; delete it to regenerate")
    with open(jf, "w", newline="") as f:
        w = csv.DictWriter(f, fieldnames=["id", "campaign", "kind", "start", "seed", "args"])
        w.writeheader()
        w.writerows(rows)
    print(f"wrote {len(rows)} jobs to {jf}")


def summary():
    recs = [json.loads(p.read_text()) for p in sorted(D.glob("*.done.json"))]
    print(f"{'family':<10} {'arm':<3} {'n':>2} {'best':>9} {'median':>9} {'worst':>9}  best spec")
    for fam, (_, win, wb) in FAMILIES.items():
        print(f"{fam:<10} sweep winner {wb:.5f} {win}")
        for arm in "RCK":
            rs = [r for r in recs if r["id"].startswith(f"{arm}_{fam}_")]
            if not rs:
                continue
            b = sorted(r["bias"] for r in rs)
            best = min(rs, key=lambda r: r["bias"])
            print(f"{fam:<10} {arm:<3} {len(rs):>2} {b[0]:9.5f} {statistics.median(b):9.5f} {b[-1]:9.5f}  {best['best']}")
    allb = sorted({(r["bias"], r["best"]) for r in recs})
    below = [x for x in allb if x[0] < 0.10704]
    print(f"\n{len(below)} distinct race winners below 0.10704")
    with open(D / "winners.csv", "w", newline="") as f:
        w = csv.writer(f)
        w.writerow(["id", "spec", "bias", "start", "start_bias"])
        for r in sorted(recs, key=lambda r: r["bias"]):
            w.writerow([r["id"], r["best"], r["bias"], r["start"], r.get("start_bias")])


if __name__ == "__main__":
    cmds = dict(jobs=jobs, summary=summary)
    if len(sys.argv) < 2 or sys.argv[1] not in cmds:
        sys.exit(__doc__)
    cmds[sys.argv[1]]()
