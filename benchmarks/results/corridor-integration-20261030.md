# Corridor widening and constructible edges, 2026-10-30

Machine: Apple M3, 8 logical CPUs, macOS 25.0.0, revision 0bcfd56 plus uncommitted changes, load about 5 (not idle).
Measured with `bench --constructive 1` on the import: once with `layeredConstruction` on (the import places and routes with the
constructed routes fixed), once without (`tracks-20261030.md`, baseline). Per dataset: `corridor-integration-20261030.jsonl`.

What the layout already does: `corridor_gaps` widens the corridor between two siblings to 2 × margin + (k − 1) × track, where k is
the number of routes through it. A delegated edge is routed by the general router, so its route widens the corridor it needs. This
is an implicit integration; the constructor's own demand (NoFreeTrack) is not passed to `corridor_gaps` explicitly.

| dataset | constructed (with option) | NoFreeTrack (with option) | constructed (without) | NoFreeTrack (without) |
|---|---:|---:|---:|---:|
| petri-net | 14 | 5 | 14 | 5 |
| layerweave-code | 29 | 5 | 27 | 7 |
| all other datasets | unchanged | unchanged | | |

Reading: the existing widening changes almost nothing on constructible edges (layerweave-code +2, NoFreeTrack −2). On petri-net the five
edges without a track stay without one, so their missing track is not a missing corridor width. The fixture of the narrow corridor
(`fixture-narrow-corridor-20261030.md`) shows that widening helps when the corridor between two boxes is too narrow. These edges are
blocked by boxes inside a rank, which widening does not move. An explicit feed of the constructor's demand into `corridor_gaps` is
therefore not expected to help here; the next step is to look at what blocks the five petri-net edges.
