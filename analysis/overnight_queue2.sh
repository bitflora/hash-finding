#!/bin/sh
# Overnight queue 2 (2026-11-20): does a negation + top-bit sweep escape the
# local optima where earlier family races stalled? Replaces queue 1 after its
# sweep from 0.10936 led back to the record.
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
say "queue2 start"
sweep swrecord "[16 21f0aaad 15 735a2d97 15]" > /dev/null
esc 7870 51 "[16 7870aaad 15 be523597 15]"
esc d303 52 "[16 d30332d3 15 c865c965 15]"
race negrec_both 42 "[16 de0f5553 15 8ca5d269 15]"
race neg7870_m1 43 "[16 7870aaad 15 41adca69 15]"
say "queue2 done"
