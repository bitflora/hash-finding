#!/bin/bash
# Radius-2 check + 2-step beam search around CENTRE (FINDINGS F23/F25 method).
# Usage: bash analysis/queue_local_search.sh "[16 m0 15 m1 15]" OUTDIR SEEDBASE ["SEEN_GLOBS"]
# SEEN_GLOBS: ';'-separated candidate CSVs already raced (skipped). Resumable.
cd "$(dirname "$0")/.."
C="$1"; D="$2"; S="$3"; SEEN="$4"; B=harness/target/release/mixlab.exe
mkdir -p $D; printf 'spec\n%s\n' "$C" > $D/centre.csv
log() { echo "[$(date '+%F %T')] $*" | tee -a results/ironborn_ideas/queue.log; }
race_chunks() {  # $1 = prefix, $2 = seed base
  for k in 0 1 2 3 4 5; do
    o=$D/$1_k$k.race.txt; [ -s $o ] && grep -q '^accepted' $o && continue
    $B race --bits 32 --start "$C" --candidates $D/$1_k$k.csv --seed $(($2+k)) > $o 2> $D/$1_k$k.race.err
    log "local $C $1 chunk $k: $(grep '^accepted' $o)"
  done
}
log "local search $C: radius 2"
if [ ! -f $D/r2_k0.csv ]; then
  python -I analysis/gen_radius2_cands.py "$C" $D/r2 6
  [ -n "$SEEN" ] && python -I analysis/filter_seen.py "$D/r2_k*.csv" "$SEEN" | tee -a results/ironborn_ideas/queue.log
fi
race_chunks r2 $S
log "local search $C: beam 1"
[ -f $D/b1_k0.csv ] || python -I analysis/beam_step.py $D/b1 6 66 "$D/r2_k*.race.txt" "$D/r2_k*.csv;$D/centre.csv${SEEN:+;$SEEN}" | tee -a results/ironborn_ideas/queue.log
race_chunks b1 $((S+100))
log "local search $C: beam 2"
[ -f $D/b2_k0.csv ] || python -I analysis/beam_step.py $D/b2 6 66 "$D/b1_k*.race.txt" "$D/r2_k*.csv;$D/b1_k*.csv;$D/centre.csv${SEEN:+;$SEEN}" | tee -a results/ironborn_ideas/queue.log
race_chunks b2 $((S+200))
log "local search $C: done"
