#!/usr/bin/env bash
# Round four: ILS from the record and from the 17/13/17/17 schedule lineage.
set -euo pipefail
cd "$(dirname "$0")/.."
OUT=results/triple32_memetic/descent
python analysis/triple32_descent.py $OUT/ils_record4 ils --start "[18 9d2ac69b 10 604c9af1 15 7b269b67 15]" --iters 150 --kick-min 3 --kick-max 6 --seed 7
python analysis/triple32_descent.py $OUT/ils_sched4 ils --start "[17 88bb3153 13 238e8679 17 4da6b66b 17]" --iters 150 --kick-min 3 --kick-max 6 --seed 8
