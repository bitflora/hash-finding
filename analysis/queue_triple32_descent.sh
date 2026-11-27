#!/usr/bin/env bash
# Exact-scored three-round descents (see analysis/triple32_descent.py).
set -euo pipefail
cd "$(dirname "$0")/.."
OUT=results/triple32_memetic/descent
python analysis/triple32_descent.py $OUT/ils_record ils --iters 150 --kick-min 3 --kick-max 6 --seed 1
python analysis/triple32_descent.py $OUT/restarts_17_11_15_14 restarts --count 30 --pool 32 --seed 1
python analysis/triple32_descent.py $OUT/schedules_13_19 schedules --range 13..19 --top 20 \
  --donors "[17 ad5ad4bb 11 ac4c1b51 15 b9848bab 14]" "[16 a16f7153 12 3bcd8663 15 3d06b66b 16]"
