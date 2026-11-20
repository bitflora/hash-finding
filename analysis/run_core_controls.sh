#!/bin/sh
# F10 follow-up (2026-11-20): exact bias of core-control mixers. Resumable.
cd "$(dirname "$0")/../harness" || exit 1
R=../results/core_controls
mkdir -p $R
B=./target/release/mixlab.exe
[ -f $R/starts.csv ] || python ../analysis/gen_core_controls.py 16 2026 $R/starts.csv
O=$R/core_controls_exact.csv
[ -f $O ] || echo "spec,core,draw,bias" > $O
tail -n +2 $R/starts.csv | while IFS=, read spec core draw; do
  grep -qF "$spec" $O && continue
  b=$($B eval --bits 32 --spec "$spec" --exact 2>&1 | sed -n 's/.*bias=\([0-9.]*\).*/\1/p')
  echo "$spec,$core,$draw,$b" >> $O
done
echo "core controls done"
