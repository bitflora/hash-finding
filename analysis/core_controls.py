"""Summarise results/core_controls/core_controls_exact.csv (F16).
Per core: min / median / max exact bias over the shared random high halves,
plus the source mixer's own score; Spearman rho between source score and
core median; paired win counts of each core against the record core."""
import csv, statistics, sys
sys.path.insert(0, __file__.rsplit("/", 1)[0].rsplit("\\", 1)[0])
from gen_core_controls import SOURCES  # noqa: E402  (argv parsing guarded below)

path = sys.argv[1] if len(sys.argv) > 1 else "../results/core_controls/core_controls_exact.csv"
text = open(path, newline="").read().replace(chr(13) + ",", ",")  # drafts had CR inside the draw field
rows = [r for r in csv.DictReader(text.splitlines()) if r["bias"]]
by = {}
for r in rows:
    by.setdefault(r["core"], {})[int(r["draw"].strip())] = float(r["bias"])

def rank(v):
    s = sorted(range(len(v)), key=lambda i: v[i]); out = [0.0] * len(v); i = 0
    while i < len(s):
        j = i
        while j + 1 < len(s) and v[s[j + 1]] == v[s[i]]: j += 1
        for k in range(i, j + 1): out[s[k]] = (i + j) / 2
        i = j + 1
    return out

def spearman(a, b):
    ra, rb = rank(a), rank(b); ma, mb = statistics.mean(ra), statistics.mean(rb)
    num = sum((x - ma) * (y - mb) for x, y in zip(ra, rb))
    return num / (sum((x - ma) ** 2 for x in ra) * sum((y - mb) ** 2 for y in rb)) ** 0.5

ref = by.get("record", {})
print(f"{'core':<10} {'source':>8} {'n':>3} {'min':>7} {'median':>7} {'max':>7}  wins_vs_record")
names = [n for n in SOURCES if n in by]
for n in names:
    v = by[n]; xs = list(v.values())
    common = [d for d in v if d in ref]
    wins = sum(v[d] < ref[d] for d in common)
    print(f"{n:<10} {SOURCES[n][5]:8.5f} {len(xs):3d} {min(xs):7.4f} {statistics.median(xs):7.4f} {max(xs):7.4f}  {wins}/{len(common)}")
if len(names) >= 3:
    src = [SOURCES[n][5] for n in names]
    print(f"\nSpearman(source score, core median) = {spearman(src, [statistics.median(by[n].values()) for n in names]):+.3f}")
    print(f"Spearman(source score, core min)    = {spearman(src, [min(by[n].values()) for n in names]):+.3f}")
    print(f"Spearman(source score, core max)    = {spearman(src, [max(by[n].values()) for n in names]):+.3f}")
