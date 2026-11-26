#!/bin/bash
# Radius-3 check of the current best (F30): race every candidate in r3_k*.csv
# against the centre (mixlab race --candidates; same protocol as F20/F25 radius-2).
cd "$(dirname "$0")/../.."
C="[16 4b0332d3 15 549a3597 15]"; D=results/radius3_4b03; B=harness/target/release/mixlab.exe
for k in $(seq 0 11); do
  o=$D/r3_k$k.race.txt; [ -s $o ] && grep -q '^accepted' $o && continue
  $B race --bits 32 --start "$C" --candidates $D/r3_k$k.csv --seed $((900+k)) > $o 2> $D/r3_k$k.race.err
  echo "[$(date '+%F %T')] chunk $k: $(grep '^accepted' $o)" | tee -a $D/run.log
done
echo "[$(date '+%F %T')] done" | tee -a $D/run.log
