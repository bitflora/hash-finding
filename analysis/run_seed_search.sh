#!/bin/sh
# Full 32-bit seed-search comparison (2026-11-20): 4 populations x 3 seeds,
# 64 candidates each, screened at 2^22 samples, best 4 verified exactly.
cd "$(dirname "$0")/../harness" || exit 1
R=../results/seed_search32
mkdir -p $R
B=${SEED_SEARCH_BIN:-./target/release/seed-search.exe}
for seed in 1 2 3; do
  for src in spectral random matched mixer; do
    out=$R/${src}_s$seed.csv
    [ -f $out.verified.csv ] && continue
    rm -f $out
    $B --source $src --bits 32 --count 64 --samples p22 --seed $seed --verify 4 --out $out \
      > $R/${src}_s$seed.log 2>&1
  done
done
echo "seed search done"
