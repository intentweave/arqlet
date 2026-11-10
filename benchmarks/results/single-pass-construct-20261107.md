# Single pass: no conflict reroute after construction, 2026-11-07

Phase-level work on the provisional `singlePass` mode (`{"abstractRouting":true,"singlePass":true}`), after
[the phase work](single-pass-phase-20261107.md). Revision `d14db45` plus uncommitted changes, Apple M3 (8 logical
CPUs), release builds. The default path is unchanged: all 20 stored default fingerprints are identical to a build of
`HEAD` (threads 1 and 4).

## Finding

The phase report said that nothing is rerouted after construction. That was wrong. A sampled profile of the code-view
import showed that the conflict reroute in `compute_counted` still took **21 % of the import**: it rerouted 7 routes
with the legacy search against 351 fixed routes, then finished and decorated the drawing again. The conflicts came
from delegated edges, which single pass draws as seed routes (straight Z-routes through the channel midpoint, and one
generic self-loop that hit its neighbour).

## Changes

Each cause of delegation is now handled where the route is constructed. All three apply to `singlePass` only.

1. **Channel width for the joint bend compaction.** The channel growth measured the demand of a channel with interval
   slots only, but the joint bend compaction needs more room. It then failed on channels that were 6 px short
   (`JointCapacity`), and the affected edges were delegated. The growth now runs the joint compaction's lower bounds
   (`joint::demand`, no upper bounds) in its demand-only realization, including edges already marked as narrow, and
   widens each channel to that demand. The main realization then has the same 8529 constraints and no capacity failure.
2. **No same-rank link after the cycle break.** The Kahn cycle break could leave a link inside one rank, which has no
   chain (`MissingChain`; code view e217, org chart e1). The longest-path re-ranking along the Kahn order, until now used
   for flattened areas, also runs under `singlePass`.
3. **Self-loops on adjacent sides.** A self-loop whose ports lie on adjacent sides is drawn around the corner:
   `[source, out, corner, out, target]`, one bend fewer than the generic loop, outside its own node. It goes out
   20 px plus `selfLoopClearance`; when another node on that side is closer than that distance plus the required
   clearance, the loop runs in the middle of the free room.

On the three frozen views no route conflict is left, so the reroute does not run. The code view still delegates its
8 self-loops whose ports are not on adjacent sides; they keep their seed routes, without hits.

## Instruction counts

Retired instructions per import, `/usr/bin/time -l bench --graph VIEW --limit 1000 --reps N --threads none
--pipeline-only 1 --import-options '{"abstractRouting":true,"singlePass":true}'`, (T(5) − T(1)) / 4.

| View | phase work | now |
|---|---:|---:|
| layerweave-code | 1004 M | **831 M (−17 %)** |
| workflow-dag | 149 M | 155 M (+4 %) |
| org-chart | 158 M | **124 M (−22 %)** |

The workflow view had nothing to reroute; it pays for the joint demand pass, which runs whether or not a channel needs
it.

## Quality: seed means

Five shuffle seeds (sweep runs ≥ 3), 33 views: 30 corpus views and the 3 frozen views. The corpus `layerweave-code` view
is left out, because it is generated from the repository source, which changed. Sums of the single-pass arm, mean over
the seeds:

| | Crossings | Bends | Line–node hits | Issues |
|---|---:|---:|---:|---:|
| phase work | 6642 | 7067 | 101.2 | 8.2 |
| **now** | **6644** | **7185 (+1.7 %)** | **95.8** | **7.0** |

Per view, mean over the seeds (crossings / bends):

| View | phase work | now |
|---|---|---|
| layerweave-code (frozen) | 3439 / 1594 | 3554 / 1606 |
| org-chart | 36.6 / 180.8 | 28.8 / 185.6 |
| workflow-dag | 237.2 / 452.0 | 235.6 / 452.8 |
| petri-net | 49 / 73 | 32 / 84 |
| petri-net-shapes | 62 / 109, 40.8 hits | 44 / 102, 35 hits |
| mixed-large | 611 / 423 | 533 / 475 |
| synthetic-2k | 1763 / 1452 | 1784 / 1510 |
| network-vlan | 26 / 134 | 24 / 124 |
| uml-class | 0 / 43 | 1 / 37 |
| bpmn-pools | 1 / 34 | 1 / 32 |

All other views are unchanged. The frozen code view is worse by 3.3 % crossings; the corpus as a whole is not. Issues:
petri-net-shapes and self-loops lose their clearance issue; mixed-large has one more near-node pair (a delegated seed
route, e88 beside `mesh-n0`), in a view that already has 3 geometry issues and 46 hits.

## Quality gate, frozen views

`node benchmarks/eval/single-pass.mjs --repeat 1 --arms singlePass --no-elk`, raw:
[single-pass-construct-20261107.json](single-pass-construct-20261107.json). Corpus:
[single-pass-construct-corpus-20261107.json](single-pass-construct-corpus-20261107.json), 5310 crossings / 6471 bends,
95 hits, 7 issues. The machine was heavily loaded (load 21), so the times in these files are not meaningful.

| View | singlePass | before | elk-rs (phase report) |
|---|---|---|---|
| layerweave-code | 3369 / 1634, 0 hits | 3362 / 1618 | 3057 / 1638 |
| workflow-dag | 234 / 448, 0 hits | 234 / 448 | 278 / 434 |
| org-chart | 31 / 184, 0 hits | 30 / 190 | 23 / 148 |

## Open

- The joint demand pass costs the workflow view 4 %; skipping it when no channel can be short is open.
- The 8 delegated self-loops of the code view (ports on opposite or equal sides) and the delegated edges of mixed-large.
- Next by profile: the geometry placement pass, the sweep, and the growth's demand realization.
