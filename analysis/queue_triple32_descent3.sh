#!/usr/bin/env bash
# Round three: continue ILS from the round-two leaders.
set -euo pipefail
cd "$(dirname "$0")/.."
OUT=results/triple32_memetic/descent
[ -f $OUT/ils_record3/descents.csv ] || python analysis/triple32_descent.py $OUT/ils_record3 ils --start "[18 111a45bb 10 e04c12d1 16 a2869b67 15]" --iters 150 --kick-min 3 --kick-max 6 --seed 4
python analysis/triple32_descent.py $OUT/ils_restart3 ils --start "[17 a03ac133 11 ad92e95d 15 994cd969 14]" --iters 80 --kick-min 3 --kick-max 6 --seed 5
python analysis/triple32_descent.py $OUT/ils_sched3 ils --start "[15 b16b5153 13 3bcf8663 17 5506b66b 19]" --iters 60 --kick-min 3 --kick-max 6 --seed 6
