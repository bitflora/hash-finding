#!/bin/sh
# 2026-11-20 queue: (a) BIC-constrained races (F18), (b) races from the best
# random-high-half draw of four non-record cores (F19). Resumable per race.
cd "$(dirname "$0")/../harness" || exit 1
B=./target/release/mixlab.exe
Q=../results/queue_bic_cores.log
say() { echo "[$(date '+%F %T')] $*" | tee -a $Q; }
run() {  # name outdir start extra-args...
  name=$1; dir=$2; start=$3; shift 3
  mkdir -p $dir
  if grep -q "^best " $dir/$name.log 2>/dev/null; then say "skip $name"; return; fi
  say "start $name from $start $*"
  $B race --bits 32 --start "$start" --iters 25 --patience 6 --neighbors 96 --seed 61 "$@" \
    --out $dir/$name.csv > $dir/$name.log 2>&1
  say "done $name: $(grep '^best ' $dir/$name.log)"
}
F=../results/bic_frontier
run bic005_s12 $F "[16 7870aaad 15 be523597 12]" --max-bic-corr 0.05
run bic015_s14 $F "[16 7870aaad 15 be523597 14]" --max-bic-corr 0.15 --max-s2 15
C=../results/core_races
run core_1908 $C "[17 83065565 16 a5ae6b47 15]" --max-s2 16
run core_d303 $C "[16 c88b32d3 15 9293c965 15]" --max-s2 16
run core_450c $C "[16 3d6c2d35 15 00a649a9 15]" --max-s2 16
run core_lowbias32 $C "[16 c4a3352d 15 7daba68b 15]" --max-s2 16
say "queue done"
