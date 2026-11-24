"""Every radius-1 and radius-2 neighbour of a 32-bit 2-round mixer.

A move is one of: flip bit k (1..31) of m0 or m1, or shift s0/s1/s2 by +-1
(TheIronBorn's move set, issue #19). Radius 2 includes repeated moves,
so a shift may change by +-2. Cancelling moves are deduplicated away.
s2 is capped at 16, as in the campaign.
Writes N chunk files <out>_kK.csv for `mixlab race --candidates`.
Usage: python gen_radius2_cands.py "[16 21f0aaad 15 735a2d97 15]" OUTPREFIX CHUNKS
"""
import itertools, sys

spec, out, chunks = sys.argv[1], sys.argv[2], int(sys.argv[3])
s0, m0, s1, m1, s2 = spec.strip("[]").split()
base = [int(s0), int(m0, 16), int(s1), int(m1, 16), int(s2)]
moves = [("bit", i, k) for i in (1, 3) for k in range(1, 32)]
moves += [("sh", i, d) for i in (0, 2, 4) for d in (-1, 1)]

def apply(ms):
    c = list(base)
    for kind, i, v in ms:
        if kind == "bit": c[i] ^= 1 << v
        else: c[i] += v
    if not all(1 <= c[i] <= 31 for i in (0, 2, 4)) or c[4] > 16: return None
    return tuple(c)

seen, rows = {tuple(base)}, []
for r in (1, 2):
    for ms in itertools.combinations_with_replacement(moves, r):
        c = apply(ms)
        if c and c not in seen:
            seen.add(c); rows.append(c)
for k in range(chunks):
    with open(f"{out}_k{k}.csv", "w", newline="\n") as f:
        f.write("spec\n")
        for c in rows[k::chunks]:
            f.write(f"[{c[0]} {c[1]:08x} {c[2]} {c[3]:08x} {c[4]}]\n")
print(f"{len(rows)} candidates in {chunks} chunks -> {out}_k*.csv")
