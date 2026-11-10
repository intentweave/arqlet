#!/bin/sh
set -eu
cd "$(dirname "$0")/../.."
input=${1:-exports/intentweave-initial/graph.json}
mkdir -p apps/playground/generated
cargo build -p arqlet-wasm --target wasm32-unknown-unknown --release --locked
cargo build -p arqlet-runtime --bins --locked
cp target/wasm32-unknown-unknown/release/arqlet_wasm.wasm apps/playground/generated/arqlet.wasm
# The npm package ships the engine beside its loader (git-ignored copy); the package tests load it from there in Node.
cp target/wasm32-unknown-unknown/release/arqlet_wasm.wasm packages/engine-client/arqlet.wasm
# Replace only this generated demo artifact, never caller input.
rm -f apps/playground/generated/snapshot.json
target/debug/snapshot "$input" apps/playground/generated/snapshot.json 40
# Browser-only mode imports this copy; the caller's input is only read.
cp "$input" apps/playground/generated/graph.json
# Presets for both UIs: versioned config sets with run policy. One index holds the full documents,
# in the order the selector shows them.
mkdir -p apps/playground/generated/presets
python3 -c "import json; order=['vorschau','raster','lesbar','qualitaet','auto']; docs={d['id']:d for d in (json.load(open(f'presets/{n}.json')) for n in order)}; json.dump([docs[n] for n in order], open('apps/playground/generated/presets/index.json','w'), indent=1)"
# Web-app examples: deterministic datasets from benchmarks/generate.py. Not written to the manifest.
python3 benchmarks/generate.py --demo-out apps/playground/generated/examples --no-manifest > /dev/null
# Curation editor: TypeScript compiled to generated/editor (git-ignored), typechecked and tested below.
pnpm --filter @arqlet/playground run build
node apps/playground/smoke.mjs

node --test apps/playground/revisions.test.mjs
python3 apps/playground/server_test.py

node apps/playground/ui-regression.mjs

# Scene adapters, SVG viewer and React Flow mapping: fixtures, golden markup and invariants.
node --test packages/viewer/*.test.mjs packages/engine-client/*.test.mjs packages/arqlet/*.test.mjs apps/playground/comparison.test.mjs apps/playground/aggregation.test.mjs apps/playground/import-map.test.mjs apps/playground/markup.test.mjs

node --test apps/playground/scene-engine.test.mjs

# The move gesture on the page: one request in flight, coalesced positions, ordered steps, refusals.
node --test apps/playground/drag.test.mjs

# Leaf resize from the bottom-right corner: the corner area, the pointer travel and the minimum size.
node --test apps/playground/resize.test.mjs

# The geometry the page reports: the outline per element and the numbering of its measurements.
node --test apps/playground/geometry.test.mjs

# Curation editor against the WASM engine: drafts, undo, validation, save and reopen.
pnpm --filter @arqlet/playground run test
