"""Sampled screen of a spec list (resumable): appends spec,ms,se to OUT.
Usage: python screen_list.py IN.csv OUT.csv [log2_samples=26] [seed=7]
"""
import csv, os, re, subprocess, sys
inp, out = sys.argv[1], sys.argv[2]
lg = sys.argv[3] if len(sys.argv) > 3 else "26"
seed = sys.argv[4] if len(sys.argv) > 4 else "7"
BIN = os.path.join(os.path.dirname(__file__), "..", "harness", "target", "release", "mixlab.exe")
done = set()
if os.path.exists(out):
    done = {r["spec"] for r in csv.DictReader(open(out, newline=""))}
specs = [r["spec"] for r in csv.DictReader(open(inp, newline=""))]
new = not os.path.exists(out)
with open(out, "a", newline="\n") as f:
    if new: f.write("spec,ms,se\n")
    for i, s in enumerate(specs):
        if s in done: continue
        o = subprocess.run([BIN, "eval", "--bits", "32", "--spec", s, "--samples", f"p{lg}", "--seed", seed],
                           capture_output=True, text=True, encoding="utf-8", check=True).stdout
        m = re.search(r"ms=(\S+) ± (\S+)", o)
        f.write(f"{s},{m.group(1)},{m.group(2)}\n"); f.flush()
        if i % 50 == 0: print(f"{i}/{len(specs)}", flush=True)
