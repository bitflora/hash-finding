"""One beam-search step beyond an exhaustive radius-2 check.

Takes the BEAM best neighbours that earlier `race --candidates` runs scored
exactly (verdict Rejected = close but not better, with its exact ms), and
writes every radius-1 neighbour of each (same move set as
gen_radius2_cands.py) that was not already tried, in CHUNKS files for
`mixlab race --candidates` against the original centre.
Usage: python beam_step.py OUTPREFIX CHUNKS BEAM "RACE_TXT_GLOB" "SEEN_CSV_GLOB"
(each glob may be several patterns separated by ";")
"""
import glob, sys

out, chunks, beam, race_glob, seen_glob = sys.argv[1], int(sys.argv[2]), int(sys.argv[3]), sys.argv[4], sys.argv[5]
key = lambda s: tuple(s.strip("[]").split())
seen = set()
for f in (x for g in seen_glob.split(";") for x in glob.glob(g)):
    seen |= {key(l.strip()) for l in open(f) if l.startswith("[")}
scored = {}
for f in (x for g in race_glob.split(";") for x in glob.glob(g)):
    for l in open(f):
        if l.startswith("[") and "Rejected" in l:
            s = l[:l.index("]") + 1]; scored[s] = float(l.split()[-1])
top = sorted(scored, key=scored.get)[:beam]
print(f"{len(scored)} exactly scored; beam of {len(top)}: ms {scored[top[0]]:.4e} .. {scored[top[-1]]:.4e}")

moves = [("bit", i, k) for i in (1, 3) for k in range(1, 32)] + [("sh", i, d) for i in (0, 2, 4) for d in (-1, 1)]
rows = []
for s in top:
    s0, m0, s1, m1, s2 = s.strip("[]").split()
    base = [int(s0), int(m0, 16), int(s1), int(m1, 16), int(s2)]
    for kind, i, v in moves:
        c = list(base)
        if kind == "bit": c[i] ^= 1 << v
        else: c[i] += v
        if not all(1 <= c[j] <= 31 for j in (0, 2, 4)) or c[4] > 16: continue
        t = f"[{c[0]} {c[1]:08x} {c[2]} {c[3]:08x} {c[4]}]"
        if key(t) not in seen:
            seen.add(key(t)); rows.append(t)
for k in range(chunks):
    with open(f"{out}_k{k}.csv", "w", newline="\n") as f:
        f.write("spec\n" + "".join(r + "\n" for r in rows[k::chunks]))
print(f"{len(rows)} new candidates in {chunks} chunks -> {out}_k*.csv")
