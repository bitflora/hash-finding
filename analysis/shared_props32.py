"""Which multiplier properties do good 2-round 32-bit xmx mixers share?

Reads results/explored32.csv (every exactly scored mixer) and computes
per-multiplier and pair features. Reports, for the 16/15/15 template:
  * Spearman rho of each feature with exact bias over all mixers and over one
    representative (best) per low-16-bit core pair (reduces lineage
    duplication),
  * feature distributions of the elite (bias < 0.11, one per core pair)
    against random odd 32-bit multipliers,
  * per-bit 1-frequency of elite multipliers (pattern mining), deduplicated.

Usage: python analysis/shared_props32.py [--elite 0.11]
"""
import csv
import sys
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parent.parent
M32 = (1 << 32) - 1


def popcount(x):
    return bin(x).count("1")


def naf_weight(x):
    """Number of nonzero digits in the non-adjacent form of x (as an integer)."""
    w = 0
    while x:
        if x & 1:
            d = 2 - (x & 3)  # +1 or -1
            x -= d
            w += 1
        x >>= 1
    return w


def inverse(m):
    return pow(m, -1, 1 << 32)


def longest_run(x, bits=32):
    best = cur = 0
    prev = None
    for i in range(bits):
        b = (x >> i) & 1
        cur = cur + 1 if b == prev else 1
        prev = b
        best = max(best, cur)
    return best


def spectral2(m, mod=1 << 32):
    """Shortest nonzero vector length of the 2D lattice {(a, b): b = a*m mod
    mod}, normalised by sqrt(mod) (Gauss reduction). Higher = better LCG."""
    u = np.array([1, m], dtype=object)
    v = np.array([0, mod], dtype=object)
    def n2(w):
        return w[0] * w[0] + w[1] * w[1]
    if n2(u) > n2(v):
        u, v = v, u
    while True:
        q = (u[0] * v[0] + u[1] * v[1]) / n2(u)
        q = int(round(q))
        v = v - q * u
        if n2(v) >= n2(u):
            break
        u, v = v, u
    return float(np.sqrt(float(n2(u)))) / np.sqrt(float(mod))


def mult_feats(m, tag):
    neg = (-m) & M32
    inv = inverse(m)
    return {
        f"{tag}_pop": popcount(m),
        f"{tag}_pop_lo16": popcount(m & 0xFFFF),
        f"{tag}_pop_hi16": popcount(m >> 16),
        f"{tag}_naf": naf_weight(m),
        f"{tag}_naf_min_neg": min(naf_weight(m), naf_weight(neg)),
        f"{tag}_trans": popcount((m ^ (m >> 1)) & 0x7FFFFFFF),
        f"{tag}_run": longest_run(m),
        f"{tag}_mod8": m & 7,
        f"{tag}_inv_pop": popcount(inv),
        f"{tag}_inv_naf": naf_weight(inv),
        f"{tag}_spec32": spectral2(m),
        f"{tag}_spec16": spectral2(m & 0xFFFF, 1 << 16),
        f"{tag}_top": m >> 31,
    }


def pair_feats(s0, m0, s1, m1, s2):
    f = {}
    f.update(mult_feats(m0, "m0"))
    f.update(mult_feats(m1, "m1"))
    p = (m0 * m1) & M32
    f["prod_pop"] = popcount(p)
    f["prod_naf"] = naf_weight(p)
    f["xor_pop"] = popcount(m0 ^ m1)
    f["xor_lo16_pop"] = popcount((m0 ^ m1) & 0xFFFF)
    return f


def rankdata(a):
    a = np.asarray(a, dtype=float)
    order = a.argsort(kind="mergesort")
    r = np.empty(len(a))
    sa = a[order]
    i = 0
    while i < len(a):
        j = i
        while j + 1 < len(a) and sa[j + 1] == sa[i]:
            j += 1
        r[order[i:j + 1]] = (i + j) / 2.0
        i = j + 1
    return r


def spearman(x, y):
    rx, ry = rankdata(x), rankdata(y)
    if rx.std() == 0 or ry.std() == 0:
        return float("nan")
    return float(np.corrcoef(rx, ry)[0, 1])


def main():
    elite_cut = 0.11
    if "--elite" in sys.argv:
        elite_cut = float(sys.argv[sys.argv.index("--elite") + 1])
    rows = []
    for r in csv.DictReader(open(ROOT / "results/explored32.csv")):
        s = (int(r["s0"]), int(r["s1"]), int(r["s2"]))
        if s != (16, 15, 15):
            continue
        m0, m1 = int(r["m0"], 16), int(r["m1"], 16)
        rows.append((m0, m1, float(r["bias"])))
    print(f"16/15/15 exact mixers: {len(rows)}")

    # one representative (best) per low-16-bit core pair
    best = {}
    for m0, m1, b in rows:
        k = (m0 & 0xFFFF, m1 & 0xFFFF)
        if k not in best or b < best[k][2]:
            best[k] = (m0, m1, b)
    dedup = list(best.values())
    print(f"distinct core pairs: {len(dedup)}")

    def table(rs):
        feats = [pair_feats(16, m0, 15, m1, 15) for m0, m1, _ in rs]
        keys = list(feats[0])
        X = {k: np.array([f[k] for f in feats], float) for k in keys}
        y = np.array([b for *_, b in rs])
        return keys, X, y

    keys, Xa, ya = table(rows)
    _, Xd, yd = table(dedup)
    print("\nSpearman rho with exact bias (negative = higher feature, lower bias)")
    print(f"{'feature':18s} {'all':>7s} {'dedup':>7s}  {'dedup<0.2':>9s}")
    good = yd < 0.2
    for k in keys:
        print(f"{k:18s} {spearman(Xa[k], ya):7.3f} {spearman(Xd[k], yd):7.3f}  "
              f"{spearman(Xd[k][good], yd[good]):9.3f}")

    # elite vs random odd multipliers
    rng = np.random.default_rng(1)
    rand = [(int(a) | 1, int(b) | 1, 0.0) for a, b in
            rng.integers(0, 1 << 32, size=(20000, 2), dtype=np.uint64)]
    _, Xr, _ = table(rand)
    elite = [r for r in dedup if r[2] < elite_cut]
    _, Xe, ye = table(elite)
    print(f"\nElite (bias < {elite_cut}, one per core pair): n = {len(elite)}")
    print(f"{'feature':18s} {'elite mean':>10s} {'random mean':>11s} {'rand sd':>8s} {'z':>6s}")
    for k in keys:
        if k.endswith("mod8"):
            continue
        e, r = Xe[k], Xr[k]
        z = (e.mean() - r.mean()) / (r.std() / np.sqrt(len(e)))
        print(f"{k:18s} {e.mean():10.3f} {r.mean():11.3f} {r.std():8.3f} {z:6.2f}")
    for tag in ("m0", "m1"):
        e = np.bincount(Xe[f"{tag}_mod8"].astype(int), minlength=8)[1::2]
        print(f"{tag} mod 8 counts (1,3,5,7): elite {list(e)}")

    # per-bit frequency among elite multipliers
    for tag, idx in (("m0", 0), ("m1", 1)):
        ms = np.array([r[idx] for r in elite], dtype=np.uint64)
        freq = [float(((ms >> np.uint64(i)) & np.uint64(1)).mean()) for i in range(32)]
        n = len(ms)
        flagged = [(i, round(f, 2)) for i, f in enumerate(freq)
                   if i > 0 and abs(f - 0.5) > 3 * 0.5 / np.sqrt(n)]
        print(f"{tag} bits with 1-frequency beyond 3 sd of 0.5 (bit, freq): {flagged}")


if __name__ == "__main__":
    main()
