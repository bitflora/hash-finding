"""Exact-ranked beams for the three-round record and published alternate schedules."""

import argparse
import csv
import os
import random
import subprocess

from triple32_bic import HEADER, bic, forced_two_bit
from triple32_extended import RECORD, rank
from triple32_memetic import (
    DATA, ROOT, MULTIPLIERS, exact, fmt, load_scores, neighbors, replace, screen,
)


def history():
    scores = load_scores(DATA / "seeds.csv", "published_bias")
    for folder, file in (
        ("cap_race_01", "leaderboard.csv"),
        ("extended_radius_two", "neighborhood.csv"),
        ("extended_seeded", "leaderboard.csv"),
        ("extended_shifts", "generation_shift_finalists_exact.csv"),
        ("extended_shifts", "generation_shift_polish_exact.csv"),
    ):
        for candidate, bias in load_scores(DATA / folder / file, "bias").items():
            if candidate in scores and abs(scores[candidate] - bias) > 1e-15:
                raise RuntimeError(f"conflicting exact scores for {fmt(candidate)}")
            scores[candidate] = bias
    return scores


def offspring(beam, rng, broad):
    local = set()
    high = set()
    for parent in beam:
        moves = [c for c in neighbors(parent) if c[::2] == parent[::2]]
        local.update(moves if broad else rng.sample(moves, 30))
        for i in MULTIPLIERS:
            for bit in range(25, 32):
                high.add(replace(parent, i, parent[i] ^ (1 << bit)))
        for _ in range(48 if broad else 24):
            a, b = rng.sample(MULTIPLIERS, 2)
            child = replace(parent, a, parent[a] ^ (1 << rng.randrange(21, 32)))
            high.add(replace(child, b, child[b] ^ (1 << rng.randrange(21, 32))))
        for _ in range(48 if broad else 20):
            donor = rng.choice(beam)
            i = rng.choice(MULTIPLIERS)
            cut = rng.choice((16, 22, 26, 28, 30, 31))
            mask = (1 << cut) - 1
            child = replace(parent, i, (parent[i] & mask) | (donor[i] & ~mask))
            local.add(child)
            j = rng.choice(MULTIPLIERS)
            local.add(replace(child, j, child[j] ^ (1 << rng.randrange(1, 32))))
    return local | high, high


def run_beams(binary, worker, output, schedules, scores, generations, broad):
    rng = random.Random(20261130 if broad else 20261201)
    width = 12 if broad else 4
    elite_count = width // 2
    beams = {
        schedule: rank(scores, [c for c in scores if c[::2] == schedule])[:width]
        for schedule in schedules
    }
    with (output / "screen.csv").open("w", newline="", buffering=1) as raw, (
        output / "beam.csv"
    ).open("w", newline="", buffering=1) as states:
        writer = csv.writer(raw)
        writer.writerow(("stage", "spec", "samples", "seed", "sampled_ms"))
        state = csv.writer(states)
        state.writerow(("generation", "schedule", "rank", "spec", "bias"))
        for gen in range(generations + 1):
            for schedule, lane in beams.items():
                name = "/".join(map(str, schedule))
                state.writerows((gen, name, i, fmt(c), scores[c]) for i, c in enumerate(lane, 1))
            print(f"g{gen}: {fmt(min((lane[0] for lane in beams.values()), key=scores.get))}",
                  flush=True)
            if gen == generations:
                break
            finals = {}
            for index, (schedule, lane) in enumerate(beams.items()):
                pool, high = offspring(lane, rng, broad)
                fresh = pool - scores.keys()
                direct = sorted(high & fresh)
                direct = rng.sample(direct, min(len(direct), 96 if broad else 8))
                tag = f"g{gen}_" + "_".join(map(str, schedule))
                seed = 51100 + gen * 100 + index
                first = screen(worker, writer, tag, fresh - set(direct), 24, seed)
                ranked = screen(worker, writer, tag + "_select",
                                first[:192 if broad else 32], 28, seed + 1000)
                finals[schedule] = set(direct) | set(ranked[:128 if broad else 8])
            pool = sorted(set().union(*finals.values()))
            if pool:
                scores.update(exact(binary, "--gpu", output, gen, pool))
            for schedule, previous in beams.items():
                new = rank(scores, finals[schedule])
                ordered = rank(scores, set(previous) | set(new))
                elites = ordered[:elite_count]
                explorers = [c for c in new if c not in elites]
                beams[schedule] = list(dict.fromkeys(elites + explorers + ordered))[:width]

    with (output / "leaders.csv").open("w", newline="") as stream:
        board = csv.writer(stream)
        board.writerow(("schedule", "spec", "bias", "two_bit_masks_with_prefix_witness"))
        for candidate in rank(scores, [lane[0] for lane in beams.values()]):
            schedule = candidate[::2]
            masks = {(a, b) for a, b, *_ in forced_two_bit(schedule)}
            board.writerow(("/".join(map(str, schedule)), fmt(candidate), scores[candidate], len(masks)))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("record", "alternate", "bic"))
    parser.add_argument("--generations", type=int, default=3)
    args = parser.parse_args()
    if args.generations < 1:
        parser.error("--generations must be positive")
    suffix = ".exe" if os.name == "nt" else ""
    binary = ROOT / "harness" / "target-triple32" / "release" / f"mixlab{suffix}"
    output = DATA / f"beam_{args.mode}"
    if args.mode == "bic":
        check_leaders(binary, output)
        return
    output.mkdir()
    scores = history()
    broad = args.mode == "record"
    schedules = [RECORD[::2]] if broad else sorted({
        c[::2] for c in load_scores(DATA / "seeds.csv", "published_bias")
    } - {RECORD[::2]})
    sampler = binary.parent / "examples" / f"triple32_screen{suffix}"
    with subprocess.Popen([sampler, "--gpu"], stdin=subprocess.PIPE,
                          stdout=subprocess.PIPE, text=True) as worker:
        try:
            run_beams(binary, worker, output, schedules, scores, args.generations, broad)
        finally:
            worker.stdin.close()
        if worker.wait():
            raise RuntimeError("sampling worker failed")


def check_leaders(binary, output):
    output.mkdir()
    panel = {}
    for mode in ("record", "alternate"):
        with (DATA / f"beam_{mode}" / "leaders.csv").open() as stream:
            entries = list(csv.DictReader(stream))
        for row in entries[:5]:
            panel[row["spec"]] = mode
        clean = next((row for row in entries if int(row["two_bit_masks_with_prefix_witness"]) == 0), None)
        if clean:
            panel[clean["spec"]] = "no_two_bit_prefix_witness"
    with (output / "bic.csv").open("w", newline="", buffering=1) as stream:
        writer = csv.writer(stream)
        writer.writerow(HEADER)
        for spec, group in panel.items():
            bic(binary, writer, group, spec, 20, [1, 2, 3, 4, 5])


if __name__ == "__main__":
    main()
