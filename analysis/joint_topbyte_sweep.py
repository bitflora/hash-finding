"""Joint sweep of both multipliers' top bytes (65,536 pairs), everything
else fixed to a base spec. Too many for exact scoring (about 11 days), so:

  calibrate  screen the specs that topbyte_sweep.py already scored exactly
             (CAL_DIRS) and report where the true best land, per budget
  screen     all 65,536 pairs, sampled at 2^LG1 (seed SEED1), resumable
  rescreen   the best K1 by screen, fresh samples at 2^LG2 (seed SEED2)
  exact      the best K2 by rescreen, exact via `mixlab exact --list`
  summary    ranking and the exact results

    python analysis/joint_topbyte_sweep.py calibrate|screen|rescreen|exact|summary

Sampling is examples/screen_list.rs (the SIMD sampler, signed debiased ms);
exact scores are mixlab's. Python only selects and ranks.
"""
import csv, os, re, subprocess, sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
REL = ROOT / "harness" / "target" / "release"
EXE = ".exe" if os.name == "nt" else ""
MIXLAB = REL / f"mixlab{EXE}"
SCREEN = REL / "examples" / f"screen_list{EXE}"

BASE = "[16 4b0332d3 15 549a3597 15]"
OUT = ROOT / "results" / "ironborn_ideas" / "joint_topbyte_4b03"
CAL_DIRS = ["topbyte_cb03_m0", "topbyte_4b03_m1"]
# 2^24 was too noisy in calibration (a true top-10 spec ranked 271/511);
# at 2^26 the true top 10 mostly rank within the top 6%.
LG1, SEED1, K1 = 26, 3101, 4096
LG2, SEED2, K2 = 28, 3102, 64

SPEC_RE = re.compile(r"\[\s*(\d+)\s+([0-9a-fA-F]+)\s+(\d+)\s+([0-9a-fA-F]+)\s+(\d+)\s*\]")


def parse(spec):
    s0, m0, s1, m1, s2 = SPEC_RE.fullmatch(spec.strip()).groups()
    return int(s0), int(m0, 16), int(s1), int(m1, 16), int(s2)


def spec_for(b0, b1):
    s0, m0, s1, m1, s2 = parse(BASE)
    m0 = (m0 & 0x00FFFFFF) | (b0 << 24)
    m1 = (m1 & 0x00FFFFFF) | (b1 << 24)
    return f"[{s0} {m0:08x} {s1} {m1:08x} {s2}]"


def bytes_of(spec):
    p = parse(spec)
    return p[1] >> 24, p[3] >> 24


def write_list(path, specs):
    path.write_text("spec\n" + "\n".join(specs) + "\n")


def screen_file(inp, out, lg, seed):
    subprocess.run([str(SCREEN), str(inp), str(out), str(lg), str(seed)], check=True)


def read_ms(path):
    with open(path) as f:
        return {r["spec"]: float(r["sampled_ms"]) for r in csv.DictReader(f)}


def read_exact(path):
    with open(path) as f:
        return {r["spec"]: float(r["bias"]) for r in csv.DictReader(f)}


def calibrate():
    truth = {}
    for d in CAL_DIRS:
        truth.update(read_exact(ROOT / "results" / "ironborn_ideas" / d / "exact.csv"))
    cal = OUT / "calibration"
    cal.mkdir(parents=True, exist_ok=True)
    write_list(cal / "known.csv", list(truth))
    top = sorted(truth, key=truth.get)[:10]
    n = len(truth)
    print(f"{n} exactly scored specs; true top 10 biases {[round(truth[k], 5) for k in top]}")
    for lg in (24, 26, 28):
        for seed in (11, 12):
            out = cal / f"screen_{lg}_{seed}.csv"
            screen_file(cal / "known.csv", out, lg, seed)
            ms = read_ms(out)
            order = sorted(ms, key=ms.get)
            rank = {k: i + 1 for i, k in enumerate(order)}
            print(f"  2^{lg} seed {seed}: sampled ranks of true top 10 (of {n}): {[rank[k] for k in top]}")


def screen():
    OUT.mkdir(parents=True, exist_ok=True)
    write_list(OUT / "all.csv", [spec_for(b0, b1) for b0 in range(256) for b1 in range(256)])
    screen_file(OUT / "all.csv", OUT / f"screen_{LG1}.csv", LG1, SEED1)


def rescreen():
    ms = read_ms(OUT / f"screen_{LG1}.csv")
    assert len(ms) == 65536, f"screen incomplete: {len(ms)}"
    write_list(OUT / "rescreen_in.csv", sorted(ms, key=ms.get)[:K1])
    screen_file(OUT / "rescreen_in.csv", OUT / f"rescreen_{LG2}.csv", LG2, SEED2)


def exact():
    ms = read_ms(OUT / f"rescreen_{LG2}.csv")
    assert len(ms) == K1, f"rescreen incomplete: {len(ms)}"
    (OUT / "exact_in.txt").write_text("\n".join(sorted(ms, key=ms.get)[:K2]) + "\n")
    subprocess.run([str(MIXLAB), "exact", "--bits", "32", "--list", str(OUT / "exact_in.txt"),
                    "--out", str(OUT / "exact.csv")], check=True)


def summary():
    s1 = read_ms(OUT / f"screen_{LG1}.csv")
    s2 = read_ms(OUT / f"rescreen_{LG2}.csv")
    ex = read_exact(OUT / "exact.csv")
    r1 = {k: i + 1 for i, k in enumerate(sorted(s1, key=s1.get))}
    r2 = {k: i + 1 for i, k in enumerate(sorted(s2, key=s2.get))}
    b0, b1 = bytes_of(BASE)
    print(f"base {BASE}: screen rank {r1[BASE]}, rescreen rank {r2.get(BASE)}, exact {ex.get(BASE)!r}")
    print(f"exact results ({len(ex)}), best first:")
    for spec in sorted(ex, key=ex.get):
        x0, x1 = bytes_of(spec)
        d = bin(x0 ^ b0).count("1") + bin(x1 ^ b1).count("1")
        print(f"  {spec}  {ex[spec]!r}  screen#{r1[spec]} rescreen#{r2[spec]}  {d} flips from base")
    print(f"below 0.1: {sum(v < 0.1 for v in ex.values())}")


if __name__ == "__main__":
    stages = {"calibrate": calibrate, "screen": screen, "rescreen": rescreen, "exact": exact, "summary": summary}
    if len(sys.argv) != 2 or sys.argv[1] not in stages:
        sys.exit(__doc__)
    stages[sys.argv[1]]()
