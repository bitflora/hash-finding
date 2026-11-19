"""Sample-cost model for comparing a base mixer with its neighbours.

Noise model (validated by `mixlab design` / `mixlab paired`): for N = bits^2
matrix entries and n i.i.d. inputs, the debiased mean-square estimate has
    Var(ms_hat) ~= (4*ms/n + 2/n^2) / N
and two different mixers' errors are ~independent even on common inputs.
So the difference A-B has Var ~= (4*(msA+msB)/n + 4/n^2) / N.

Usage: python racing_cost.py paired32_record_n18.csv BITS BASE_MS [Z]
(the CSV needs a true_dms column, or uses mean_dms when absent)
"""
import csv, math, sys


def n_needed(dms, ms_a, ms_b, bits, z):
    N = bits * bits
    lo, hi = 1.0, 2.0 ** 70
    for _ in range(200):  # bisection on log scale
        n = math.sqrt(lo * hi)
        sd = math.sqrt((4 * (ms_a + ms_b) / n + 4 / n ** 2) / N)
        if z * sd < abs(dms):
            hi = n
        else:
            lo = n
    return hi


def main():
    path, bits, base_ms = sys.argv[1], int(sys.argv[2]), float(sys.argv[3])
    z = float(sys.argv[4]) if len(sys.argv) > 4 else 3.0
    rows = [r for r in csv.DictReader(l for l in open(path) if not l.startswith('#'))]
    cap = 2.0 ** bits if bits <= 32 else 2.0 ** 34
    need = []
    print(f"{'move':<26}{'dms':>12}{'log2 n needed':>15}")
    for r in rows:
        d = float(r['true_dms']) if r.get('true_dms') else float(r['mean_dms'])
        ms_b = base_ms - d
        n = n_needed(d, base_ms, max(ms_b, 0.0), bits, z)
        need.append(min(n, cap))
        print(f"{r['move'][:25]:<26}{d:>12.3e}{math.log2(n):>15.1f}")
    k = len(need)
    fixed = k * max(need)
    seq = sum(need)
    print(f"\nz={z}, {k} neighbours, cap 2^{math.log2(cap):.0f} per mixer")
    print(f"fixed budget (resolve hardest): 2^{math.log2(fixed):.1f} inputs")
    print(f"sequential (stop each when decided): 2^{math.log2(seq):.1f} inputs  -> saving x{fixed/seq:.1f}")
    for t in (2 ** 20, 2 ** 24, 2 ** 28):
        print(f"  neighbours decided within 2^{math.log2(t):.0f} samples: {sum(n <= t for n in need)}/{k}")


if __name__ == '__main__':
    main()
