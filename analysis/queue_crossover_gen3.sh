#!/bin/bash
# 2026-11-22: crossover gen 3 - whole/half/uniform children, 3 race seeds each
# (FINDINGS §5 variations 1 and 6). Resumable.
cd "$(dirname "$0")/.."
D=results/ironborn_ideas
log() { echo "[$(date '+%F %T')] $*" | tee -a $D/queue.log; }
log "gen3 (whole/half/uniform, RACE=2 x SEEDS=3)"
python -I analysis/crossover_gen3.py $D/gen3 $D/gen3_parents.csv \
  $D/children_p26.csv $D/gen1/screen_p26.csv $D/gen2/screen_p26.csv >> $D/gen3.log 2>&1
log "gen3 exit $?"
