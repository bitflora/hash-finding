"""Validate candidate 64-bit metrics at 32 bits.

Joins results/metric_validation/set.csv (exact bias) with metrics_s*.csv from
`mixlab metrics` (harness/src/metrics.rs) and reports Spearman rho of each metric
with exact bias over all mixers, over bias < 0.2, and over bias < 0.12;
plus the metrics of the three PractRand-tested mixers.

Usage: python analysis/metric_validation.py [metrics_s1.csv ...]
"""
import csv
import sys
from pathlib import Path

import numpy as np

sys.path.insert(0, str(Path(__file__).parent))
from shared_props32 import spearman  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
D = ROOT / "results/metric_validation"
METRICS = ["ctr", "rev", "hi", "adj2", "add", "sac", "bic_rmsz", "bic_worst_corr"]
PRACTRAND = {  # bit-reversed counter, 512 MB, tests with no anomalies (FINDINGS F21/F26)
    "[16 21f0aaad 15 735a2d97 15]": ("old record", 111),
    "[16 9c013335 15 335a2d97 15]": ("F21", 122),
    "[16 e00332d3 15 549a3597 15]": ("F26", 126),
}


def main():
    files = sys.argv[1:] or [str(D / "metrics_s1.csv")]
    bias = {r["spec"]: float(r["exact_bias"]) for r in csv.DictReader(open(D / "set.csv"))}
    rows = {}
    for f in files:
        for r in csv.DictReader(open(f)):
            rows[r["spec"]] = r
    specs = [s for s in rows if s in bias]
    y = np.array([bias[s] for s in specs])
    X = {m: np.array([float(rows[s][m]) for s in specs]) for m in METRICS}
    print(f"n = {len(specs)} mixers (bias {y.min():.4f} .. {y.max():.2f})")
    print(f"{'metric':15s} {'rho all':>8s} {'<0.2':>7s} {'<0.12':>7s}   median (all / <0.12)")
    for m in METRICS:
        a = spearman(X[m], y)
        g2, g12 = y < 0.2, y < 0.12
        print(f"{m:15s} {a:8.3f} {spearman(X[m][g2], y[g2]):7.3f} {spearman(X[m][g12], y[g12]):7.3f}"
              f"   {np.median(X[m]):8.3f} / {np.median(X[m][g12]):.3f}   (n<0.2={g2.sum()}, n<0.12={g12.sum()})")
    print("\nPractRand-tested mixers (bit-reversed counter):")
    for s, (name, clean) in PRACTRAND.items():
        if s in rows:
            print(f"  {name:10s} bias {bias[s]:.5f}  clean tests {clean}  " +
                  "  ".join(f"{m}={float(rows[s][m]):.3g}" for m in METRICS))
    print("\nWorst 12 by adj2 among bias < 0.2:")
    good = [s for s in specs if bias[s] < 0.2]
    for s in sorted(good, key=lambda s: -float(rows[s]["adj2"]))[:12]:
        print(f"  {s}  bias {bias[s]:.5f}  adj2 {float(rows[s]['adj2']):8.2f}  add {float(rows[s]['add']):7.2f}"
              f"  bic {float(rows[s]['bic_worst_corr']):.3f}")


if __name__ == "__main__":
    main()
