"""Drop already-tried specs from candidate chunk files, in place.
Usage: python filter_seen.py "CHUNK_GLOB" "SEEN_CSV_GLOB[;GLOB...]"
"""
import glob, sys
key = lambda s: tuple(s.strip("[]").split())
seen = set()
for f in (x for g in sys.argv[2].split(";") for x in glob.glob(g)):
    seen |= {key(l.strip()) for l in open(f) if l.startswith("[")}
kept = dropped = 0
for f in glob.glob(sys.argv[1]):
    rows = [l.strip() for l in open(f) if l.startswith("[")]
    new = [r for r in rows if key(r) not in seen]
    kept += len(new); dropped += len(rows) - len(new)
    open(f, "w", newline="\n").write("spec\n" + "".join(r + "\n" for r in new))
print(f"kept {kept}, dropped {dropped} already tried")
