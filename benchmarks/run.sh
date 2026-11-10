#!/bin/sh
# Reproducible benchmark sweep. Builds the release bench binary, regenerates the datasets
# (deterministic), runs every configuration in SWEEP and appends one JSON line per run to the
# results file. A run that exceeds TIMEOUT seconds is recorded as timed out.
#
# Usage: sh benchmarks/run.sh [RESULTS.jsonl]
set -eu
cd "$(dirname "$0")/.."
out=${1:-benchmarks/results/$(date -u +%Y%m%dT%H%M%SZ).jsonl}
timeout_s=${TIMEOUT:-600}
if command -v timeout >/dev/null; then wrap="timeout $timeout_s"
elif command -v gtimeout >/dev/null; then wrap="gtimeout $timeout_s"
else wrap=""; echo "warning: no timeout command found; runs are not bounded" >&2; fi
cargo build --release -p arqlet-runtime --bin bench --features benchmark-timings --locked
python3 benchmarks/generate.py > /dev/null
mkdir -p benchmarks/results
# dataset  view-limit  repetitions  refinement-threads
SWEEP="
cloud-architecture 100 5 1,2,4
activity-order 100 5 1,2,4
deployment-cluster 100 5 1,2,4
state-order 100 5 1,2,4
state-tcp 100 5 1,2,4
workflow-dag 100 5 1,2,4
arqlet-pipeline 100 5 1,2,4
org-chart 100 5 1,2,4
uml-class 100 5 1,2,4
er-diagram 100 5 1,2,4
bpmn-pools 100 5 1,2,4
network-vlan 100 5 1,2,4
petri-net 100 5 1,2,4
layerweave-code 25 5 1,2,4
layerweave-code 50 5 1,2,4
layerweave-code 100 5 1,2,4
synthetic-2k 25 3 1,4
synthetic-2k 50 3 1,4
synthetic-2k 100 3 1,4
synthetic-20k 500 3 none
"
if [ -f exports/intentweave-initial/graph.json ]; then
  # Local, git-ignored export. Included only when present, so the sweep stays reproducible without it.
  mkdir -p benchmarks/datasets
  cp exports/intentweave-initial/graph.json benchmarks/datasets/intentweave-export.json
  SWEEP="$SWEEP
intentweave-export 100 3 1,4"
fi
printf '%s\n' "$SWEEP" | while read -r name limit reps threads; do
  [ -n "$name" ] || continue
  if ! $wrap target/release/bench --graph "benchmarks/datasets/$name.json" \
      --limit "$limit" --reps "$reps" --threads "$threads" --name "$name" >> "$out" 2>> "$out.log"; then
    printf '{"dataset":"%s","limit":%s,"status":"failed or timed out after %ss"}\n' "$name" "$limit" "$timeout_s" >> "$out"
  fi
done
echo "Results: $out"
