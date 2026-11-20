"""Exact BIC pair correlation over all 2^32 inputs for a 2-round 32-bit xmx.
For input bit i, the flip indicators A = bit j of f(x)^f(x^e_i) and
B = bit k of the same; prints counts and the Pearson correlation as exact
fractions. Usage: python exact_pair_corr.py "[16 21f0aaad 15 735a2d97 15]" i j k"""
import sys
from fractions import Fraction
import numpy as np

spec, i, j, k = sys.argv[1], int(sys.argv[2]), int(sys.argv[3]), int(sys.argv[4])
s0, m0, s1, m1, s2 = spec.strip("[]").split()
s0, s1, s2 = int(s0), int(s1), int(s2)
m0, m1 = np.uint32(int(m0, 16)), np.uint32(int(m1, 16))

def f(x):
    x = x ^ (x >> np.uint32(s0)); x = x * m0
    x = x ^ (x >> np.uint32(s1)); x = x * m1
    return x ^ (x >> np.uint32(s2))

N = 1 << 32
CH = 1 << 25
na = nb = nab = 0
e = np.uint32(1 << i)
with np.errstate(over="ignore"):
    for c in range(N // CH):
        x = np.arange(c * CH, (c + 1) * CH, dtype=np.uint64).astype(np.uint32)
        d = f(x) ^ f(x ^ e)
        a = (d >> np.uint32(j)) & np.uint32(1)
        b = (d >> np.uint32(k)) & np.uint32(1)
        na += int(a.sum(dtype=np.uint64)); nb += int(b.sum(dtype=np.uint64))
        nab += int((a & b).sum(dtype=np.uint64))
pa, pb, pab = Fraction(na, N), Fraction(nb, N), Fraction(nab, N)
cov = pab - pa * pb
var = pa * (1 - pa) * pb * (1 - pb)
corr = float(cov) / float(var) ** 0.5
print(f"{spec} in {i} -> out {j},{k}: P(A)={pa} P(B)={pb} P(AB)={pab} corr={corr:+.10f}")
