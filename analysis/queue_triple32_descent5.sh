#!/usr/bin/env bash
# Round five: exhaustive two-step neighbourhoods of both round-four leaders,
# then ILS from each.
set -euo pipefail
cd "$(dirname "$0")/.."
OUT=results/triple32_memetic/descent
A="[17 88bb3153 13 238e8679 17 7da6b66b 17]"
B="[18 1daac69b 10 404c9af9 15 f122bb27 15]"
python analysis/triple32_descent.py $OUT/radius2_sched5 radius2 --start "$A" --top 5 --seed 9
python analysis/triple32_descent.py $OUT/radius2_record5 radius2 --start "$B" --top 5 --seed 10
# start each ILS from the best that its radius-two run reached
best() { python -c "import csv,sys; r=min(csv.DictReader(open(sys.argv[1])), key=lambda r: float(r['end_bias'])); print(r['end'] if float(r['end_bias']) < float(sys.argv[3]) else sys.argv[2])" "$1" "$2" "$3"; }
A=$(best $OUT/radius2_sched5/descents.csv "$A" 0.019134091125396723)
B=$(best $OUT/radius2_record5/descents.csv "$B" 0.019489272521653327)
echo "ILS starts: $A / $B"
python analysis/triple32_descent.py $OUT/ils_sched5 ils --start "$A" --iters 150 --kick-min 3 --kick-max 6 --seed 11
python analysis/triple32_descent.py $OUT/ils_record5 ils --start "$B" --iters 150 --kick-min 3 --kick-max 6 --seed 12
