"""Compare single-bit BIC and shift-forced two-bit parities in the three-round campaign."""

import argparse
import csv
import math
import os
import re
import subprocess
from itertools import combinations

from triple32_memetic import DATA, ROOT, fmt, load_scores

BASELINE = "[17 ed5ad4bb 11 ac4c1b51 15 31848bab 14]"
CONTROLS = [
    "[16 4b0332d3 15 549a3597 15]",
    "[16 21f0aaad 15 735a2d97 15]",
    "[16 7feb352d 15 846ca68b 16]",
]
BIC = re.compile(
    r"seed=(\d+) rms z=([\d.]+).*?max\|z\|=([\d.]+).*?"
    r"worst: in (\d+) -> out (\d+),(\d+) corr ([+-][\d.]+)"
)
HEADER = ("group", "spec", "samples", "seed", "rms_z", "max_z",
          "input_bit", "out_j", "out_k", "corr")


def bic(binary, writer, group, spec, power, seeds):
    result = subprocess.check_output(
        [binary, "bic", "--cpu", "--bits", "32", "--spec", spec,
         "--samples", f"p{power}", "--seeds", ",".join(map(str, seeds))], text=True,
    )
    rows = BIC.findall(result)
    if [int(row[0]) for row in rows] != seeds:
        raise RuntimeError(f"missing BIC rows for {spec}: {result}")
    for row in rows:
        writer.writerow((group, spec, 1 << power, *row))
    return max(float(row[2]) for row in rows)


def forced_two_bit(schedule):
    """F41: propagate the lowest differing bit, then invert the last shift."""
    first, *middle, last = schedule
    for a, b in combinations(range(32), 2):
        mask = (1 << a) | (1 << b)
        shifted = mask ^ (mask >> first)
        low = (shifted & -shifted).bit_length() - 1 - sum(middle)
        if low < 0:
            continue
        fixed = []
        for j in range(low + 1):
            if j + last <= low:
                fixed.append(((j,), j + last == low))
            else:
                fixed.append((tuple(range(j, 32, last)), j == low))
        shortest = min(len(bits) for bits, _ in fixed)
        for bits, flips in fixed:
            if len(bits) == shortest:
                yield a, b, "/".join(map(str, bits)), shortest, int(flips)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", nargs="?", default="screen",
                        choices=("screen", "confirm-alternates", "exact-pairs"))
    args = parser.parse_args()
    binary = ROOT / "harness" / "target-triple32" / "release" / "mixlab"
    if os.name == "nt":
        binary = binary.with_suffix(".exe")
    output = DATA / "bit_independence"
    if args.mode == "exact-pairs":
        exact_pairs(binary.parent / "examples" / f"flip_pair{binary.suffix}", output)
        return
    schedules = load_scores(DATA / "extended_shifts" / "schedules_polished.csv", "bias")
    if args.mode == "confirm-alternates":
        with (output / "bic.csv").open() as stream:
            done = {r["spec"] for r in csv.DictReader(stream) if int(r["samples"]) == 1 << 20}
        with (output / "bic.csv").open("a", newline="", buffering=1) as stream:
            for candidate in schedules:
                spec = fmt(candidate)
                if spec not in done:
                    bic(binary, csv.writer(stream), "alternate", spec, 20, [1, 2, 3, 4, 5])
                    print(f"Confirmed {spec}", flush=True)
        return
    output.mkdir()
    scores = load_scores(DATA / "cap_race_01" / "leaderboard.csv", "bias")
    for folder, file, column in [
        ("extended_radius_two", "neighborhood.csv", "bias"),
        ("extended_seeded", "leaderboard.csv", "bias"),
    ]:
        scores.update(load_scores(DATA / folder / file, column))
    seeds = load_scores(DATA / "seeds.csv", "published_bias")
    winners = sorted((c for c in scores if scores[c] < min(seeds.values())), key=scores.get)
    panel = {fmt(c): "winner" for c in winners} | {fmt(c): "alternate" for c in schedules}
    panel[BASELINE] = "published"
    with (output / "bic.csv").open("w", newline="", buffering=1) as stream:
        writer = csv.writer(stream)
        writer.writerow(HEADER)
        outliers = []
        for i, (spec, group) in enumerate(panel.items(), 1):
            max_z = bic(binary, writer, group, spec, 18, [1])
            if max_z >= 6:
                outliers.append(spec)
            if i % 50 == 0:
                print(f"Screened {i} / {len(panel)} three-round specs", flush=True)
        confirm = [fmt(c) for c in winners[:8]] + [
            fmt(c) for c in sorted(schedules, key=schedules.get)[:5]
        ] + [BASELINE, *outliers, *CONTROLS]
        for spec in dict.fromkeys(confirm):
            bic(binary, writer, panel.get(spec, "two_round_control"), spec, 20, [1, 2, 3, 4, 5])
            print(f"Confirmed {spec}", flush=True)

    with (output / "two_bit_parities.csv").open("w", newline="") as stream:
        writer = csv.writer(stream)
        writer.writerow(("schedule", "input_j", "input_k", "output_bits", "weight", "parity_flips"))
        for schedule in sorted({c[::2] for c in winners} | {c[::2] for c in schedules}):
            name = "/".join(map(str, schedule))
            for row in forced_two_bit(schedule):
                writer.writerow((name, *row))


def exact_pairs(binary, output):
    from triple32_extended import RECORD

    record = fmt(RECORD)
    alt = "[16 64767aad 12 daa18229 16 09e53beb 16]"
    checks = [(record, "80004000", 4, 18), (record, "80004000", 5, 19),
              (alt, "80008000", 0, 16), (alt, "80008000", 3, 19)]
    with (output / "bic.csv").open() as stream:
        outliers = {
            (r["spec"], f"{1 << int(r['input_bit']):08x}", int(r["out_j"]), int(r["out_k"]))
            for r in csv.DictReader(stream)
            if r["group"] == "alternate" and int(r["samples"]) == 1 << 20
            and abs(float(r["corr"])) >= 0.01
        }
    with (output / "exact_pairs.csv").open("w", newline="", buffering=1) as stream:
        writer = csv.writer(stream)
        writer.writerow(("spec", "input_mask", "out_j", "out_k", "unique_input_pairs",
                         "n00", "n01", "n10", "n11", "corr"))
        for spec, mask, j, k in checks + sorted(outliers):
            row = subprocess.check_output(
                [binary, spec, mask, str(j), str(k), "exact"], text=True,
            ).strip().split(",")
            n, a, b, c, d = map(int, row[4:])
            assert n == a + b + c + d == 1 << 31
            numerator = a * d - b * c
            variance = (a + b) * (a + c) * (b + d) * (c + d)
            assert variance > 0, "both flip indicators must vary for correlation"
            corr = math.copysign(1.0, numerator) if numerator**2 == variance else (
                numerator / math.sqrt(variance)
            )
            writer.writerow([*row, corr])
            print(f"{spec} delta={mask} out={j},{k} corr={corr:+.9f}", flush=True)


if __name__ == "__main__":
    main()
