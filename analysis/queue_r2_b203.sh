#!/bin/bash
# 2026-11-23: radius-2 exhaustive check of the F23 best-bias mixer. Resumable.
cd "$(dirname "$0")/.."
D=results/ironborn_ideas/r2_b203; B=harness/target/release/mixlab.exe
log() { echo "[$(date '+%F %T')] $*" | tee -a results/ironborn_ideas/queue.log; }
for k in 0 1 2 3 4 5; do
  o=$D/r2_k$k.race.txt; [ -s $o ] && grep -q '^accepted' $o && continue
  log "r2 b20332d3 chunk $k"; $B race --bits 32 --start "[16 b20332d3 15 d21a3597 15]" --candidates $D/r2_k$k.csv --seed $((400+k)) > $o 2> $D/r2_k$k.race.err
  log "r2 b20332d3 chunk $k: $(grep '^accepted' $o)"
done
log "r2 b20332d3 done"
