#!/bin/sh
# The cases of benchmarks/results/bus-collapse-20261126.md (#150) in which a move or a refinement draws differently with
# `skipTapReroute`: sh benchmarks/experiments/bus-collapse-drawing.sh BINARY OUT
set -e
# CONTAINERS UNITS BUSES GATEWAY VIEW
for case in "16 2 3 2 organic/lines" "24 3 3 2 organic/bar" "8 3 3 3 organic/bar" "24 3 3 3 organic/bar" \
  "16 5 4 2 organic/bar" "12 3 4 3 organic/lines" "12 5 4 3 layered/bar" "16 5 4 3 layered/bar"; do
  set -- "$1" "$2" $case
  node benchmarks/experiments/bus-collapse.mjs --binary "$1" --out "$2" --sizes "$3" --units "$4" --buses "$5" \
    --gateway "$6" --views "$7" --reps 1 --skip 1 --ops move,refine
done
