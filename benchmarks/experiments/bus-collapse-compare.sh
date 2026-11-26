#!/bin/sh
# The comparison grid of benchmarks/results/bus-collapse-20261126.md (#150):
# sh benchmarks/experiments/bus-collapse-compare.sh OLD NEW OUT
set -e
run() { node benchmarks/experiments/bus-collapse-compare.mjs --old "$OLD" --new "$NEW" --out "$OUT" "$@"; }
OLD=$1 NEW=$2 OUT=$3
list() { echo "$@" | tr ' ' ','; }
# Every view style, small and middle sizes: SIZE:UNITS:BUSES:GATEWAY.
run --cases "$(list 8:3:3:2 8:2:4:3 16:3:3:2 16:2:3:2 16:5:4:3 24:4:3:2 24:3:4:3)"
# Larger organic lines views, in which the second routing is left out most often (no refinement: it takes minutes).
run --views organic/lines,organic/trunk --cases "$(list 32:4:3:2 32:5:4:2 36:3:3:2)"
# The six slow cases of the results file, collapsed (a step takes seconds there); two of them also as a session.
run --views organic/lines --variants collapsed --modes stateless --cases "$(list 40:5:3:2 48:3:3:2 48:5:4:2 64:3:3:3 64:3:4:2 64:3:5:3)"
run --views organic/lines --variants collapsed --modes session --cases "$(list 40:5:3:2 48:3:3:2)"
# What keeps the second routing in place, and what a view may be configured with.
small=24:4:3:2
organic() { run --views organic/lines --cases "$small" --refine 0 --extra "$1"; }
organic '{"edgeLabels":{"t0-c1u0":{"text":"tap","width":20,"height":10},"t1-c1u0":{"text":"tap","width":20,"height":10},"t2-c1u0":{"text":"tap","width":20,"height":10}}}'
organic '{"minPortSpacing":40}'
organic '{"singlePass":false}'
organic '{"hierarchicalOrdering":false}'
organic '{"directionsFirst":true}'
organic '{"organicForces":false}'
organic '{"bundling":"complete"}'
organic '{"routingEngine":"channel"}'
# Straight routing, collapsed and at 8 areas: with more nodes in view single steps take over 10 minutes in either build.
run --views organic/lines --cases 8:3:3:2 --variants collapsed --refine 0 --extra '{"graph":{"defaultRouting":"straight"}}'
organic '{"graph":{"defaultRouting":"bus"}}'
organic '{"graph":{"edges":[{"id":"x1","kind":"calls","source":"c1u0","target":"c2u0"},{"id":"x2","kind":"calls","source":"c3u0","target":"c4u0"}]}}'
# A layered top level with organic areas inside.
inner="$(node -e 'console.log(JSON.stringify({graph: {defaultDirection: "lr", containerStrategies: Object.fromEntries(Array.from({length: 24}, (_, i) => [`c${i}`, "organic"]))}}))')"
run --views layered/lines --cases "$small" --refine 0 --extra "$inner"
