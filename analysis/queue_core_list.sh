#!/bin/bash
# 2026-11-22: after gen 3, exact-score the 39 hand-picked core pairings in
# results/ironborn_ideas/core_list.csv and race the best 5 (2 seeds each). Resumable.
cd "$(dirname "$0")/.."
D=results/ironborn_ideas
log() { echo "[$(date '+%F %T')] $*" | tee -a $D/queue.log; }
# wait until the latest gen3 entry in queue.log is its exit line
until grep 'gen3' $D/queue.log | grep -v paused | tail -1 | grep -q 'gen3 exit'; do sleep 60; done
log "core list (39 specs, RACE=5 x SEEDS=2)"
python -I analysis/race_list.py $D/core_list $D/core_list.csv >> $D/core_list.log 2>&1
log "core list exit $?"
