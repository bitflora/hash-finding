"""Crossover children of good [16 _ 15 _ 15] mixers: m0 from parent A, m1 from B.

Parents: every campaign winner with those shifts, plus the 20 best published
mixers with those shifts. Writes parents.csv and children.csv (A != B, both
orders, children that are themselves parents dropped).
Usage: python gen_crossover.py OUTDIR
"""
import csv, glob, json, os, sys

out = sys.argv[1]
ROOT = os.path.join(os.path.dirname(__file__), "..")
par = {}
def add(spec, bias, src):
    p = spec.strip("[]").split()
    if p[0] == "16" and p[2] == "15" and p[4] == "15":
        key = (int(p[1], 16), int(p[3], 16))
        if key not in par or bias < par[key][0]: par[key] = (float(bias), src)
for f in glob.glob(os.path.join(ROOT, "results", "campaign", "*.done.json")):
    r = json.load(open(f)); add(r["best"], r["bias"], r["id"])
pub = sorted(csv.DictReader(open(os.path.join(ROOT, "results", "published_2round32.csv"), newline="")),
             key=lambda r: float(r["exact_bias"]))
k = 0
for r in pub:
    p = r["spec"].strip("[]").split()
    if p[0] == "16" and p[2] == "15" and p[4] == "15" and k < 20:
        add(r["spec"], r["exact_bias"], "pub:" + r["source"]); k += 1
P = sorted(par.items(), key=lambda kv: kv[1][0])
with open(os.path.join(out, "parents.csv"), "w", newline="\n") as f:
    f.write("spec,bias,source\n")
    for (a, b), (bias, src) in P:
        f.write(f"[16 {a:08x} 15 {b:08x} 15],{bias!r},{src}\n")
with open(os.path.join(out, "children.csv"), "w", newline="\n") as f:
    f.write("spec,parent_m0,parent_m1\n")
    n = 0
    for i, ((a, _), _) in enumerate(P):
        for j, ((_, b), _) in enumerate(P):
            if i != j and (a, b) not in par:
                f.write(f"[16 {a:08x} 15 {b:08x} 15],{i},{j}\n"); n += 1
print(f"{len(P)} parents, {n} children -> {out}")
