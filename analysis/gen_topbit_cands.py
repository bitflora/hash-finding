"""Candidate list for a structured sweep around a 32-bit 2-round mixer.

Writes every combination of the top 3 bits of m0 and m1 (63 variants), plus the
three negations (m -> 2^32 - m) of the base. Usage:
    python gen_topbit_cands.py "[16 5e0f5553 15 735a2d97 15]" out.csv
Feed to: mixlab race --bits 32 --start SPEC --candidates out.csv
"""
import sys

spec, out = sys.argv[1], sys.argv[2]
s0, m0, s1, m1, s2 = spec.strip("[]").split()
m0, m1 = int(m0, 16), int(m1, 16)
N = 1 << 32
seen = {(m0, m1)}
rows = []
for a in range(8):
    for b in range(8):
        c = (m0 ^ (a << 29), m1 ^ (b << 29))
        if c not in seen:
            seen.add(c)
            rows.append(c)
for c in [((N - m0) % N, m1), (m0, (N - m1) % N), ((N - m0) % N, (N - m1) % N)]:
    if c not in seen:
        seen.add(c)
        rows.append(c)
with open(out, "w") as f:
    f.write("spec\n")
    for x, y in rows:
        f.write(f"[{s0} {x:08x} {s1} {y:08x} {s2}]\n")
print(f"{len(rows)} candidates -> {out}")
