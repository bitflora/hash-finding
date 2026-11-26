#!/bin/bash
# Iterated radius-3 hill climb (F45): race every radius-3 neighbour of the
# centre, exact-score the accepted and near-miss neighbours, move to the
# best if it beats the centre, repeat. Usage: climb.sh "[spec]" OUTDIR MAXSTEPS
cd "$(dirname "$0")/../.."
C="$1"; D="$2"; N="${3:-6}"; B=harness/target/release/mixlab.exe; mkdir -p $D
best=$($B exact --bits 32 --spec "$C" 2>/dev/null | grep -o "bias[= ]*[0-9.]*" | grep -o "[0-9.]*$")
printf 'spec\n%s\n' "$C" > $D/c.txt; $B exact --bits 32 --list $D/c.txt --out $D/c.csv >/dev/null; best=$(tail -1 $D/c.csv | cut -d, -f2)
echo "[$(date '+%F %T')] start $C $best" | tee -a $D/climb.log
for step in $(seq 1 $N); do
  P=$D/step$step; py -I analysis/gen_radius2_cands.py "$C" $P 4 3 > /dev/null
  for k in 0 1 2 3; do $B race --bits 32 --start "$C" --candidates ${P}_k$k.csv --seed $((step*10+k)) > ${P}_k$k.race.txt 2>/dev/null; done
  { grep -h -E "Rejected|Accepted|Better" ${P}_k*.race.txt | awk '{print $1,$2,$3,$4,$5}'; grep -h "^accepted: \[" ${P}_k*.race.txt | sed 's/accepted: //' | awk '{print $1,$2,$3,$4,$5}'; } | sort -u > ${P}_near.txt
  $B exact --bits 32 --list ${P}_near.txt --out ${P}_exact.csv > /dev/null
  top=$(tail -n +2 ${P}_exact.csv | sort -t, -k2 -g | head -1)
  s=$(echo "$top" | cut -d, -f1); b=$(echo "$top" | cut -d, -f2)
  echo "[$(date '+%F %T')] step $step: best neighbour $s $b ($(($(wc -l < ${P}_near.txt))) exact)" | tee -a $D/climb.log
  if py -c "import sys; sys.exit(0 if float('$b') < float('$best') else 1)"; then C="$s"; best="$b"; else break; fi
done
echo "[$(date '+%F %T')] end $C $best" | tee -a $D/climb.log
