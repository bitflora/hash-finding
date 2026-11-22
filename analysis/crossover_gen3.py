"""Crossover generation 3: finer crossover + several race seeds per child.

Tries FINDINGS §5 variations 1 (finer crossover) and 6 (several seeds per
child) on top of the gen-2 population. Three child kinds, kept apart so the
variations can be compared:
  whole   - m0 from one parent, m1 from another (as in gen 1/2)
  half    - each multiplier = high 16 bits of one top parent's multiplier
            | low 16 bits of another's (cores and high halves recombined)
  uniform - each multiplier = per-bit random mix of two top parents'
            multipliers (random 32-bit mask, forced odd)
Only genuine children are kept (core pair m0&0xffff, m1&0xffff belongs to no
parent). Each kind: screen at 2^26 -> exact top TOPX -> race the RACE best
with distinct core pairs, each with SEEDS race seeds. Resumable.
Usage: python crossover_gen3.py OUTDIR PARENTS.csv [prior_screen.csv ...]
Env: TOPX (12), RACE (2), SEEDS (3), NHALF (500), NUNI (500), TOP (16), SEED (101).
"""
import csv, os, random, re, subprocess, sys

out, parents_csv, priors = sys.argv[1], sys.argv[2], sys.argv[3:]
ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
BIN = os.path.join(ROOT, "harness", "target", "release", "mixlab.exe")
E = lambda k, d: int(os.environ.get(k, d))
TOPX, RACE, SEEDS, NHALF, NUNI, TOP, SEED = (E("TOPX", 12), E("RACE", 2), E("SEEDS", 3),
                                             E("NHALF", 500), E("NUNI", 500), E("TOP", 16), E("SEED", 101))
os.makedirs(out, exist_ok=True)
log = lambda *a: print(*a, flush=True)
run = lambda args: subprocess.run([BIN] + args, capture_output=True, text=True, encoding="utf-8", check=True).stdout
spec = lambda a, b: f"[16 {a:08x} 15 {b:08x} 15]"
ms = lambda s: (int(s.split()[1], 16), int(s.split()[3], 16))
core = lambda a, b: (a & 0xffff, b & 0xffff)

par = {}
for r in csv.DictReader(open(parents_csv, newline="")):
    k = ms(r["spec"])
    if k not in par or float(r["bias"]) < par[k][0]: par[k] = (float(r["bias"]), r["source"])
P = sorted(par.items(), key=lambda kv: kv[1][0])
cores = {core(a, b) for (a, b), _ in P}
top = [k for k, _ in P[:TOP]]
log(f"{len(P)} parents, {len(cores)} core pairs, top {len(top)} used for half/uniform")

kids = {"whole": {}, "half": {}, "uniform": {}}
def put(kind, a, b, why):
    s = spec(a, b)
    if core(a, b) in cores or (a, b) in par or any(s in d for d in kids.values()): return
    kids[kind][s] = why
for (a, _), (_, sa) in P:
    for (_, b), (_, sb) in P: put("whole", a, b, f"{sa} x {sb}")
rng = random.Random(SEED)
H, L = 0xffff0000, 0x0000ffff
tries = 0
while len(kids["half"]) < NHALF and tries < 50 * NHALF:
    tries += 1
    w, x, y, z = (rng.choice(top) for _ in range(4))
    put("half", (w[0] & H) | (x[0] & L), (y[1] & H) | (z[1] & L), "half")
tries = 0
while len(kids["uniform"]) < NUNI and tries < 50 * NUNI:
    tries += 1
    p, q = rng.sample(top, 2)
    m0, m1 = rng.getrandbits(32), rng.getrandbits(32)
    put("uniform", ((p[0] & m0) | (q[0] & ~m0 & 0xffffffff)) | 1,
        ((p[1] & m1) | (q[1] & ~m1 & 0xffffffff)) | 1, "uniform")
log("children: " + ", ".join(f"{k}={len(v)}" for k, v in kids.items()))
if os.environ.get("DRY"): sys.exit()

# screen (all kinds in one resumable file)
sc = {}
for p in priors + [os.path.join(out, "screen_p26.csv")]:
    if os.path.exists(p):
        for r in csv.DictReader(open(p, newline="")): sc[r["spec"]] = float(r["ms"])
todo = [s for d in kids.values() for s in d if s not in sc]
sp = os.path.join(out, "screen_p26.csv"); new = not os.path.exists(sp)
with open(sp, "a", newline="\n") as f:
    if new: f.write("spec,ms,se\n")
    for n, s in enumerate(todo):
        m = re.search(r"ms=(\S+) ± (\S+)", run(["eval", "--bits", "32", "--spec", s, "--samples", "p26", "--seed", "7"]))
        f.write(f"{s},{m.group(1)},{m.group(2)}\n"); f.flush(); sc[s] = float(m.group(1))
        if n % 100 == 0: log(f"screen {n}/{len(todo)}")
log("screen done")

# exact, per kind
ep = os.path.join(out, "exact.csv")
ex = {r["spec"]: float(r["bias"]) for r in csv.DictReader(open(ep, newline=""))} if os.path.exists(ep) else {}
with open(ep, "a", newline="\n") as f:
    if not ex: f.write("spec,bias,kind,parents\n")
    for kind, d in kids.items():
        for s in sorted(d, key=lambda s: sc[s])[:TOPX]:
            if s in ex: continue
            ex[s] = float(re.search(r"bias=(\S+)", run(["eval", "--bits", "32", "--spec", s, "--exact"])).group(1))
            f.write(f"{s},{ex[s]!r},{kind},{d[s]}\n"); f.flush()
            log(f"exact {kind:7s} {s} {ex[s]:.5f}")

# race: RACE best distinct-core children per kind, SEEDS seeds each
for kind, d in kids.items():
    picked, seen = [], set()
    for s in sorted((s for s in d if s in ex), key=ex.get):
        c = core(*ms(s))
        if c in seen: continue
        seen.add(c); picked.append(s)
        if len(picked) == RACE: break
    for k, s in enumerate(picked):
        for r in range(SEEDS):
            tag = s.split()[1] + "x" + s.split()[3]
            o = os.path.join(out, f"race_{kind}_{tag}_s{r}.csv")
            if os.path.exists(o + ".done"): continue
            log(f"race {kind} {s} seed#{r} (exact {ex[s]:.5f})")
            res = run(["race", "--bits", "32", "--start", s, "--iters", "25", "--neighbors", "96", "--patience", "6",
                       "--max-s2", "16", "--seed", str(SEED + 10 * k + r), "--out", o])
            log(res.strip()); open(o + ".done", "w").write(res)
log("gen3 done")
