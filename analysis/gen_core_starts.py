"""Starts for the F10 low-bit-core test: N mixers with the record's low 16 bits
(m0 ..aaad, m1 ..2d97) and random high halves, and N fully random odd mixers,
all with shifts [16 _ 15 _ 15]. Usage: python gen_core_starts.py N seed out.csv"""
import random, sys
n, seed, out = int(sys.argv[1]), int(sys.argv[2]), sys.argv[3]
rng = random.Random(seed)
with open(out, "w") as f:
    f.write("spec,group\n")
    for _ in range(n):
        f.write(f"[16 {rng.getrandbits(16) << 16 | 0xaaad:08x} 15 {rng.getrandbits(16) << 16 | 0x2d97:08x} 15],core\n")
    for _ in range(n):
        f.write(f"[16 {rng.getrandbits(32) | 1:08x} 15 {rng.getrandbits(32) | 1:08x} 15],random\n")
