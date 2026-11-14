#!/bin/sh
# ADR 0020, S1-4: time of the default against the candidates (B: regionPorts + straightDetours + singlePassReversed, and
# directionsFirst) with the release bench binary: import (selection and layout) and refinement, `reps` runs each, one
# refinement thread. Run on an idle machine; the bench rows record the machine, revision and load.
# Usage: sh benchmarks/eval/s1-4-timing.sh [RESULTS.jsonl]
set -eu
cd "$(dirname "$0")/../.."
out=${1:-benchmarks/results/s1-4-timing-$(date -u +%Y%m%dT%H%M%SZ).jsonl}
cargo build --release -p arqlet-runtime --bin bench --features benchmark-timings --locked
B='{"regionPorts":true,"straightDetours":true,"singlePassReversed":true}'
D='{"directionsFirst":true}'
# dataset  view-limit  repetitions
SWEEP="
cloud-architecture 100 5
activity-order 100 5
deployment-cluster 100 5
state-order 100 5
state-tcp 100 5
workflow-dag 100 5
arqlet-pipeline 100 5
org-chart 100 5
uml-class 100 5
er-diagram 100 5
bpmn-pools 100 5
network-vlan 100 5
petri-net 100 5
can-bus 100 5
mixed-layouts 100 5
self-loops 100 5
petri-net-shapes 100 5
nested-tree-radial 150 5
mixed-large 150 3
layerweave-code 100 3
synthetic-2k 100 3
"
printf '%s\n' "$SWEEP" | while read -r name limit reps; do
  [ -n "$name" ] || continue
  for variant in default B directionsFirst; do
    case $variant in
      default) opts='{}' ;;
      B) opts=$B ;;
      directionsFirst) opts=$D ;;
    esac
    if ! target/release/bench --graph "benchmarks/datasets/$name.json" --limit "$limit" --reps "$reps" --threads 1 \
        --name "$name|$variant" --import-options "$opts" >> "$out" 2>> "$out.log"; then
      printf '{"dataset":"%s|%s","limit":%s,"status":"failed"}\n' "$name" "$variant" "$limit" >> "$out"
    fi
  done
done
echo "Results: $out"
