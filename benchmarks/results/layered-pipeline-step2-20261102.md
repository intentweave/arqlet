# Consistent layered pipeline, steps 1 and 2 together, 2026-11-02

Machine: Apple M3, 8 logical CPUs, macos, release, `bench --reps 1 --threads 1`, limit 100
(er-diagram and tree-100: 1000). Revision 37bdb10 plus uncommitted changes. layerweave-code is the graph of 0bcfd56
(a7636dbb). The machine was not idle; no times are reported. Raw records: `layered-pipeline-step2-20261102.jsonl` (first 16
lines `{"layeredConstruction":true}`, last 16 with `"hierarchicalOrdering":true` as well).

Invariants: default output unchanged (16 of 16 refinement fingerprints); with `layeredConstruction` alone the geometry is
that of `layered-tracks-20261101.jsonl` (16 of 16) and the refinement fingerprints equal those of 37bdb10 (checked on three
datasets).

Each cell: `layeredConstruction` / with `hierarchicalOrdering`, after refinement.

| dataset | hard | crossings | bends | length | refinement kept |
|---|---|---|---|---|---|
| petri-net | 0 / 0 | 62 / 56 | 92 / 100 | 15k / 14k | routing-only / forward-barycenter |
| er-diagram | 0 / 0 | 0 / 0 | 29 / 29 | 3k / 3k | routing-only / routing-only |
| state-order | 0 / 0 | 0 / 0 | 31 / 31 | 3k / 3k | routing-only / routing-only |
| activity-order | 0 / 0 | 54 / 54 | 70 / 70 | 19k / 19k | forward-barycenter / forward-barycenter |
| uml-class | 0 / 0 | 5 / 17 | 61 / 63 | 12k / 13k | forward-barycenter / routing-only |
| bpmn-pools | 0 / 0 | 6 / 6 | 46 / 46 | 11k / 11k | routing-only / routing-only |
| network-vlan | 0 / 0 | 62 / 62 | 106 / 108 | 29k / 29k | routing-only / routing-only |
| layerweave-pipeline | 0 / 0 | 38 / 38 | 74 / 78 | 16k / 16k | routing-only / forward-barycenter |
| cloud-architecture | 0 / 0 | 30 / 32 | 83 / 88 | 27k / 28k | routing-only / keep-existing |
| mixed-layouts | 25 / 25 | 5 / 5 | 0 / 0 | 4k / 4k | forward-barycenter / forward-barycenter |
| nested-tree-radial | 0 / 0 | 0 / 0 | 88 / 88 | 18k / 18k | keep-existing / keep-existing |
| tree-100 | 196 / 196 | 0 / 0 | 0 / 0 | 28k / 28k | keep-existing / keep-existing |
| state-tcp | 0 / 0 | 0 / 0 | 36 / 36 | 6k / 6k | routing-only / routing-only |
| deployment-cluster | 0 / 0 | 107 / 132 | 136 / 188 | 60k / 61k | routing-only / routing-only |
| self-loops | 23 / 23 | 0 / 0 | 30 / 30 | 3k / 3k | keep-existing / keep-existing |
| layerweave-code | 0 / 0 | 900 / 850 | 493 / 1071 | 380k / 436k | routing-only / keep-existing |
| **all 16** | 244 / 244 | 1269 / 1252 | 1375 / 2026 | 633k / 693k | |

layerweave-code at import (before refinement): crossings 1030 / 850, bends
559 / 1071, shared route pairs 2 / 0.

Findings:
- Crossings fall on the large hierarchical view (layerweave-code 900 → 850) and on petri-net (62 → 56); they rise on
  uml-class (5 → 17) and deployment-cluster (107 → 132). Bends rise on almost every dataset that changes, most on
  layerweave-code (493 → 1071): the routes through border nodes change lanes at every container border and in every
  channel, because the placement does not yet align a border node with its chain inside and outside.
- On layerweave-code the routes from sections were drawn far worse than their plans until the channels were as wide as
  the lane changes of every edge, the changes were ordered without overlaps, and a container's own edges took border
  lanes; afterwards each area's drawn crossings are close to its plan's (diagnostic count during development).
- Refinement replaced the hierarchical import by a searched routing as long as the import had two shared route pairs
  (from a moved port slot whose route rejoined its old course); with the end segment slid to the new slot, the import is
  kept.
