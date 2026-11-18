#!/bin/sh
# ADR 0019, step 4: scene as JSON objects against columns. Waits (at most WAIT seconds) for a quiet machine, then writes
# benchmarks/results/columns-<UTC>.jsonl: a machine line, then the lines of measure-columns.mjs per size and view.
#
#   cargo build -p arqlet-wasm --target wasm32-unknown-unknown --release --locked
#   sh benchmarks/protocol/measure-columns.sh [reps]
set -eu
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
REPS=${1:-5}
MAXLOAD=${MAXLOAD:-3}
WAIT=${WAIT:-1200}
WASM="$ROOT/target/wasm32-unknown-unknown/release/arqlet_wasm.wasm"
[ -f "$WASM" ] || { echo "Missing $WASM" >&2; exit 1; }
load() { uptime | sed 's/.*load averages*: *//; s/[ ,].*//; s/,/./'; }
waited=0
while [ "$waited" -lt "$WAIT" ] && [ "$(load | awk -v m="$MAXLOAD" '{print ($1 > m)}')" = 1 ]; do sleep 10; waited=$((waited + 10)); done
STAMP=$(date -u +%Y%m%dT%H%M%SZ)
OUT="$ROOT/benchmarks/results/columns-$STAMP.jsonl"
DIRTY=false
[ -n "$(git -C "$ROOT" status --porcelain --untracked-files=no)" ] && DIRTY=true
printf '{"machine":"%s","os":"%s","cores":%s,"revision":"%s","dirty":%s,"rustc":"%s","node":"%s","load":%s,"utc":"%s"}\n' \
  "$(sysctl -n machdep.cpu.brand_string)" "$(uname -sr)" "$(sysctl -n hw.ncpu)" "$(git -C "$ROOT" rev-parse --short HEAD)" "$DIRTY" \
  "$(rustc --version)" "$(node --version)" "$(load)" "$(date -u +%Y-%m-%dT%H:%M:%SZ)" > "$OUT"
for n in 10000 100000; do
  for view in lod full; do
    node "$ROOT/benchmarks/protocol/measure-columns.mjs" "$WASM" "$ROOT/benchmarks/datasets/protocol-$n.json" "$REPS" "$view" >> "$OUT"
  done
done
printf '{"loadAtEnd":%s}\n' "$(load)" >> "$OUT"
echo "$OUT"
