#!/bin/sh
# Queue 3 (2026-11-20): F10 low-bit-core test. Waits for queue 2 to finish.
cd "$(dirname "$0")/../harness" || exit 1
R=../results/negation_races
Q=$R/queue.log
# Originally a side build (CARGO_TARGET_DIR=../../target-lowbits) so the binary
# with --fix-low-bits could be built while queue 2 held target/. The current
# harness includes --fix-low-bits, so the normal build works.
B=target/release/mixlab.exe
say() { echo "[$(date '+%F %T')] $*" | tee -a $Q; }
until grep -q "queue2 done" $Q; do sleep 30; done
say "queue3 start"
O=$R/core_exact.csv
[ -f $O ] || echo "spec,group,bias" > $O
tail -n +2 $R/core_starts.csv | while IFS=, read spec group; do
  grep -qF "$spec" $O && continue
  b=$($B eval --bits 32 --spec "$spec" --exact 2>&1 | sed -n 's/.*bias=\([0-9.]*\).*/\1/p')
  echo "$spec,$group,$b" >> $O
done
say "core exact evals done"
# race from the best core start, low 16 bits fixed vs unrestricted, same seed
S=$(grep ',core,' $O | sort -t, -k3 -g | head -n 1 | cut -d, -f1)
say "core race start: $S"
$B race --bits 32 --start "$S" --iters 15 --patience 6 --neighbors 96 --seed 61 --max-s2 16 --fix-low-bits 16 \
  --out $R/core_fixed.csv > $R/core_fixed.log 2>&1
say "core_fixed done: $(grep '^best ' $R/core_fixed.log)"
$B race --bits 32 --start "$S" --iters 15 --patience 6 --neighbors 96 --seed 61 --max-s2 16 \
  --out $R/core_free.csv > $R/core_free.log 2>&1
say "core_free done: $(grep '^best ' $R/core_free.log)"
say "queue3 done"
