"""Local test of the m mod 8 class: for each elite core pair (best mixer per
core pair with bias < 0.11, 16/15/15), flip one low bit of m0 or m1.
Bits 1, 2 change the +-1/+-3 class; bits 3-6 keep it. Sampled at 2^26."""
import csv, re, subprocess, sys
from pathlib import Path
ROOT = Path(__file__).resolve().parent.parent
EXE = ROOT / "harness/target/release/mixlab.exe"
best = {}
for r in csv.DictReader(open(ROOT / "results/explored32.csv")):
    if (r["s0"], r["s1"], r["s2"]) != ("16", "15", "15"): continue
    m0, m1, b = int(r["m0"], 16), int(r["m1"], 16), float(r["bias"])
    k = (m0 & 0xffff, m1 & 0xffff)
    if b < 0.11 and (k not in best or b < best[k][2]): best[k] = (m0, m1, b)
out = csv.writer(open(ROOT / "results/mod8_flip.csv", "w", newline=""))
out.writerow(["base", "base_bias", "which", "bit", "class_change", "spec", "bias_p26"])
for m0, m1, b in sorted(best.values(), key=lambda t: t[2]):
    for which in ("m0", "m1"):
        for bit in range(1, 7):
            a, c = (m0 ^ (1 << bit), m1) if which == "m0" else (m0, m1 ^ (1 << bit))
            spec = f"[16 {a:08x} 15 {c:08x} 15]"
            o = subprocess.run([str(EXE), "eval", "--bits", "32", "--spec", spec, "--samples", "p26", "--seed", "7"],
                               capture_output=True, text=True).stdout
            bias = float(re.search(r"bias=([0-9.]+)", o)[1])
            out.writerow([f"[16 {m0:08x} 15 {m1:08x} 15]", b, which, bit, int(bit <= 2), spec, bias])
            print(which, bit, spec, bias, flush=True)
