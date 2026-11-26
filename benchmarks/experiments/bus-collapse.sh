#!/bin/sh
# The cases of benchmarks/results/bus-collapse-20261126.md (#150): sh benchmarks/experiments/bus-collapse.sh BINARY OUT LABEL [more options]
set -e
run() { node benchmarks/experiments/bus-collapse.mjs --binary "$BINARY" --out "$OUT" --label "$LABEL" "$@"; }
BINARY=$1 OUT=$2 LABEL=$3
shift 3
# CONTAINERS UNITS BUSES GATEWAY: organic views in which the collapsed view was several times slower than the open one.
for case in "40 5 3 2" "48 3 3 2" "48 5 4 2" "64 3 3 3" "64 3 4 2" "64 3 5 3"; do
  set -- $case "$@"
  c=$1 u=$2 b=$3 g=$4
  shift 4
  run --sizes "$c" --units "$u" --buses "$b" --gateway "$g" --views organic/lines "$@"
done
# The script's defaults (6 units, every 8th area on two buses) in every view style, where the collapsed view was faster.
run --sizes 20,40,80 "$@"
