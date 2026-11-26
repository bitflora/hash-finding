"""Race the upper multiplier bits of the best exact three-round seed."""

import argparse
import os
from pathlib import Path

from triple32_memetic import (
    DATA, ROOT, exact, fmt, load_scores, neighbors, replace, write_leaderboard,
)

CAP_BITS = 5
LOW_MASK = (1 << (32 - CAP_BITS)) - 1


def caps(candidate, slot):
    low = candidate[slot] & LOW_MASK
    for cap in range(1 << CAP_BITS):
        yield replace(candidate, slot, low | (cap << (32 - CAP_BITS)))


def top(scores, count):
    return sorted(scores, key=lambda candidate: (scores[candidate], candidate))[:count]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("--from-run", type=Path, default=DATA / "exact_refinement")
    args = parser.parse_args()
    previous, output = args.from_run.resolve(), args.output.resolve()
    if not all(path.is_relative_to(DATA) and path != DATA for path in (previous, output)):
        parser.error("both run paths must be under results/triple32_memetic")
    scores = load_scores(previous / "leaderboard.csv", "bias")
    seeds = load_scores(DATA / "seeds.csv", "published_bias")
    leader = top(scores, 1)[0]
    output.mkdir()
    binary = ROOT / "harness" / "target-triple32" / "release" / "mixlab"
    if os.name == "nt":
        binary = binary.with_suffix(".exe")

    outer_grid = {c for a in caps(leader, 1) for c in caps(a, 5)}
    for gen in range(3):
        if gen == 0:
            pool = outer_grid
        elif gen == 1:
            known = seeds | scores
            outer_scores = {c: known[c] for c in outer_grid}
            pool = {c for parent in top(outer_scores, 16) for c in caps(parent, 3)}
        else:
            pool = {c for parent in top(scores, 8) for c in neighbors(parent)}
        fresh = sorted(pool - scores.keys() - seeds.keys())
        if fresh:
            scores.update(exact(binary, "--gpu", output, gen, fresh))
        best = top(scores, 1)[0]
        print(f"g{gen}: {len(fresh)} fresh exact scores; best {fmt(best)} {scores[best]}",
              flush=True)

    write_leaderboard(output, scores)


if __name__ == "__main__":
    main()
