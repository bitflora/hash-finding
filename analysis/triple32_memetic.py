"""Seeded memetic search for three-round, 32-bit xorshift/multiply mixers."""

import argparse
import csv
import os
import random
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DATA = ROOT / "results" / "triple32_memetic"
MULTIPLIERS = (1, 3, 5)
SHIFTS = (0, 2, 4, 6)


def parse(spec):
    fields = spec.strip("[]").split()
    if len(fields) != 7:
        raise ValueError(f"expected three multipliers and four shifts: {spec}")
    candidate = tuple(int(s, 16 if i in MULTIPLIERS else 10) for i, s in enumerate(fields))
    if any(not (0 < candidate[i] < 1 << 32 and candidate[i] & 1) for i in MULTIPLIERS):
        raise ValueError(f"expected odd 32-bit multipliers: {spec}")
    if any(not 1 <= candidate[i] <= 31 for i in SHIFTS):
        raise ValueError(f"shifts must be in 1..31: {spec}")
    return candidate


def fmt(candidate):
    return "[" + " ".join(f"{x:08x}" if i in MULTIPLIERS else str(x)
                          for i, x in enumerate(candidate)) + "]"


def replace(candidate, i, value):
    return candidate[:i] + (value,) + candidate[i + 1:]


def neighbors(candidate):
    for i in MULTIPLIERS:
        for bit in range(1, 32):
            yield replace(candidate, i, candidate[i] ^ (1 << bit))
    for i in SHIFTS:
        for offset in (-1, 1):
            if 1 <= candidate[i] + offset <= 31:
                yield replace(candidate, i, candidate[i] + offset)


def crossover(a, b, rng):
    i = rng.choice(MULTIPLIERS)
    cut = rng.choice((8, 12, 16, 20, 24, 32))
    low = (1 << cut) - 1
    child = replace(a, i, (a[i] & ~low) | (b[i] & low))
    if rng.random() < 0.25:
        j = rng.choice(SHIFTS)
        child = replace(child, j, b[j])
    return child


def screen(worker, writer, stage, candidates, lg, seed):
    candidates = sorted(set(candidates))
    scores = {}
    for start in range(0, len(candidates), 64):
        batch = candidates[start:start + 64]
        worker.stdin.write(f"{lg} {seed} {len(batch)}\n")
        worker.stdin.writelines(fmt(candidate) + "\n" for candidate in batch)
        worker.stdin.flush()
        for candidate in batch:
            line = worker.stdout.readline()
            if not line:
                raise RuntimeError("sampling worker stopped")
            scores[candidate] = float(line)
            writer.writerow([stage, fmt(candidate), 1 << lg, seed, scores[candidate]])
    print(f"{stage}: {len(scores)} candidates at p{lg}", flush=True)
    return sorted(scores, key=scores.get)


def exact(binary, backend, output, generation, candidates):
    candidates = list(dict.fromkeys(candidates))
    path = output / f"generation_{generation}_exact.csv"
    subprocess.run(
        [binary, "exact", backend, "--bits", "32", "--list", "-", "--out", path],
        input="".join(fmt(candidate) + "\n" for candidate in candidates),
        text=True, check=True,
    )
    scores = load_scores(path, "bias")
    if set(scores) != set(candidates):
        raise RuntimeError("exact output differs from requested candidates")
    return scores


def load_scores(path, column):
    with path.open() as stream:
        return {parse(row["spec"]): float(row[column]) for row in csv.DictReader(stream)}


def write_leaderboard(output, measured):
    with (output / "leaderboard.csv").open("w", newline="") as stream:
        board = csv.writer(stream)
        board.writerow(("spec", "bias"))
        for candidate in sorted(measured, key=measured.get):
            board.writerow((fmt(candidate), measured[candidate]))


def campaign(worker, binary, backend, writer, output, generations):
    seeds = load_scores(DATA / "seeds.csv", "published_bias")
    baseline = min(seeds, key=seeds.get)
    rng = random.Random(20261126)
    population = list(seeds)
    measured = {}
    for gen in range(generations):
        elite = population[:16]
        children = set()
        while len(children) < 192:
            a, b = rng.sample(population, 2)
            child = crossover(a, b, rng)
            if rng.random() < 0.35:
                child = rng.choice(list(neighbors(child)))
            if child not in population:
                children.add(child)

        local = set(neighbors(elite[0]))
        for parent in population[1:]:
            moves = list(neighbors(parent))
            local.update(rng.sample(moves, 10))
            for i in MULTIPLIERS:
                local.add(replace(parent, i, parent[i] ^ (1 << 31)))
        seed = 46000 + 10 * gen
        cross_ranked = screen(worker, writer, f"g{gen}_cross", children, 23, seed)
        for parent in cross_ranked[:8]:
            local.update(rng.sample(list(neighbors(parent)), 12))
        initial = screen(worker, writer, f"g{gen}_local",
                         local | set(cross_ranked[:64]) | set(population), 24, seed + 1)
        promoted = [c for c in initial if c not in measured and c not in seeds][:112]
        mid = screen(worker, writer, f"g{gen}_promote",
                     promoted + elite[:4], 26, seed + 2)
        promoted = [c for c in mid if c not in measured and c not in seeds][:32]
        final = screen(worker, writer, f"g{gen}_select",
                       promoted + [baseline, elite[0]], 29, seed + 3)
        finalists = [c for c in final if c not in measured and c not in seeds][:10]
        if gen == 0:
            finalists.insert(0, baseline)
        measured.update(exact(binary, backend, output, gen, finalists))
        if abs(measured[baseline] - seeds[baseline]) > 1e-15:
            raise RuntimeError("exact baseline differs from the published score")
        best = min(measured, key=measured.get)
        print(f"g{gen} exact best: {fmt(best)} {measured[best]:.17g}", flush=True)
        known = seeds | measured
        population = list(dict.fromkeys([*sorted(known, key=known.get)[:14], *final[:12]]))

    write_leaderboard(output, measured)


def refine(binary, backend, output, generations, previous):
    seeds = load_scores(DATA / "seeds.csv", "published_bias")
    measured = load_scores(previous / "leaderboard.csv", "bias")
    rng = random.Random(20261127)
    for gen in range(generations):
        known = seeds | measured
        ranked = sorted(known, key=known.get)
        parents = list(seeds) if gen == 0 else ranked[:16]
        pool = {move for parent in ranked[:3] for move in neighbors(parent)}
        for parent in parents:
            # High-bit moves preserve all three low-16 multiplier cores.
            for i in MULTIPLIERS:
                for bit in range(28, 32):
                    pool.add(replace(parent, i, parent[i] ^ (1 << bit)))
        for _ in range(160):
            a, b = rng.sample(parents, 2)
            i = rng.choice(MULTIPLIERS)
            child = replace(a, i, (a[i] & 0xffff) | (b[i] & 0xffff0000))
            pool.add(child)
        measured.update(exact(binary, backend, output, gen, sorted(pool - known.keys())))
        best = min(measured, key=measured.get)
        print(f"g{gen} exact best: {fmt(best)} {measured[best]:.17g}", flush=True)
    write_leaderboard(output, measured)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path, help="new run directory inside the campaign directory")
    parser.add_argument("--bin-dir", type=Path,
                        default=ROOT / "harness" / "target-triple32" / "release")
    parser.add_argument("--generations", type=int, default=3)
    parser.add_argument("--refine-from", type=Path, help="completed sampled run to refine exactly")
    parser.add_argument("--cpu", action="store_true")
    args = parser.parse_args()
    output = args.output.resolve()
    if not output.is_relative_to(DATA) or output == DATA or args.generations < 1:
        parser.error("provide a new run directory under results/triple32_memetic and positive generations")
    suffix = ".exe" if os.name == "nt" else ""
    binary = args.bin_dir.resolve() / ("mixlab" + suffix)
    sampler = binary.parent / "examples" / ("triple32_screen" + suffix)
    backend = "--cpu" if args.cpu else "--gpu"
    if args.refine_from:
        args.refine_from = args.refine_from.resolve()
        if not args.refine_from.is_relative_to(DATA) or not (args.refine_from / "leaderboard.csv").is_file():
            parser.error("--refine-from needs a completed run under results/triple32_memetic")
    output.mkdir()
    if args.refine_from:
        refine(binary, backend, output, args.generations, args.refine_from)
        return
    with (output / "screen.csv").open("w", newline="", buffering=1) as stream:
        writer = csv.writer(stream)
        writer.writerow(("stage", "spec", "samples", "seed", "sampled_ms"))
        with subprocess.Popen([sampler, backend], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                              text=True) as worker:
            try:
                campaign(worker, binary, backend, writer, output, args.generations)
            finally:
                worker.stdin.close()
            if worker.wait() != 0:
                raise RuntimeError("sampling worker failed")


if __name__ == "__main__":
    main()
