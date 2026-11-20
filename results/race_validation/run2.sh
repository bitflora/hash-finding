#!/bin/sh
# Resume 32-bit validation after the negation race (PID given as $1) exits.
# Run from mixing-functions/harness.
set -e
while tasklist //FI "PID eq $1" | grep -q " $1 "; do sleep 10; done
echo "other race finished at $(date)"
O=../results/race_validation
B=../../target-trace/release/mixlab.exe
$B trace --bits 32 --list $O/issue19_exact.csv --bases 3 --reps 4 --n0 p14 --cap p28 --seed 3 --resume 0:4 --out $O/trace32_issue19.csv 2>> $O/trace32_issue19.log
$B trace --bits 32 --start "[16 de0f5553 15 735a2d97 15]" --path 0 --neighbours 64 --max-s2 15 --reps 4 --n0 p14 --cap p28 --seed 4 --out $O/trace32_negrec.csv 2> $O/trace32_negrec.log
echo "validation finished at $(date)"
