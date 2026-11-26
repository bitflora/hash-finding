"""Exact radius-two check and seeded three-round search races."""

import argparse
import csv
import os
import random
import subprocess
from pathlib import Path

from triple32_memetic import (
    DATA, ROOT, MULTIPLIERS, SHIFTS, crossover, exact, fmt, load_scores,
    neighbors, parse, replace, screen, write_leaderboard,
)

RECORD = parse("[17 ad5ad4bb 11 ac4c1b51 15 b9848bab 14]")


def rank(scores, candidates):
    return sorted(candidates, key=lambda candidate: (scores[candidate], candidate))


def measure(binary, output, stage, pool, known):
    fresh = sorted(set(pool) - known.keys())
    path = output / f"generation_{stage}_exact.csv"
    if path.exists():
        scores = load_scores(path, "bias")
        if scores.keys() != set(fresh):
            raise RuntimeError(f"incomplete or mismatched checkpoint {path}")
    else:
        scores = exact(binary, "--gpu", output, stage, fresh) if fresh else {}
    known.update(scores)
    print(f"{stage}: {len(fresh)} fresh exact scores", flush=True)


def radius_two(binary, output, known):
    one = set(neighbors(RECORD))
    two = {child for parent in one for child in neighbors(parent)} - one - {RECORD}
    fresh = sorted((one | two) - known.keys())
    print(f"Radius two: {len(one)} at distance one, {len(two)} at distance two, "
          f"{len(fresh)} fresh", flush=True)
    for start in range(0, len(fresh), 640):
        batch = fresh[start:start + 640]
        measure(binary, output, start // 640, batch, known)
        best = rank(known, (one | two | {RECORD}) & known.keys())[0]
        print(f"Best checked: {fmt(best)} {known[best]}", flush=True)
    with (output / "neighborhood.csv").open("w", newline="") as stream:
        writer = csv.writer(stream)
        writer.writerow(("spec", "distance", "bias"))
        writer.writerows((fmt(c), 0 if c == RECORD else 1 if c in one else 2, known[c])
                         for c in rank(known, one | two | {RECORD}))


def seeded(binary, worker, writer, output, known, original, baseline):
    winners = rank(original, [c for c in original if original[c] < baseline])
    assert len(winners) == 76
    rng = random.Random(20261128)
    population = winners
    evaluated = set(winners)
    for gen in range(4):
        pool = set()
        for parent in winners:
            pool.update(rng.sample(list(neighbors(parent)), 12))
            for _ in range(6):
                mate = rng.choice(population)
                child = crossover(parent, mate, rng)
                if rng.random() < 0.4:
                    child = rng.choice(list(neighbors(child)))
                pool.add(child)
        for parent in population[:8]:
            pool.update(neighbors(parent))
        seed = 48000 + 4 * gen
        initial = screen(worker, writer, f"g{gen}_screen", pool - known.keys(), 24, seed)
        local = set(initial[:128])
        for child in initial[:12]:
            local.update(rng.sample(list(neighbors(child)), 10))
        promoted = screen(worker, writer, f"g{gen}_local", local - known.keys(), 27, seed + 1)
        finalists = screen(worker, writer, f"g{gen}_select", promoted[:64], 29, seed + 2)
        selected = finalists[:32]
        measure(binary, output, gen, selected, known)
        evaluated.update(selected)
        population = rank(known, set(population) | set(selected))[:32]
        print(f"g{gen} seeded best: {fmt(population[0])} {known[population[0]]}", flush=True)
    write_leaderboard(output, {c: known[c] for c in sorted(evaluated)})


def shifts(binary, worker, writer, output, known, seeds):
    groups = {}
    for candidate in rank(seeds, seeds):
        groups.setdefault(candidate[::2], []).append(candidate)
    del groups[RECORD[::2]]
    rng = random.Random(20261129)
    sampled = []
    exact_pool = set()
    for index, (schedule, parents) in enumerate(sorted(groups.items())):
        transfer = tuple(value for pair in zip(schedule, RECORD[1::2]) for value in pair) + (schedule[-1],)
        pool = set()
        for parent in parents + [transfer]:
            for i in MULTIPLIERS:
                for bit in range(26, 32):
                    pool.add(replace(parent, i, parent[i] ^ (1 << bit)))
            pool.update(rng.sample([c for c in neighbors(parent) if c[::2] == schedule], 32))
        for _ in range(32):
            a, b = rng.sample(parents + [transfer], 2)
            pool.add(crossover(a, b, rng))
        name = "_".join(map(str, schedule))
        ranked = screen(worker, writer, name, pool - known.keys(), 25, 49000 + index)
        finalists = screen(worker, writer, f"{name}_select", ranked[:16], 28, 49100 + index)
        exact_pool.update(finalists[:6])
        exact_pool.add(transfer)
        sampled.append((schedule, parents, transfer, pool))
    measure(binary, output, "shift_finalists", exact_pool, known)
    rows = []
    for schedule, parents, transfer, pool in sampled:
        best = rank(known, (pool | set(parents) | {transfer}) & known.keys())[0]
        rows.append(("/".join(map(str, schedule)), fmt(parents[0]), known[parents[0]],
                     fmt(best), known[best], known[transfer]))
        print(f"{schedule} best: {fmt(best)} {known[best]}", flush=True)
    with (output / "schedules.csv").open("w", newline="") as stream:
        report = csv.writer(stream)
        report.writerow(("schedule", "published_spec", "published_bias", "spec", "bias", "transfer_bias"))
        report.writerows(rows)


def shift_polish(binary, output, known):
    known.update(load_scores(output / "generation_shift_finalists_exact.csv", "bias"))
    with (output / "schedules.csv").open() as stream:
        reader = csv.DictReader(stream)
        fields = reader.fieldnames
        rows = list(reader)

    pools = {}
    for row in rows:
        anchor = parse(row["spec"])
        # Cross every setting of the first and third multipliers' top two
        # bits, and separately sweep those bits in the middle multiplier.
        pools[anchor] = {
            replace(replace(anchor, 1, anchor[1] ^ (first << 30)), 5, anchor[5] ^ (third << 30))
            for first in range(4) for third in range(4)
        } | {replace(anchor, 3, anchor[3] ^ (middle << 30)) for middle in range(1, 4)}
    measure(binary, output, "shift_polish", set().union(*pools.values()), known)
    for row in rows:
        best = rank(known, pools[parse(row["spec"])])[0]
        row["spec"], row["bias"] = fmt(best), known[best]
    with (output / "schedules_polished.csv").open("w", newline="") as stream:
        report = csv.DictWriter(stream, fields)
        report.writeheader()
        report.writerows(rows)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("radius_two", "seeded", "shifts", "shift_polish"))
    args = parser.parse_args()
    output = DATA / f"extended_{'shifts' if args.mode == 'shift_polish' else args.mode}"
    if args.mode != "shift_polish":
        output.mkdir(exist_ok=args.mode == "radius_two")
    seeds = load_scores(DATA / "seeds.csv", "published_bias")
    original = load_scores(DATA / "exact_refinement" / "leaderboard.csv", "bias")
    caps = load_scores(DATA / "cap_race_01" / "leaderboard.csv", "bias")
    known = seeds | original | caps
    suffix = ".exe" if os.name == "nt" else ""
    binary = ROOT / "harness" / "target-triple32" / "release" / ("mixlab" + suffix)
    if args.mode == "radius_two":
        radius_two(binary, output, known)
        return
    if args.mode == "shift_polish":
        shift_polish(binary, output, known)
        return
    sampler = binary.parent / "examples" / ("triple32_screen" + suffix)
    with (output / "screen.csv").open("w", newline="", buffering=1) as stream:
        writer = csv.writer(stream)
        writer.writerow(("stage", "spec", "samples", "seed", "sampled_ms"))
        with subprocess.Popen([sampler, "--gpu"], stdin=subprocess.PIPE,
                              stdout=subprocess.PIPE, text=True) as worker:
            try:
                if args.mode == "seeded":
                    seeded(binary, worker, writer, output, known, original, min(seeds.values()))
                else:
                    shifts(binary, worker, writer, output, known, seeds)
            finally:
                worker.stdin.close()
            if worker.wait() != 0:
                raise RuntimeError("sampling worker failed")


if __name__ == "__main__":
    main()
