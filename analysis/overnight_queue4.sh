#!/bin/sh
# Queue 4 (2026-11-20): second escape round for the d303 family, after queue 3.
# Each job waits until no mixlab.exe has run for 60 s, so it yields the CPU to
# other sessions' jobs (e.g. race validation) between steps.
cd "$(dirname "$0")/../harness" || exit 1
R=../results/negation_races
Q=$R/queue.log
mkdir -p $R
say() { echo "[$(date '+%F %T')] $*" | tee -a $Q; }

running() { tasklist //FI "IMAGENAME eq mixlab.exe" 2>/dev/null | grep -q mixlab.exe; }
wait_idle() {
  idle=0
  while [ $idle -lt 60 ]; do
    if running; then idle=0; else idle=$((idle + 15)); fi
    sleep 15
  done
}
best_of() { grep '^best ' "$1" | sed 's/^best \(\[[^]]*\]\).*/\1/'; }

race() { # name seed start
  if [ -f "$R/$1.log" ] && grep -q '^best ' "$R/$1.log"; then say "skip $1 (done)"; return; fi
  wait_idle
  say "race $1 from $3"
  ./target/release/mixlab race --bits 32 --start "$3" --iters 25 --patience 6 --neighbors 96 \
    --seed "$2" --max-s2 16 --out "$R/$1.csv" > "$R/$1.log" 2>&1
  say "race $1 done: $(grep '^best ' $R/$1.log)"
}

sweep() { # name start  -> prints final spec on stdout (last line)
  spec=$2; i=0
  while [ $i -lt 5 ]; do
    i=$((i + 1))
    python -I ../analysis/gen_topbit_cands.py "$spec" "$R/$1_c$i.csv" > /dev/null
    wait_idle
    say "sweep $1 round $i from $spec"
    ./target/release/mixlab race --bits 32 --start "$spec" --candidates "$R/$1_c$i.csv" > "$R/$1_r$i.txt" 2>&1
    acc=$(grep '^accepted:' "$R/$1_r$i.txt")
    say "sweep $1 round $i: $acc"
    case "$acc" in
      "accepted: none"*) break ;;
      *) spec=$(echo "$acc" | sed 's/^accepted: \(\[[^]]*\]\).*/\1/') ;;
    esac
  done
  echo "$spec"
}

esc() { # name seed start: sweep, then race from the sweep result if it moved
  S=$(sweep "sw_$1" "$3" | tail -n 1)
  say "escape $1: $3 -> $S"
  if [ "$S" != "$3" ]; then race "esc_$1" "$2" "$S"; fi
}
until grep -q "queue3 done" $Q; do sleep 30; done
say "queue4 start"
esc d303b 53 "[16 d70332d3 15 379a369b 15]"
say "queue4 done"
