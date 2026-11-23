"""Exact-score a list of specs, then race the best few distinct core pairs.

For hand-picked core pairings (e.g. results/ironborn_ideas/core_list.csv:
cores only ever tried with their own partners). Resumable.
Usage: python race_list.py OUTDIR LIST.csv   (LIST needs a `spec` column)
Env: RACE (5), SEEDS (2), SEED (201).
"""
import csv, os, re, subprocess, sys

out, lst = sys.argv[1], sys.argv[2]
ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
BIN = os.path.join(ROOT, "harness", "target", "release", "mixlab.exe")
RACE, SEEDS, SEED = (int(os.environ.get(k, d)) for k, d in (("RACE", 5), ("SEEDS", 2), ("SEED", 201)))
os.makedirs(out, exist_ok=True)
log = lambda *a: print(*a, flush=True)
run = lambda args: subprocess.run([BIN] + args, capture_output=True, text=True, encoding="utf-8", check=True).stdout
specs = [r["spec"] for r in csv.DictReader(open(lst, newline=""))]

ep = os.path.join(out, "exact.csv")
ex = {r["spec"]: float(r["bias"]) for r in csv.DictReader(open(ep, newline=""))} if os.path.exists(ep) else {}
with open(ep, "a", newline="\n") as f:
    if not ex: f.write("spec,bias\n")
    for s in specs:
        if s in ex: continue
        ex[s] = float(re.search(r"bias=(\S+)", run(["eval", "--bits", "32", "--spec", s, "--exact"])).group(1))
        f.write(f"{s},{ex[s]!r}\n"); f.flush()
        log(f"exact {s} {ex[s]:.5f}")

picked, seen = [], set()
for s in sorted(specs, key=ex.get):
    p = s.split(); c = (int(p[1], 16) & 0xffff, int(p[3], 16) & 0xffff)
    if c in seen: continue
    seen.add(c); picked.append(s)
    if len(picked) == RACE: break
for k, s in enumerate(picked):
    for r in range(SEEDS):
        o = os.path.join(out, f"race_{s.split()[1]}x{s.split()[3]}_s{r}.csv")
        if os.path.exists(o + ".done"): continue
        log(f"race {s} seed#{r} (exact {ex[s]:.5f})")
        res = run(["race", "--bits", "32", "--start", s, "--iters", "25", "--neighbors", "96", "--patience", "6",
                   "--max-s2", "16", "--seed", str(SEED + 10 * k + r), "--out", o])
        log(res.strip()); open(o + ".done", "w").write(res)
log("done")
