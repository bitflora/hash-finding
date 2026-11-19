#!/bin/sh
# Long 32-bit racing runs from distinct published families.
# Resumable: finished runs (log has a "best" line) are skipped; a partial run
# restarts from the last incumbent in its CSV, writing NAME_r<k>.{csv,log}.
cd "$(dirname "$0")/../harness" || exit 1
R=../results/family_races
mkdir -p $R
ITERS=25; PATIENCE=6
run() { # name seed spec
  name=$1; seed=$2; spec=$3
  k=0; prev=""
  while [ -f "$R/$name${prev}.log" ]; do
    if grep -q '^best ' "$R/$name${prev}.log"; then return; fi
    last=$(grep -E '^[0-9]' "$R/$name${prev}.csv" | tail -n 1 | cut -d, -f3)
    [ -n "$last" ] && spec=$last
    k=$((k+1)); prev="_r$k"
  done
  ./target/release/mixlab race --bits 32 --start "$spec" --iters $ITERS --patience $PATIENCE \
    --neighbors 96 --seed $((seed + 100*k)) --out "$R/$name${prev}.csv" > "$R/$name${prev}.log" 2>&1
}
run fam450c    21 "[16 450c2d35 15 caf649a9 16]"
run famd303    22 "[16 d30332d3 15 c865c965 15]"
run famlowbias 23 "[16 93f3352d 15 042cb68b 16]"
run famd168    24 "[15 d168aaad 15 af723597 15]"
run fam9e48    25 "[17 9e485565 16 ef1d6b47 16]"
echo ALL DONE
