"""Bit-position importance and core-width profile of a 2-round 32-bit mixer.

Two spec sets around a base mixer (default: the F30 best), scored by mixlab:
  flips  every single-bit flip of m0 and of m1, bits 1..31 (bit 0 would make
         the multiplier even, so it is skipped)
  cores  keep the low k bits of both multipliers, replace the rest with random
         bits; the same random draws are used for every k (paired design)

Usage (from the repository root):
  python analysis/bit_sensitivity.py gen [--out results/bit_sensitivity]
  harness/target/release/mixlab exact --bits 32 --list results/bit_sensitivity/specs.txt \
      --out results/bit_sensitivity/bit_sensitivity_exact.csv
  python analysis/bit_sensitivity.py analyse [--out results/bit_sensitivity]

Scores come from mixlab only; this script builds specs and summarises scores.
"""
import argparse
import csv
import os
import random
import statistics

BASE = (16, 0x4B0332D3, 15, 0x549A3597, 15)
WIDTHS = (8, 12, 16, 20, 24)
DRAWS = 8
SEED = 20261125


def spec(s0, m0, s1, m1, s2):
    return f"[{s0} {m0:08x} {s1} {m1:08x} {s2}]"


def parse(sp):
    t = sp.strip("[]").split()
    return int(t[0]), int(t[1], 16), int(t[2]), int(t[3], 16), int(t[4])


def build(base):
    s0, m0, s1, m1, s2 = base
    out = [("base", -1, -1, spec(*base))]
    for which in (0, 1):
        for b in range(1, 32):
            a, c = (m0 ^ (1 << b), m1) if which == 0 else (m0, m1 ^ (1 << b))
            out.append(("flip", which, b, spec(s0, a, s1, c, s2)))
    rng = random.Random(SEED)
    draws = [(rng.getrandbits(32), rng.getrandbits(32)) for _ in range(DRAWS)]
    for k in WIDTHS:
        mask = (1 << k) - 1
        for d, (r0, r1) in enumerate(draws):
            a = (m0 & mask) | (r0 & ~mask & 0xFFFFFFFF)
            c = (m1 & mask) | (r1 & ~mask & 0xFFFFFFFF)
            out.append(("core", k, d, spec(s0, a, s1, c, s2)))
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("cmd", choices=["gen", "analyse"])
    ap.add_argument("--out", default="results/bit_sensitivity")
    ap.add_argument("--base", default=spec(*BASE))
    a = ap.parse_args()
    base = parse(a.base)
    rows = build(base)
    os.makedirs(a.out, exist_ok=True)
    if a.cmd == "gen":
        seen, specs = set(), []
        for r in rows:
            if r[3] not in seen:
                seen.add(r[3])
                specs.append(r[3])
        with open(os.path.join(a.out, "specs.txt"), "w") as f:
            f.write("\n".join(specs) + "\n")
        with open(os.path.join(a.out, "design.csv"), "w", newline="") as f:
            w = csv.writer(f)
            w.writerow(["kind", "a", "b", "spec"])
            w.writerows(rows)
        print(f"{len(specs)} distinct specs -> {a.out}/specs.txt")
        return

    score = {}
    with open(os.path.join(a.out, "bit_sensitivity_exact.csv")) as f:
        for r in csv.DictReader(f):
            score[r["spec"]] = float(r["bias"])
    b0 = score[spec(*base)]
    print(f"base {spec(*base)} = {b0:.6f}\n")
    print("single-bit flips: bias / base  (m0 | m1)")
    print(" bit   m0      m1")
    prof = {0: {}, 1: {}}
    for kind, which, b, sp in rows:
        if kind == "flip":
            prof[which][b] = score[sp] / b0
    for b in range(1, 32):
        print(f" {b:3d}  {prof[0][b]:7.3f} {prof[1][b]:7.3f}")
    for which in (0, 1):
        for lo, hi in ((1, 8), (8, 16), (16, 24), (24, 32)):
            v = [prof[which][b] for b in range(lo, hi)]
            print(f"m{which} bits {lo:2d}-{hi - 1:2d}: median ratio "
                  f"{statistics.median(v):.3f}, max {max(v):.3f}")
    print("\ncore width k (low k bits kept, random above; same draws per k)")
    for k in WIDTHS:
        v = sorted(score[sp] for kind, kk, d, sp in rows if kind == "core" and kk == k)
        print(f" k={k:2d}: min {v[0]:.4f}  median {statistics.median(v):.4f}  "
              f"max {v[-1]:.4f}")


if __name__ == "__main__":
    main()
