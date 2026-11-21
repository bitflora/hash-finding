#!/bin/bash
# 2026-11-21: TheIronBorn-inspired checks (FINDINGS F20). Resumable: finished steps are skipped.
cd "$(dirname "$0")/.."
D=results/ironborn_ideas; B=harness/target/release/mixlab.exe
log() { echo "[$(date '+%F %T')] $*" | tee -a $D/queue.log; }
[ -f $D/STOP ] && exit 0
log "crossover screen"; python -I analysis/screen_list.py $D/children.csv $D/children_p26.csv 26 7
for k in 0 1 2 3 4 5; do
  [ -f $D/STOP ] && { log "STOP"; exit 0; }
  o=$D/r2_k$k.race.txt; [ -s $o ] && grep -q '^accepted' $o && continue
  log "radius-2 chunk $k"; $B race --bits 32 --start "[16 21f0aaad 15 735a2d97 15]" --candidates $D/r2_k$k.csv --seed $((300+k)) > $o 2> $D/r2_k$k.race.err
  log "chunk $k: $(grep '^accepted' $o)"
done
log "queue done"
