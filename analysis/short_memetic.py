"""Crossover and local races for the three short-race families (run from any cwd)."""
import argparse
import csv
import os
import random
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DATA = ROOT / "results/short_core_race"
BIN = ROOT / "harness/target/release" / ("short-race.exe" if os.name == "nt" else "short-race")
def log_name(round_no, cores=False):
    suffix = "" if round_no == 1 else f"_r{round_no}"
    return f"memetic{suffix}{'_cores' if cores else ''}.csv"


def spec(row):
    return (int(row["m0"], 16), int(row["m1"], 16), int(row["shift0"]), int(row["shift1"]))


def read_rows(name):
    with (DATA / name).open() as stream:
        return list(csv.DictReader(stream))


def score(writer, family, stage, candidates, samples, seed):
    candidates = list(dict.fromkeys(candidates))
    lines = "".join(f"{family} {a:x} {b:x} {s} {t}\n" for a, b, s, t in candidates)
    result = subprocess.run([BIN, str(samples), str(seed)], input=lines, text=True,
                            stdout=subprocess.PIPE, check=True)
    scores = {}
    for candidate, line in zip(candidates, result.stdout.splitlines(), strict=True):
        a, b, s, t = candidate
        ms, worst = map(float, line.split(","))
        writer.writerow([stage, family, f"{a:08x}", f"{b:08x}", s, t, samples, seed, ms, worst])
        scores[candidate] = ms
    return sorted(candidates, key=scores.get)


def mutate(candidate, family, rng):
    parts = list(candidate)
    idx = rng.randrange(2)
    if family != "crc3" and rng.random() < 0.15:
        idx = rng.choice([2, 3]) if family == "xs6" else 3
        parts[idx] = max(1, min(31, parts[idx] + rng.choice([-2, -1, 1, 2])))
    else:
        parts[idx] ^= 1 << rng.randrange(1, 32)
    return tuple(parts)


def cross(pop, rng):
    p, q = rng.sample(pop, 2)
    a, b = p[:2]
    cut = rng.choice([8, 12, 16, 20, 24, 32])
    mask = (1 << cut) - 1
    if rng.randrange(2):
        a = (a & ~mask) | (q[0] & mask)
    else:
        b = (b & ~mask) | (q[1] & mask)
    return a, b, p[2], p[3]


def run(round_no=1):
    offset = (round_no - 1) * 10000
    previous = read_rows(log_name(round_no - 1)) if round_no > 1 else []
    original = read_rows("races.csv")
    shift = read_rows("crc_shift_race.csv")
    with (DATA / log_name(round_no)).open("w", newline="") as stream:
        writer = csv.writer(stream)
        writer.writerow("stage family m0 m1 shift0 shift1 n seed ms max_abs".split())
        for family in ("crc5", "xs6", "crc3"):
            rng = random.Random(17823 + offset)
            rows = [r for r in original if r["family"] == family]
            confirmed = ([r for r in shift if r["stage"] == "verify"] if family == "crc5"
                         else [r for r in rows if r["stage"] == "confirm"])
            ranked = lambda rs: [spec(r) for r in sorted(rs, key=lambda r: float(r["ms"]))]
            parents = ranked(confirmed)
            pop = list(dict.fromkeys(parents + ranked(r for r in rows if r["stage"] == "holdout")))[:12]
            if previous:
                prior = [r for r in previous if r["family"] == family and r["stage"] == "selection"]
                prior_elites = ranked(prior)
                parents = prior_elites[:1]
                pop = list(dict.fromkeys(prior_elites[:8] + pop))[:12]
            for gen in range(12 if previous else 5):
                pool = set(pop)
                while len(pool) < 192:
                    child = cross(pop, rng)
                    if rng.random() < 0.7:
                        child = mutate(child, family, rng)
                    pool.add(child)
                base = 31000 + offset + gen * 4
                n = 32768 if family == "crc5" else 2048
                screen = score(writer, family, f"cross{gen}", sorted(pool), n, base)
                local = set(screen[:12])
                for elite in screen[:6]:
                    for _ in range(32):
                        local.add(mutate(elite, family, rng))
                refined = score(writer, family, f"local{gen}", sorted(local), n * 2, base + 1)
                pop = score(writer, family, f"race{gen}", pop + screen[:12] + refined[:16],
                            n * 8, base + 2)[:12]
                stream.flush()
            finalists = score(writer, family, "selection", pop + parents, 1 << 21, 32000 + offset)
            winner = finalists[0]
            for seed in (33001, 33002, 33003):
                score(writer, family, "holdout", [winner, *parents], 1 << 22, seed + offset)
            stream.flush()
            print(f"{family} completed", flush=True)


def compare_cores(round_no=1):
    """Give winner and historical parent cores identical fresh upper halves and shifts."""
    offset = (round_no - 1) * 10000
    rows = read_rows(log_name(round_no))
    rng = random.Random(48912 + offset)
    caps = [(rng.getrandbits(16) << 16, rng.getrandbits(16) << 16) for _ in range(32)]
    with (DATA / log_name(round_no, cores=True)).open("w", newline="") as stream:
        writer = csv.writer(stream)
        writer.writerow("stage family m0 m1 shift0 shift1 n seed ms max_abs".split())
        for family in ("crc5", "xs6", "crc3"):
            group = [spec(r) for r in rows if r["family"] == family and r["stage"] == "holdout"
                     and int(r["seed"]) == 33001 + offset]
            s, t = group[0][2:]
            pool = [(hi | (a & 65535), hj | (b & 65535), s, t)
                    for a, b, _, _ in group for hi, hj in caps]
            score(writer, family, "core_panel", pool, 1 << 18, 34001 + offset)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--round", type=int, default=1)
    round_no = parser.parse_args().round
    if round_no < 1:
        parser.error("--round must be positive")
    run(round_no)
    compare_cores(round_no)
