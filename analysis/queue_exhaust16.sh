#!/bin/sh
# Exhaust 2-round 16-bit shift triples listed one per line (S0/S1/S2) in $1,
# keeping every mixer with exact bias <= 8. Skips triples already done.
cd "$(dirname "$0")/.." || exit 1
BIN=harness/target/release/mixlab
[ -x "$BIN.exe" ] && BIN="$BIN.exe"
tr -d '\r' < "$1" | while read -r t; do
  [ -z "$t" ] && continue
  name=s$(echo "$t" | tr / _)
  [ -f "results/exhaust16/$name.csv" ] && continue
  "$BIN" exhaust --bits 16 --shifts "$t" --max-score 8 --out "results/exhaust16/$name.csv.tmp" 2> "results/exhaust16/$name.log" \
    && mv "results/exhaust16/$name.csv.tmp" "results/exhaust16/$name.csv"
  tail -n 1 "results/exhaust16/$name.log"
done
