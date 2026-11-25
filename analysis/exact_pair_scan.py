"""Is an exact BIC output pair (|corr| = 1) structural for a shift template?

For every template s0 in 13..18, s1 in 12..17, s2 in 12..15 (the Plan A grid),
draw N random odd multiplier pairs and run `mixlab bic` (sampled, 2^16 inputs,
one seed). A template whose every draw shows |corr| = 1.000 has an exact pair
forced by its shifts (in the spirit of F3/F11); a template where only some draws
show it has a multiplier-dependent defect. Python only parses mixlab output.

Usage (from the repository root):
  python analysis/exact_pair_scan.py [--n 16] [--seed 1] [--out results/exact_pair_scan]
Writes bic.csv (one row per spec) and templates.csv (per-template summary).
"""
import csv, os, random, re, subprocess, sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
EXE = ".exe" if os.name == "nt" else ""
MIXLAB = ROOT / "harness" / "target" / "release" / f"mixlab{EXE}"
LINE = re.compile(r"max\|z\|=([\d.]+).*worst: in (\d+) -> out (\d+),(\d+) corr ([-+\d.]+)")


def opt(name, default):
    return sys.argv[sys.argv.index(name) + 1] if name in sys.argv else default


def main():
    n, seed = int(opt("--n", 16)), int(opt("--seed", 1))
    out = ROOT / opt("--out", "results/exact_pair_scan")
    out.mkdir(parents=True, exist_ok=True)
    rng = random.Random(seed)
    rows = []
    for s0 in range(13, 19):
        for s1 in range(12, 18):
            for s2 in range(12, 16):
                for _ in range(n):
                    m0, m1 = rng.getrandbits(32) | 1, rng.getrandbits(32) | 1
                    spec = f"[{s0} {m0:08x} {s1} {m1:08x} {s2}]"
                    r = subprocess.run([str(MIXLAB), "bic", "--bits", "32", "--spec", spec,
                                        "--samples", "p16", "--seeds", "1"],
                                       capture_output=True, text=True, check=True)
                    m = LINE.search(r.stdout)
                    if not m:
                        sys.exit(f"unparsed: {r.stdout!r}")
                    z, i, j, k, c = m.groups()
                    rows.append(dict(template=f"{s0}/{s1}/{s2}", spec=spec, max_z=z,
                                     worst_in=i, out_j=j, out_k=k, corr=c))
    with open(out / "bic.csv", "w", newline="") as f:
        w = csv.DictWriter(f, fieldnames=list(rows[0]))
        w.writeheader()
        w.writerows(rows)
    summ = {}
    for r in rows:
        t = summ.setdefault(r["template"], dict(template=r["template"], draws=0, exact=0, pairs=set()))
        t["draws"] += 1
        if abs(float(r["corr"])) >= 0.9999:
            t["exact"] += 1
            t["pairs"].add(f"{r['worst_in']}->{r['out_j']},{r['out_k']}")
    with open(out / "templates.csv", "w", newline="") as f:
        w = csv.writer(f)
        w.writerow(["template", "draws", "exact_pair_draws", "exact_pairs_seen"])
        for t in summ.values():
            w.writerow([t["template"], t["draws"], t["exact"], " ".join(sorted(t["pairs"]))])
    allx = [t["template"] for t in summ.values() if t["exact"] == t["draws"]]
    some = [t["template"] for t in summ.values() if 0 < t["exact"] < t["draws"]]
    print(f"{len(summ)} templates x {n} draws; exact pair in every draw: {len(allx)}; in some: {len(some)}")
    print("always:", " ".join(allx))
    print("sometimes:", " ".join(some))


if __name__ == "__main__":
    main()
