#!/usr/bin/env bash
# Round two: continue ILS from the round-one leaders.
set -euo pipefail
cd "$(dirname "$0")/.."
OUT=results/triple32_memetic/descent
python analysis/triple32_descent.py $OUT/ils_record2 ils --start "[17 895245bb 10 a04c13d1 15 cac49ba3 14]" --iters 150 --kick-min 3 --kick-max 6 --seed 2
python analysis/triple32_descent.py $OUT/ils_restart2 ils --start "[17 6c32c163 11 2f92edc9 15 ea50d969 14]" --iters 80 --kick-min 3 --kick-max 6 --seed 3
