"""Race low-16 multiplier pairs with matched upper halves; run after building short-race."""
import csv
import os
import random
import statistics
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BIN = ROOT / "harness/target/release" / ("short-race.exe" if os.name == "nt" else "short-race")
OUT = ROOT / "results/short_core_race"
OUT.mkdir(exist_ok=True)
rng = random.Random(20261126)
families = {"crc3": (0, 0), "crc5": (0, 15), "xs6": (20, 15)}


def core():
    return rng.randrange(1, 65536, 2), rng.randrange(1, 65536, 2)


def caps(count):
    return [(rng.getrandbits(16), rng.getrandbits(16)) for _ in range(count)]


def batch(writer, stage, family, pairs, uppers, n, seed):
    s, t = families[family]
    rows = [(pair, hi << 16 | pair[0], hj << 16 | pair[1])
            for pair in pairs for hi, hj in uppers]
    specs = "".join(f"{family} {a:x} {b:x} {s} {t}\n" for _, a, b in rows)
    output = subprocess.run([BIN, str(n), str(seed)], input=specs, text=True,
                            stdout=subprocess.PIPE, check=True).stdout
    scores = {}
    for (pair, a, b), line in zip(rows, output.splitlines(), strict=True):
        ms, worst = map(float, line.split(","))
        scores.setdefault(pair, []).append(ms)
        writer.writerow([stage, family, f"{a:08x}", f"{b:08x}", s, t, n, seed, ms, worst])
    return sorted(scores, key=lambda pair: statistics.median(scores[pair]))


initial = list(dict.fromkeys([(0x32d3, 0x3597), (0xaaad, 0x2d97)] + [core() for _ in range(1022)]))
controls = [core() for _ in range(32)]
panels = [caps(k) for k in (3, 8, 16)]
with (OUT / "races.csv").open("w", newline="") as stream:
    writer = csv.writer(stream)
    writer.writerow("stage family m0 m1 shift0 shift1 n seed ms max_abs".split())
    for family in families:
        first = batch(writer, "screen", family, initial, panels[0], 256, 301)
        second = batch(writer, "race", family, first[:64], panels[1], 2048, 302)
        batch(writer, "holdout", family, second[:4], panels[2], 16384, 303)
        batch(writer, "control", family, controls, panels[2], 16384, 303)
        stream.flush()
        print(f"{family} complete", flush=True)

# Confirm each family's winning full mixer on an independent, larger sample.
with (OUT / "races.csv").open() as stream:
    rows = list(csv.DictReader(stream))
with (OUT / "races.csv").open("a", newline="") as stream:
    writer = csv.writer(stream)
    for family in families:
        best = min((r for r in rows if r["stage"] == "holdout" and r["family"] == family),
                   key=lambda r: float(r["ms"]))
        a, b = (int(best[k], 16) for k in ("m0", "m1"))
        batch(writer, "confirm", family, [(a & 65535, b & 65535)],
              [(a >> 16, b >> 16)], 1 << 21, 304)

# Tune the final shift on the four surviving CRC cores, then confirm on fresh inputs.
crc_pairs = sorted({(int(r["m0"][4:], 16), int(r["m1"][4:], 16)) for r in rows
                    if r["stage"] == "holdout" and r["family"] == "crc5"})
with (OUT / "crc_shift_race.csv").open("w", newline="") as stream:
    writer = csv.writer(stream)
    writer.writerow("stage family m0 m1 shift0 shift1 n seed ms max_abs".split())
    for shift in range(8, 23):
        families["crc5"] = (0, shift)
        batch(writer, "shift", "crc5", crc_pairs, panels[2], 1 << 15, 401)
for stage, next_stage, keep, n, seed in [
    ("shift", "finalist", 32, 1 << 19, 402),
    ("finalist", "verify", 4, 1 << 22, 403),
]:
    path = OUT / "crc_shift_race.csv"
    with path.open() as stream:
        finalists = sorted((r for r in csv.DictReader(stream) if r["stage"] == stage),
                           key=lambda r: float(r["ms"]))[:keep]
    with path.open("a", newline="") as stream:
        for r in finalists:
            a, b = (int(r[k], 16) for k in ("m0", "m1"))
            families["crc5"] = (0, int(r["shift1"]))
            batch(csv.writer(stream), next_stage, "crc5", [(a & 65535, b & 65535)],
                  [(a >> 16, b >> 16)], n, seed)
