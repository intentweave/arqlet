#!/bin/sh
# The timings of benchmarks/results/bus-collapse-20261126.md (#150), two builds in turn case by case:
# sh benchmarks/experiments/bus-collapse.sh OUT MAIN BRANCH [more options]
set -e
OUT=$1 MAIN=$2 BRANCH=$3
shift 3
run() {
  node benchmarks/experiments/bus-collapse.mjs --binary "$MAIN" --out "$OUT" --label main "$@"
  node benchmarks/experiments/bus-collapse.mjs --binary "$BRANCH" --out "$OUT" --label branch "$@"
}
# CONTAINERS UNITS BUSES GATEWAY: views in which the collapsed organic lines view was several times slower than the open one;
# the same views with bars.
for case in "40 5 3 2" "48 3 3 2" "48 5 4 2" "64 3 3 3" "64 3 4 2" "64 3 5 3"; do
  set -- $case "$@"
  c=$1 u=$2 b=$3 g=$4
  shift 4
  run --sizes "$c" --units "$u" --buses "$b" --gateway "$g" --views organic/lines,organic/bar "$@"
done
# The script's defaults (6 units, every 8th area on two buses) in every view style.
run --sizes 20,40,80 "$@"
