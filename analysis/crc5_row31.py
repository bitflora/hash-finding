"""Hold m0 and the shift fixed; race m1 on whole-mixer and bit-31 error."""
import csv
from itertools import combinations
import random
import subprocess

from short_memetic import BIN, DATA

A = 0x40bb4a67
B = 0x795cb33b


def run():
    rng = random.Random(53100)
    pool = {B, *(B ^ (1 << i) for i in range(1, 32))}
    pool.update(B ^ (1 << i) ^ (1 << j) for i, j in combinations(range(1, 32), 2))
    pool.update((rng.getrandbits(16) << 16) | (B & 65535) for _ in range(256))
    pool.update(rng.getrandbits(32) | 1 for _ in range(256))

    with (DATA / "crc5_row31.csv").open("w", newline="") as stream:
        out = csv.writer(stream)
        out.writerow("stage m0 m1 shift n seed full_ms row31_ms max_abs".split())

        def race(stage, candidates, n, seed, exact=False):
            family = "crc5-exact-row31" if exact else "crc5-detail"
            candidates = sorted(set(candidates))
            specs = "".join(f"{family} {A:x} {b:x} 0 16\n" for b in candidates)
            lines = subprocess.run([BIN, str(n), str(seed)], input=specs, text=True,
                                   stdout=subprocess.PIPE, check=True).stdout.splitlines()
            scored = []
            for b, line in zip(candidates, lines, strict=True):
                values = list(map(float, line.split(",")))
                if exact:
                    row, worst = values
                    full = ""
                else:
                    full, worst, row = values
                out.writerow([stage, f"{A:08x}", f"{b:08x}", 16, n, seed, full, row, worst])
                scored.append((b, full, row))
            stream.flush()
            return scored

        for stage, n, seed, keep in [("screen", 1 << 15, 53101, 48),
                                      ("race", 1 << 18, 53102, 12),
                                      ("select", 1 << 21, 53103, 1)]:
            scores = race(stage, pool, n, seed)
            full = sorted(scores, key=lambda r: r[1])[:keep]
            row = sorted(scores, key=lambda r: r[2])[:keep]
            pool = {B, *(r[0] for r in full + row)}
        for seed in (53104, 53105):
            race("validate", pool, 1 << 24, seed)
        race("exact_row", pool, 1 << 32, 0, exact=True)


if __name__ == "__main__":
    run()
