"""Random mutants of a mixer at fixed Hamming distance, for a bias-vs-distance profile.

Writes one spec per line plus a key CSV (spec, region, k); score the specs with
`mixlab exact --bits 32 --list`. Bit 0 of each multiplier is never flipped.
Regions: any (bits 1-31 of both), low (bits 1-15), high (bits 16-31).

Usage: python analysis/distance_profile.py SPEC OUTPREFIX [--per N] [--seed S]
"""
import argparse, csv, random, re
ap = argparse.ArgumentParser()
ap.add_argument("spec"); ap.add_argument("out")
ap.add_argument("--per", type=int, default=40); ap.add_argument("--seed", type=int, default=1)
ap.add_argument("--ks", default="1,2,3,4,6,8,10,12,16,20,24,32")
a = ap.parse_args()
s0, m0, s1, m1, s2 = re.findall(r"[0-9a-f]+", a.spec)
m = [int(m0, 16), int(m1, 16)]
REG = {"any": list(range(1, 32)), "low": list(range(1, 16)), "high": list(range(16, 32))}
rng = random.Random(a.seed)
seen, rows = set(), []
for region, bits in REG.items():
    pos = [(w, b) for w in (0, 1) for b in bits]
    for k in map(int, a.ks.split(",")):
        if k > len(pos): continue
        for _ in range(a.per):
            x = m[:]
            for w, b in rng.sample(pos, k): x[w] ^= 1 << b
            spec = f"[{s0} {x[0]:08x} {s1} {x[1]:08x} {s2}]"
            if spec in seen: continue
            seen.add(spec); rows.append((spec, region, k))
open(a.out + ".txt", "w").write("".join(r[0] + "\n" for r in rows))
with open(a.out + ".key.csv", "w", newline="") as f:
    w = csv.writer(f); w.writerow(["spec", "region", "k"]); w.writerows(rows)
print(len(rows), "specs")
