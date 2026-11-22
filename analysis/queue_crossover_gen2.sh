#!/bin/bash
# 2026-11-21: race 7 more gen-1 children (RACE=10), then crossover generation 2
# with every gen-1 race winner added as a parent. Resumable.
cd "$(dirname "$0")/.."
D=results/ironborn_ideas
log() { echo "[$(date '+%F %T')] $*" | tee -a $D/queue.log; }
log "gen1 extra races (RACE=10)"
RACE=10 SEED=81 python -I analysis/crossover_round.py $D/gen1 $D/gen1_extra.csv $D/children_p26.csv >> $D/gen1.log 2>&1
cp $D/gen2_extra.csv $D/gen2_extra_full.csv
for f in $D/gen1/race_*.csv.done; do
  b=$(grep '^best' $f | sed -E 's/^best (\[[^]]*\]) +score ([0-9.]+).*/\1,\2/')
  echo "$b,gen1 race $(basename $f .csv.done)" >> $D/gen2_extra_full.csv
done
log "gen2 (RACE=5)"
RACE=5 SEED=91 python -I analysis/crossover_round.py $D/gen2 $D/gen2_extra_full.csv $D/children_p26.csv $D/gen1/screen_p26.csv > $D/gen2.log 2>&1
log "done"
