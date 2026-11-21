"""One generation of cross -> screen -> exact -> race (resumable; F20 follow-up).

Parents: campaign winners and the 20 best published mixers with shifts
16/15/15, plus EXTRA (spec,bias,source). A child takes m0 from one parent and
m1 from another. Only *genuine* children are kept: their low-16-bit core pair
(m0 & 0xffff, m1 & 0xffff) must not belong to any parent, since children
that share a parent's core pair are near-copies of that family.
Steps: screen every new child at 2^26 (specs already screened in PRIOR
screen files are reused), score the TOPX best genuine children exactly, then
race the RACE best exact children that have distinct core pairs.
Usage: python crossover_round.py OUTDIR EXTRA.csv [prior_screen.csv ...]
Env: TOPX (default 20), RACE (3), SEED (81).
"""
import csv, glob, json, os, re, subprocess, sys

out, extra, priors = sys.argv[1], sys.argv[2], sys.argv[3:]
ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
BIN = os.path.join(ROOT, "harness", "target", "release", "mixlab.exe")
TOPX, RACE, SEED = int(os.environ.get("TOPX", 20)), int(os.environ.get("RACE", 3)), int(os.environ.get("SEED", 81))
os.makedirs(out, exist_ok=True)
log = lambda *a: print(*a, flush=True)

def run(args):
    return subprocess.run([BIN] + args, capture_output=True, text=True, encoding="utf-8", check=True).stdout

par = {}
def add(spec, bias, src):
    p = spec.strip("[]").split()
    if p[0] == "16" and p[2] == "15" and p[4] == "15":
        k = (int(p[1], 16), int(p[3], 16))
        if k not in par or float(bias) < par[k][0]: par[k] = (float(bias), src)
for f in glob.glob(os.path.join(ROOT, "results", "campaign", "*.done.json")):
    r = json.load(open(f)); add(r["best"], r["bias"], r["id"])
pub = sorted(csv.DictReader(open(os.path.join(ROOT, "results", "published_2round32.csv"), newline="")),
             key=lambda r: float(r["exact_bias"]))
pub = [r for r in pub if r["spec"].split()[0] == "[16" and r["spec"].split()[2] == "15" and r["spec"].split()[4] == "15]"][:20]
for r in pub: add(r["spec"], r["exact_bias"], "pub:" + r["source"])
for r in csv.DictReader(open(extra, newline="")): add(r["spec"], r["bias"], r["source"])
P = sorted(par.items(), key=lambda kv: kv[1][0])
cores = {(a & 0xffff, b & 0xffff) for (a, b), _ in P}
spec = lambda a, b: f"[16 {a:08x} 15 {b:08x} 15]"
with open(os.path.join(out, "parents.csv"), "w", newline="\n") as f:
    f.write("spec,bias,source\n")
    for (a, b), (bias, src) in P: f.write(f"{spec(a, b)},{bias!r},{src}\n")
kids = {}
for i, ((a, _), _) in enumerate(P):
    for j, ((_, b), _) in enumerate(P):
        if (a & 0xffff, b & 0xffff) not in cores and spec(a, b) not in kids:
            kids[spec(a, b)] = (i, j)
log(f"{len(P)} parents, {len(cores)} core pairs, {len(kids)} genuine children")

# screen
sc = {}
for p in priors + [os.path.join(out, "screen_p26.csv")]:
    if os.path.exists(p):
        for r in csv.DictReader(open(p, newline="")): sc[r["spec"]] = float(r["ms"])
todo = [s for s in kids if s not in sc]
sp = os.path.join(out, "screen_p26.csv"); new = not os.path.exists(sp)
with open(sp, "a", newline="\n") as f:
    if new: f.write("spec,ms,se\n")
    for n, s in enumerate(todo):
        m = re.search(r"ms=(\S+) ± (\S+)", run(["eval", "--bits", "32", "--spec", s, "--samples", "p26", "--seed", "7"]))
        f.write(f"{s},{m.group(1)},{m.group(2)}\n"); f.flush(); sc[s] = float(m.group(1))
        if n % 100 == 0: log(f"screen {n}/{len(todo)}")

# exact
ranked = sorted(kids, key=lambda s: sc[s])[:TOPX]
ex = {}
ep = os.path.join(out, "exact.csv")
if os.path.exists(ep):
    ex = {r["spec"]: float(r["bias"]) for r in csv.DictReader(open(ep, newline=""))}
with open(ep, "a", newline="\n") as f:
    if not ex: f.write("spec,bias,parent_m0,parent_m1\n")
    for s in ranked:
        if s in ex: continue
        ex[s] = float(re.search(r"bias=(\S+)", run(["eval", "--bits", "32", "--spec", s, "--exact"])).group(1))
        i, j = kids[s]
        f.write(f"{s},{ex[s]!r},{P[i][1][1]},{P[j][1][1]}\n"); f.flush()
        log(f"exact {s} {ex[s]:.5f}  ({P[i][1][1]} x {P[j][1][1]})")

# race
picked, seen = [], set()
for s in sorted(ex, key=ex.get):
    a, b = int(s.split()[1], 16), int(s.split()[3], 16)
    if (a & 0xffff, b & 0xffff) in seen: continue
    seen.add((a & 0xffff, b & 0xffff)); picked.append(s)
    if len(picked) == RACE: break
for k, s in enumerate(picked):
    tag = s.split()[1] + "x" + s.split()[3]
    o = os.path.join(out, f"race_{tag}.csv")
    if os.path.exists(o + ".done"):
        continue
    log(f"race {s} (exact {ex[s]:.5f})")
    r = run(["race", "--bits", "32", "--start", s, "--iters", "25", "--neighbors", "96", "--patience", "6",
             "--max-s2", "16", "--seed", str(SEED + k), "--out", o])
    log(r.strip()); open(o + ".done", "w").write(r)
