#!/bin/sh
# Racing validation traces (see plan.md). Run from mixing-functions/harness.
set -e
O=../results/race_validation
./target/release/mixlab trace --bits 24 --start "[12 2f6b4d 11 a5c9d3 12]" --path 40 --every 5 --neighbours 96 --reps 8 --n0 p10 --cap p24 --seed 1 --out $O/trace24_a.csv 2> $O/trace24_a.log
./target/release/mixlab trace --bits 24 --start "[12 eb352d 11 6ca68b 12]" --path 40 --every 5 --neighbours 96 --reps 8 --n0 p10 --cap p24 --seed 2 --out $O/trace24_b.csv 2> $O/trace24_b.log
./target/release/mixlab trace --bits 32 --list $O/issue19_exact.csv --bases 3 --reps 4 --n0 p14 --cap p28 --seed 3 --out $O/trace32_issue19.csv 2> $O/trace32_issue19.log
