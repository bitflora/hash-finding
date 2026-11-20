"""F10 follow-up: is the record's low-bit core special, or does every good
mixer's core carry its quality? For each source mixer, keep its shifts and the
low 16 bits of both multipliers, and draw the high 16 bits of each multiplier
at random. The SAME N random high-half pairs are used for every core (paired
design). Usage: python gen_core_controls.py N seed out.csv"""
import random, sys

# name: (s0, m0, s1, m1, s2, exact bias of the source mixer)
SOURCES = {
    "record":   (16, 0x21f0aaad, 15, 0x735a2d97, 15, 0.10704308166917044),
    "7870":     (16, 0x7870aaad, 15, 0xbe523597, 15, 0.11527422572175930),
    "450c":     (16, 0x450c2d35, 15, 0xcaf649a9, 16, 0.12242779617496825),
    "d303":     (16, 0xd30332d3, 15, 0xc865c965, 15, 0.12786),
    "1908":     (17, 0x19085565, 16, 0xaf1d6b47, 16, 0.1401417246),
    "lowbias32": (16, 0x7feb352d, 15, 0x846ca68b, 16, 0.17353355999581582),
    "fmix32":   (16, 0x85ebca6b, 13, 0xc2b2ae35, 16, 0.26398543281818287),
}

if __name__ == "__main__":
    n, seed, out = int(sys.argv[1]), int(sys.argv[2]), sys.argv[3]
    rng = random.Random(seed)
    highs = [(rng.getrandbits(16), rng.getrandbits(16)) for _ in range(n)]
    with open(out, "w", newline="\n") as f:
        f.write("spec,core,draw\n")
        for name, (s0, m0, s1, m1, s2, _) in SOURCES.items():
            for i, (h0, h1) in enumerate(highs):
                a = h0 << 16 | (m0 & 0xFFFF)
                b = h1 << 16 | (m1 & 0xFFFF)
                f.write(f"[{s0} {a:08x} {s1} {b:08x} {s2}],{name},{i}\n")
