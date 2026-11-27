#!/bin/sh
# Fingerprints of #229: the views of the stored fingerprint files (CLAUDE.md, Layout options), each with the option that
# file was recorded with, on two builds of `bench` (release, `benchmark-timings`), threads 1 and 4. One results file per
# build and configuration: OUTDIR/<config>-<build>.jsonl. A view whose file already has its row is skipped.
#
# Usage: sh benchmarks/experiments/overlap-index-fingerprints.sh BASE_BENCH NEW_BENCH OUTDIR
set -eu
base=$1
new=$2
outdir=$3
mkdir -p "$outdir"
if command -v timeout >/dev/null; then wrap="timeout 900"
elif command -v gtimeout >/dev/null; then wrap="gtimeout 900"
else wrap=""; fi
VIEWS="cloud-architecture/100 activity-order/100 deployment-cluster/100 state-order/100 state-tcp/100 workflow-dag/100
arqlet-pipeline/100 org-chart/100 uml-class/100 er-diagram/100 bpmn-pools/100 network-vlan/100 petri-net/100
layerweave-code/100 synthetic-2k/100"
# The older files also hold layerweave-code and synthetic-2k at 25 and 50.
MORE="layerweave-code/25 layerweave-code/50 synthetic-2k/25 synthetic-2k/50"
run() { # build binary config options view
  name=${5%/*}
  limit=${5#*/}
  file="$outdir/$3-$1.jsonl"
  if [ -f "$file" ] && grep -q "\"dataset\":\"$name\",.*\"limit\":$limit[,}]" "$file"; then return; fi
  # The default runs without `--options`: an empty object also lands in the snapshot and changes its fingerprint.
  for attempt in 1 2; do
    if [ -n "$4" ]; then
      $wrap "$2" --graph "benchmarks/datasets/$name.json" --limit "$limit" --reps 1 --threads 1,4 --name "$name" \
        --options "$4" >> "$file" 2>> "$outdir/$3-$1.log" && return
    else
      $wrap "$2" --graph "benchmarks/datasets/$name.json" --limit "$limit" --reps 1 --threads 1,4 --name "$name" \
        >> "$file" 2>> "$outdir/$3-$1.log" && return
    fi
  done
  printf '{"dataset":"%s","limit":%s,"status":"failed or timed out twice"}\n' "$name" "$limit" >> "$file"
}
for config in default loop-space-off shared-border-off channels-by-demand-off single-pass-auto-off; do
  case $config in
    default) options=''; views=$VIEWS ;;
    loop-space-off) options='{"loopSpace":false}'; views="$VIEWS $MORE" ;;
    shared-border-off) options='{"sharedBorderRanks":false,"alignStacked":false}'; views="$VIEWS $MORE" ;;
    channels-by-demand-off) options='{"channelsByDemand":false}'; views="$VIEWS $MORE" ;;
    single-pass-auto-off) options='{"singlePassAuto":false,"singlePass":false,"abstractRouting":false}'; views="$VIEWS $MORE" ;;
  esac
  for view in $views; do
    run base "$base" "$config" "$options" "$view"
    run new "$new" "$config" "$options" "$view"
  done
done
