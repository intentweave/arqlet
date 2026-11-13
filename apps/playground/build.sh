#!/bin/sh
set -eu
cd "$(dirname "$0")/../.."
input=${1:-exports/intentweave-initial/graph.json}
mkdir -p apps/playground/generated
cargo build -p arqlet-wasm --target wasm32-unknown-unknown --release --locked
# The native binaries the backend and the smoke test run: optimised like release, without LTO (`fast` profile), so the
# backend answers about as fast as a release build and the build stays quick.
cargo build -p arqlet-runtime --bins --locked --profile fast
cp target/wasm32-unknown-unknown/release/arqlet_wasm.wasm apps/playground/generated/arqlet.wasm
# The npm package ships the engine beside its loader (git-ignored copy); the package tests load it from there in Node.
cp target/wasm32-unknown-unknown/release/arqlet_wasm.wasm packages/engine-client/arqlet.wasm
# Replace only this generated demo artifact, never caller input.
rm -f apps/playground/generated/snapshot.json
target/fast/snapshot "$input" apps/playground/generated/snapshot.json 40
# Browser-only mode imports this copy; the caller's input is only read.
cp "$input" apps/playground/generated/graph.json
# Presets for both UIs: versioned config sets with run policy. One index holds the full documents,
# in the order the selector shows them.
mkdir -p apps/playground/generated/presets
python3 -c "import json; order=['vorschau','raster','lesbar','qualitaet','auto']; docs={d['id']:d for d in (json.load(open(f'presets/{n}.json')) for n in order)}; json.dump([docs[n] for n in order], open('apps/playground/generated/presets/index.json','w'), indent=1)"
# Web-app examples: deterministic datasets from benchmarks/generate.py. Not written to the manifest.
python3 benchmarks/generate.py --demo-out apps/playground/generated/examples --no-manifest > /dev/null
# A flat call graph (no containers): the case for communities (option `communities`).
python3 benchmarks/overview/generate.py --nodes 2000 --seed 1 --flat --out apps/playground/generated/examples/calls-flat.json > /dev/null
python3 - <<'PY'
import json, sys
sys.path.insert(0, 'benchmarks/overview')
import generate
# Domain groups for the flat graph: each symbol's directories and file in the nested graph it was flattened from (the
# same generator and seed), so that given groups can be compared with the communities Louvain finds.
nested = generate.generate(2000, 1)
parent = {n['id']: n.get('parentId') for n in nested['nodes']}
label = {n['id']: n['label'] for n in nested['nodes']}
groups = {}
for n in nested['nodes']:
    if n['kind'] != 'symbol':
        continue
    path, at = [], parent[n['id']]
    while at:
        path.append(label[at])
        at = parent[at]
    groups[n['id']] = path[::-1]
flat_path = 'apps/playground/generated/examples/calls-flat.json'
flat = json.load(open(flat_path))
flat['groups'] = {k: v for k, v in groups.items() if k in {n['id'] for n in flat['nodes']}}
json.dump(flat, open(flat_path, 'w'), separators=(',', ':'))
path = 'apps/playground/generated/examples/index.json'
index = [e for e in json.load(open(path)) if e['name'] != 'calls-flat']
index.append({'name': 'calls-flat', 'title': 'Flat call graph', 'nodes': len(flat['nodes']),
              'description': 'A call graph without containers. Try Communities above 40 (found by Louvain), or Domain groups (the directories and files it came from).',
              'source': 'benchmarks/overview/generate.py --nodes 2000 --seed 1 --flat'})
json.dump(index, open(path, 'w'), indent=1, ensure_ascii=False)
PY
# Cluster overview (overview.html): deterministic hierarchical graphs of two sizes, seed 1.
mkdir -p apps/playground/generated/overview
python3 benchmarks/overview/generate.py --nodes 10000 --seed 1 --out apps/playground/generated/overview/code-10000.json
python3 benchmarks/overview/generate.py --nodes 50000 --seed 1 --out apps/playground/generated/overview/code-50000.json
python3 benchmarks/overview/generate.py --nodes 6000 --seed 1 --flat --out apps/playground/generated/overview/calls-flat.json
# The data's config set: rules for the overview by node kind (ADR 0018, addendum).
cp benchmarks/overview/code-index.configset.json apps/playground/generated/overview/code-index.configset.json
printf '[{"title":"Code index, synthetic","file":"code-10000.json","nodes":10000,"configSet":"code-index.configset.json"},{"title":"Code index, synthetic","file":"code-50000.json","nodes":50000,"configSet":"code-index.configset.json"},{"title":"Flat call graph, communities","file":"calls-flat.json","nodes":5612}]\n' > apps/playground/generated/overview/index.json
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
