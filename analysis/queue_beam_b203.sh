#!/bin/bash
# 2026-11-23: two-step beam search beyond the radius-2 check of the F23 mixer.
# Step 1: radius-1 neighbours of the 66 exactly-scored radius-2 neighbours.
# Step 2: radius-1 neighbours of the 66 best exactly-scored step-1 candidates.
# All raced against the centre with early acceptance. Resumable.
cd "$(dirname "$0")/.."
C="[16 b20332d3 15 d21a3597 15]"
D=results/ironborn_ideas/beam_b203; R=results/ironborn_ideas/r2_b203; B=harness/target/release/mixlab.exe
log() { echo "[$(date '+%F %T')] $*" | tee -a results/ironborn_ideas/queue.log; }
race_chunks() {  # $1 = step prefix, $2 = seed base
  for k in 0 1 2 3 4 5; do
    o=$D/$1_k$k.race.txt; [ -s $o ] && grep -q '^accepted' $o && continue
    $B race --bits 32 --start "$C" --candidates $D/$1_k$k.csv --seed $(($2+k)) > $o 2> $D/$1_k$k.race.err
    log "beam $1 chunk $k: $(grep '^accepted' $o)"
  done
}
log "beam step 1"
[ -f $D/b1_k0.csv ] || python -I analysis/beam_step.py $D/b1 6 66 "$R/r2_k*.race.txt" "$R/r2_k*.csv;$D/centre.csv"
race_chunks b1 500
log "beam step 2"
[ -f $D/b2_k0.csv ] || python -I analysis/beam_step.py $D/b2 6 66 "$D/b1_k*.race.txt" "$R/r2_k*.csv;$D/centre.csv;$D/b1_k*.csv" | tee -a results/ironborn_ideas/queue.log
race_chunks b2 600
log "beam done"
