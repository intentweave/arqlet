# Containers as nodes: first step on layerweave-code, 2026-10-30

Machine: Apple M3, 8 logical CPUs, macOS 25.0.0, release profile, rustc 1.99.0. Revision 0bcfd56 plus uncommitted changes. Load about 5
(not idle). `bench --graph layerweave-code --limit 100 --threads 1 --reps 1`; "on" = `layeredConstruction` on the import. Times are single
runs; the counters are deterministic. Per-edge data of the last row: `repair-work-hierarchical-20261030.json`.

Changes, all behind the experimental option (default output unchanged: 16 of 16 fingerprints identical):
1. The constructor takes edges between containers and edges with a container as an end: the route is one monotone path when every area
   from each end up to the common container flows along the same axis. Containers on the way are left or entered through their borders.
2. `corridor_gaps` reserves the planned tracks: every edge is lifted to its common container; the corridor before the child that holds the
   target, and after the child that holds the source, needs a track per edge.
3. The layered placement widens the corridor between two levels by those widths (it widened only inside a level before).

| variant | refinement ms | legacy candidates | grid pops | hard | crossings | length | bends |
|---|---:|---:|---:|---:|---:|---:|---:|
| off (existing path) | 7221 | 2134400 | 2251396 | 0 | 1327 | 238956 | 748 |
| on, constructor without containers | 5891 | 1608340 | 2280869 | 0 | 2030 | 268799 | 771 |
| on, step 1 (containers as endpoints) | 5021 | 1573540 | 1817624 | 0 | 1924 | 277011 | 718 |
| on, steps 1–3 (with reserved corridors) | 4992 | 1566000 | 1928355 | 0 | 1819 | 298950 | 697 |
| on, steps 1–4 (several tracks per gap) | 4289 | 1056180 | 842633 | 0 | 1745 | 295225 | 691 |

The reservation takes effect: on the root level the corridor before crate:model grows from 64 to 186 units, the one after crate:runtime
from 64 to 306.

Remaining repair work (steps 1–3): 63 edges without a free track hold 86.8 % of the grid pops; constructed edges 11.6 % (their candidates
during refinement); port side 1.6 %.

Reading: treating containers as nodes removes the cross-area rejection, and the refinement is 31 % faster than the existing path. But the
grid pops fall only by 14 %, and crossings and length are higher than on the existing path. The edges that still search have no free track
inside the containers: an edge from deep inside crate:runtime must pass the later ranks of each container on its way to the border, and
there it has no reserved lane. The reservation covers only the common level so far, not the inner levels and not the ranks a long edge
passes (dummy nodes, plan section 4.2).

## Step 4: several tracks per gap (2026-10-30)

Diagnosis first: "no free track" was split into two reasons. All 63 remaining edges had a way between the boxes, but every track was
taken (`TrackTaken`), none was blocked by boxes (`NoFreeTrack`). The constructor offered only one track per free gap (its centre), so
the widened corridors could carry only one route each.

Change (behind the option): the path search runs in two passes. The first finds the way over the gaps' centres, without regard to the
routes already built. The second searches the concrete tracks only inside the gaps on that way, with every track that fits at the track
spacing (centre first, at most 32 per gap), and keeps the spacing from the routes already built. One pass over all tracks of all gaps was
tried first and was too slow (more than ten minutes on layerweave-code, stopped).

Result on layerweave-code (row "steps 1–4" above): grid pops 2.25 M → 0.84 M (−63 % against the existing path), refinement 7.2 s → 4.3 s
(−41 %), hard violations 0. Constructed edges 52 → 86, edges with taken tracks 63 → 28. The import rises from 135 ms to 705 ms (the
construction now searches more tracks). Crossings (1745 against 1327) and length (295 k against 239 k) stay above the existing path.

Remaining repair work (`repair-work-hierarchical-tracks-20261030.json`): TrackTaken 28 edges, 68.6 %; constructed edges during
refinement candidates 28.8 %; port side 2.6 %.

Default output unchanged: 16 of 16 refinement fingerprints identical.

## Step 5: track order per corridor from the logical plan (tried, reverted), 2026-10-30

Change tried (patch: `corridor-order-20261030.patch`, against step 4): every edge is first checked and its way planned over the gaps'
centres; the lane changes of all edges in one corridor are ordered so that they cross least (pairwise crossing count, neighbours swapped
while a swap lowers it, starting from the order of the lanes' midpoints); each edge's lane change is then drawn towards its place in that
order by a cost of 2 per unit of distance in the track search.

Result on layerweave-code (import drawing, `bench --constructive 1` with the option; and the refinement):

| variant | constructed edges | crossings among constructed (at shared ends) | crossings, all | import ms | refinement ms | grid pops |
|---|---:|---:|---:|---:|---:|---:|
| step 4 | 86 | 271 (94) | 1603 | 705 | 4289 | 842633 |
| step 5 | 92 | 341 (80) | 1716 | 1007 | 5558 | 792726 |

Reading: the order does not lower the crossings (per constructed edge 4.2 before, 4.6 after) and makes the import and the refinement
slower. Reverted to step 4. Two observations for the next attempt: about three quarters of the crossings involve edges routed by the
general router, not the constructed ones; and the order fixed only the lane changes across the flow, while the lanes along the flow (the
long runs past the ranks) and the port lanes are still given in edge order.

## Step 6: flow-side ports in the port resolver, 2026-10-30

Change (behind the option): the port resolver puts the automatic ports of an edge on the flow sides (right/left, bottom/top) when every
area from each end up to the common container has the same fixed flow and the target lies after the source. Otherwise it keeps the side
facing the peer. The published ports (`finish_routes`) and the seed drawing use the same rule, so routes start and end at the published
ports (the first version left them inconsistent; two integration tests caught it). The constructor now checks the direction before the
port side, so a backward edge is reported as NotForward.

| variant | refinement ms | import ms | legacy candidates | grid pops | hard | crossings | length | bends |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| off (existing path) | 7221 | 135 | 2134400 | 2251396 | 0 | 1327 | 238956 | 748 |
| step 4 | 4289 | 705 | 1056180 | 842633 | 0 | 1745 | 295225 | 691 |
| step 6 (flow-side ports) | 4111 | 1042 | 818960 | 1067327 | 0 | 1698 | 280992 | 577 |

Remaining repair work (`repair-work-flow-ports-20261030.json`): TrackTaken 22 edges, 61.5 %; constructed edges during refinement
candidates 26 edges, 36.5 %; NotForward 9 edges, 2.0 %; port side 52 edges, 0.05 % (forward edges the rule does not reach, nearly free).

Reading: constructed edges rise from 86 to 98; bends fall by 17 % and length by 5 % against step 4, crossings by 3 %. The grid pops rise
again (0.84 M → 1.07 M), mostly through constructed edges that need the repair search in the refinement's candidate drawings. The
refinement is 43 % faster than the existing path; the import is 7.7 times slower (135 ms → 1042 ms). Default output unchanged: 16 of 16
fingerprints identical.

## Step 7: a second way when the gaps are full, 2026-10-31

Finding first: the refinement's ordering candidates already run the same layout path as the import (`candidate` → `compute_mode` →
`compute_once`) with the same options, so the reservation, the flow sides and the construction apply there too. All grid pops of the
refinement come from the four ordering candidates (about 250 k each in step 6); there they come from the same cause as in the import:
tracks taken in the gaps of the first way.

Change (behind the option): when the tracks of the first way's gaps are taken, the search looks for a second way over the gaps' centres
that avoids the routes already built, and then for tracks in that way's gaps (the second way itself is the fallback).

| variant | refinement ms | import ms | legacy candidates | grid pops | hard | crossings | length | bends |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| off (existing path) | 7221 | 135 | 2134400 | 2251396 | 0 | 1327 | 238956 | 748 |
| step 6 | 4111 | 1042 | 818960 | 1067327 | 0 | 1698 | 280992 | 577 |
| step 7 (second way) | 4738 | 1494 | 630460 | 690073 | 0 | 1892 | 287425 | 569 |

Grid pops per ordering candidate (step 7): 144 k, 128 k, 250 k, 167 k (step 6: 255 k, 243 k, 327 k, 242 k).

Remaining repair work (`repair-work-second-way-20261031.json`): TrackTaken 31 edges (12 with search), 59.2 %; constructed edges during
the candidates 37.9 %; NotForward 2.9 %. Constructed edges 98 → 141.

Reading: the grid pops fall by 35 % against step 6 (−69 % against the existing path), but the time rises: the import's routing stage
grows from 1.0 s to 1.5 s, and that stage now holds the construction. Every step of the construction's search is checked against every
obstacle and every segment already built; with more edges and more ways this dominates. Crossings rise by 11 %. The next lever is an
index for those checks (no change of output).

## Step 8: an index for the construction's checks, 2026-10-31

Change (no change of output): every step of the construction's search was checked against every obstacle and every segment already
built. A uniform grid over the obstacles' boxes and over the built segments' boxes (each widened by the track spacing) now hands over
only the entries whose boxes touch the step's box; the exact tests are the same. Built once per edge.

Output check (experimental path, `layeredConstruction` on, 9 datasets): geometry fingerprint, input geometry fingerprint and grid pops
identical on 9 of 9 (`constructive-index-before-20261031.jsonl`, `constructive-index-after-20261031.jsonl`). Default output unchanged
(16 of 16 refinement fingerprints).

Times (single runs, same machine, load about 5):

| dataset | import ms before → after | refinement ms before → after |
|---|---|---|
| layerweave-code | 1400 → 495 | 4592 → 2269 |
| tree-100 | 98 → 46 | 391 → 219 |
| petri-net | 20 → 19 | 132 → 133 |
| network-vlan | 13 → 11 | 139 → 139 |
| others (activity-order, layerweave-pipeline, cloud-architecture, bpmn-pools, state-order) | within 1 ms | within ±11 ms (noise) |

Against the existing path on layerweave-code: refinement 7221 → 2269 ms (−69 %), grid pops 2.25 M → 0.69 M (−69 %), import 135 →
495 ms (3.7 times slower), hard violations 0, crossings 1327 → 1892 (+43 %), bends 748 → 569 (−24 %), length 239 k → 287 k (+20 %).
