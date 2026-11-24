"""Exact sweep of one multiplier's top byte, everything else fixed.

Generates the 256 specs that differ from a base spec only in bits 24-31 of
m0 (or m1), scores them all with `mixlab exact --list`, and summarizes.

    python analysis/topbyte_sweep.py run     "[16 cb0332d3 15 549a3597 15]" m0 OUTDIR
    python analysis/topbyte_sweep.py summary OUTDIR

`run` writes OUTDIR/specs.txt and OUTDIR/exact.csv (mixlab's output,
spec,bias,max_abs). `summary` prints the ranking and per-feature stats.
Needs harness/target/release/mixlab (cargo build --release).
"""
import csv, os, re, subprocess, sys
from pathlib import Path
from statistics import mean, median

ROOT = Path(__file__).resolve().parent.parent
BIN = ROOT / "harness" / "target" / "release" / ("mixlab.exe" if os.name == "nt" else "mixlab")
SPEC_RE = re.compile(r"\[\s*(\d+)\s+([0-9a-fA-F]+)\s+(\d+)\s+([0-9a-fA-F]+)\s+(\d+)\s*\]")


def parse(spec):
    m = SPEC_RE.fullmatch(spec.strip())
    if not m:
        sys.exit(f"bad spec {spec!r}")
    s0, m0, s1, m1, s2 = m.groups()
    return int(s0), int(m0, 16), int(s1), int(m1, 16), int(s2)


def fmt(s0, m0, s1, m1, s2):
    return f"[{s0} {m0:08x} {s1} {m1:08x} {s2}]"


def run(base, which, outdir):
    s0, m0, s1, m1, s2 = parse(base)
    out = Path(outdir)
    out.mkdir(parents=True, exist_ok=True)
    specs = []
    for b in range(256):
        if which == "m0":
            specs.append(fmt(s0, (m0 & 0x00FFFFFF) | (b << 24), s1, m1, s2))
        else:
            specs.append(fmt(s0, m0, s1, (m1 & 0x00FFFFFF) | (b << 24), s2))
    (out / "specs.txt").write_text("\n".join(specs) + "\n")
    (out / "meta.txt").write_text(f"base {base}\nbyte {which} bits 24-31\n")
    subprocess.run([str(BIN), "exact", "--bits", "32", "--list", str(out / "specs.txt"),
                    "--out", str(out / "exact.csv")], check=True)


def summary(outdir):
    out = Path(outdir)
    meta = (out / "meta.txt").read_text().split("\n")
    base = meta[0][5:]
    which = meta[1].split()[1]
    idx = 1 if which == "m0" else 3
    rows = []
    with open(out / "exact.csv") as f:
        for r in csv.DictReader(f):
            p = parse(r["spec"])
            rows.append((float(r["bias"]), p[idx] >> 24, r["spec"]))
    rows.sort()
    base_byte = parse(base)[idx] >> 24
    base_bias = next(b for b, byte, _ in rows if byte == base_byte)
    print(f"base {base}  ({which} top byte {base_byte:02x}) bias {base_bias!r}")
    rank = [byte for _, byte, _ in rows].index(base_byte) + 1
    print(f"base rank {rank}/256; below base: {rank - 1}")
    print("top 20:")
    for b, byte, spec in rows[:20]:
        d = bin(byte ^ base_byte).count("1")
        print(f"  {byte:02x}  {spec}  {b!r}  ({d} flips from base)")
    bs = [b for b, _, _ in rows]
    print(f"min {bs[0]:.6f}  median {median(bs):.6f}  max {bs[-1]:.6f}")
    print(f"count < 0.1: {sum(b < 0.1 for b in bs)}   < 0.101: {sum(b < 0.101 for b in bs)}   < 0.105: {sum(b < 0.105 for b in bs)}")
    # descriptive: dependence on each bit of the byte and on top nibble
    by = {byte: b for b, byte, _ in rows}
    print("mean bias by bit value (bit 24+i: set vs clear):")
    for i in range(8):
        on = [v for k, v in by.items() if k >> i & 1]
        off = [v for k, v in by.items() if not k >> i & 1]
        print(f"  bit {24+i}: {mean(on):.5f} vs {mean(off):.5f}")
    print("median bias by top nibble:")
    print("  " + "  ".join(f"{n:x}:{median(by[(n << 4) | l] for l in range(16)):.4f}" for n in range(16)))


if __name__ == "__main__":
    if len(sys.argv) >= 5 and sys.argv[1] == "run":
        run(sys.argv[2], sys.argv[3], sys.argv[4])
    elif len(sys.argv) == 3 and sys.argv[1] == "summary":
        summary(sys.argv[2])
    else:
        sys.exit(__doc__)
