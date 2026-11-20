#!/bin/sh
# 2026-11-20 queue: wait for the refs rerun, then core controls, then seed search.
cd "$(dirname "$0")" || exit 1
N=$(grep -c "bias=" ../results/refs.txt)
until [ "$(grep -c "bias=" ../results/refs_corrected.txt)" -ge "$N" ]; do sleep 20; done
sleep 60   # let the last reference's extra metrics finish
sh ./run_core_controls.sh
sh ./run_seed_search.sh
echo "queue done"
