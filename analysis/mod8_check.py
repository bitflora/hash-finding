"""Check two elite-mixer properties on independent random populations:
m mod 8 class (+-1 vs +-3, negation-invariant) and NAF weight.
Datasets: random 16-bit (surrogate_random16.csv, 5000 exact), random 24-bit
(scale_random24.csv group=random, 1200 exact), published 32-bit (116 exact)."""
import csv, re, sys
from pathlib import Path
import numpy as np
sys.path.insert(0, str(Path(__file__).parent))
from shared_props32 import naf_weight, spearman
ROOT = Path(__file__).resolve().parent.parent
SPEC = re.compile(r"\[(\d+) ([0-9a-f]+) (\d+) ([0-9a-f]+) (\d+)\]")

def load(path, col, filt=None, width=None):
    out = []
    for r in csv.DictReader(open(ROOT / path)):
        if filt and not filt(r):
            continue
        k = r.get("config") or r["spec"]
        m = SPEC.search(k)
        out.append((int(m[1]), int(m[2], 16), int(m[3]), int(m[4], 16), int(m[5]), float(r[col])))
    return out

def cls(m):
    return "+-1" if (m & 7) in (1, 7) else "+-3"

def report(name, rows, bits):
    y = np.array([r[5] for r in rows])
    q10 = np.quantile(y, 0.10)
    print(f"\n== {name}: n={len(rows)}, median {np.median(y):.4g}, 10% cut {q10:.4g}")
    print("  m0 cls / m1 cls :   n   median   share in best 10%")
    for a in ("+-1", "+-3"):
        for b in ("+-1", "+-3"):
            sel = np.array([cls(r[1]) == a and cls(r[3]) == b for r in rows])
            if sel.sum():
                print(f"  {a:>5s} / {b:5s}   : {sel.sum():4d}  {np.median(y[sel]):8.4g}   {np.mean(y[sel] <= q10):.3f}")
    for t, i in (("m0", 1), ("m1", 3)):
        naf = np.array([naf_weight(r[i]) for r in rows])
        good = y <= np.quantile(y, 0.5)
        print(f"  {t} NAF/bits: rho(all) {spearman(naf, y):+.3f}, rho(best half) {spearman(naf[good], y[good]):+.3f}, "
              f"mean best10% {naf[y<=q10].mean()/bits:.3f} vs all {naf.mean()/bits:.3f}")

report("random 16-bit", load("results/surrogate_random16.csv", "exact"), 16)
report("random 24-bit", load("results/scale_random24.csv", "exact_bias", lambda r: r["group"] == "random"), 24)
report("published 32-bit", load("results/published_2round32.csv", "exact_bias"), 32)
