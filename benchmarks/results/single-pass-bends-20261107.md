# Single pass: border alignment from the top down, 2026-11-07

Revision `ad00d81` against `ad00d81` plus border alignment (`crates/layout/src/border_alignment.rs`), Apple M3, release builds,
one run each (seed 0; earlier reports give five-seed means, so their single-pass numbers differ slightly). elk-rs
(`@openedges/elk-layout`, pinned in `benchmarks/eval/package-lock.json`) is deterministic, one run. Load average was 2.7 at the end,
higher before; no time is claimed here beyond "import unchanged within noise" (the per-view import times in the raw file).

Raw: [single-pass-bends-20261107.json](single-pass-bends-20261107.json) (before, after, and the rejected repeated pass). Commands:
`node benchmarks/eval/single-pass.mjs --corpus --repeat 1 --arms singlePass` (uniform, with elk-rs), the same without `--corpus`
(frozen views), and `--corpus --raw --repeat 1 --arms singlePass --no-elk` on the 11 mixed datasets; the "before" runs pass
`--binary` with a build of `ad00d81`.

**Dataset drift:** the playground build of the previous verification regenerated `benchmarks/datasets/layerweave-code.json`
(now 923 nodes / 1841 edges, sha256 `587e5cb20b7d78d7…`). Before and after here use the same file, but its numbers are not
comparable with earlier reports. The frozen views come from the stored replay and are comparable.

## Where the bends came from

On the uniform views single pass drew more bends than elk-rs mainly on edges across container borders. Per route, counted by the
area each bend lies in (seed 0, 8 views): the routes that bend in more than one area held almost the whole gap.

| View | bends (single / elk-rs) | routes bending in >1 area: bends (single / elk-rs) |
|---|---:|---:|
| deployment-cluster | 250 / 162 | 224 / 144 |
| network-vlan | 126 / 94 | 64 / 32 |
| org-chart | 184 / 148 | 72 / 36 |
| workflow-dag | 448 / 434 | 340 / 272 |

The reason: every area is placed on its own, from the inside out. A container's border nodes follow their neighbours inside,
and the parent then places the container as one element. An edge through several borders changes lane once per area, two
bends each, where a flat layout changes lane once.

## The change

After the placement, one pass visits the containers from the outside in. A border node whose edge changes lane both outside the
container and inside it moves onto the outside lane, or else onto the inside lane. It moves only within the gaps of its border
rank and the container's margins, so nothing else moves; the container's port follows it. A first version kept a full track
between neighbouring border nodes (node width plus track) and moved few nodes (deployment-cluster 250 → 242); with the lanes a
track apart, as in a channel, most candidates fit.

## Results

Uniform task, 31 views, sums:

| | before | after | elk-rs |
|---|---:|---:|---:|
| bends | 6690 | **6240** | 6290 |
| crossings | 7684 | 7680 | 9164 |
| line–node hits | 95 | 95 | 0 |
| hugging pairs | 13 | 12 | |
| shared line pairs (6), overlaps (0), delegated edges (64) | unchanged | unchanged | |

Hugging changed on two views: deployment-cluster 2 → 3, layerweave-code 3 → 1. Lexicographic per view against elk-rs: 20 better / 11 worse, before and after. Bends rank after crossings and hugging, so
the bends do not change a view's verdict here.

Views whose bends changed (all others identical):

| View | bends before → after | elk-rs | crossings before → after | elk-rs |
|---|---:|---:|---:|---:|
| workflow-dag | 448 → **300** | 434 | 234 → 230 | 278 |
| synthetic-2k | 1422 → 1288 | 1558 | 1725 → 1731 | 3660 |
| mixed-large | 461 → 415 | 368 | 529 → 525 | 625 |
| deployment-cluster | 250 → 212 | 162 | 114 | 71 |
| layerweave-code (new graph) | 1854 → 1824 | 2082 | 4894 → 4892 | 4296 |
| network-vlan | 126 → 112 | 94 | 30 | 28 |
| org-chart | 184 → 172 | 148 | 31 | 23 |
| cloud-architecture | 110 → 98 | 106 | 15 | 18 |
| activity-order | 90 → 80 | 90 | 16 | 36 |
| bpmn-pools, layerweave-pipeline, mixed-layouts | −2 each | | unchanged | |

Frozen views:

| View | bends before → after | elk-rs | crossings before → after | elk-rs |
|---|---:|---:|---:|---:|
| layerweave-code, 188 nodes | 1674 → **1604** | 1638 | 3830 → 3832 | 3057 |
| workflow-dag | 448 → **300** | 434 | 234 → 230 | 278 |
| org-chart | 184 → 172 | 148 | 31 | 23 |

Mixed datasets (raw task, 11 views): bends 4505 → 4435, crossings 4905 → 4901, no other change. Containers whose flow runs
across their parent's are skipped (their edges cross by side nodes and legs, ADR 0017); deployment-cluster 259 → 217 and
synthetic-2k 2175 → 2147 are the only views that changed.

Default output: identical to `ad00d81` on all 31 datasets at threads 1 and 4 (import and refine, byte-compared). The pass
runs only under `singlePass`.

## Rejected: repeating the pass

Repeating the pass until nothing moves (at most three times) saved 102 more bends on the uniform views but drew 97 more
crossings (layerweave-code +89) and delegated 4 more edges. Crossings rank above bends; one pass stays.

## Remaining gap

Single pass is now below elk-rs on bends in sum, and still above on deployment-cluster, network-vlan, org-chart and mixed-large.
There most remaining candidates are blocked because the outside lane lies beyond the container: aligning them needs the
parent to place the container where its border nodes meet their lanes, not only the border nodes to move. Containers of
another axis (side nodes, legs) are not yet aligned.

## Rejected: shifting the container in its parent

Second step, against `a06408e` (one run each, seed 0). Before aligning a container's border nodes, the same top-down pass moved
the whole container across the flow, within the gaps of its rank in the parent, to the shift at which the most of its edges meet
their outside lanes (the candidates: outside lane less border lane, per edge); the container's contents moved with it.

| | before | after |
|---|---:|---:|
| uniform, 19 views: bends / crossings | 4858 / 9530 | 4754 / 9271 |
| frozen code view: bends / crossings | 1604 / 3832 | 1530 / 3362 (elk-rs 1638 / 3057) |
| frozen code view: shared line pairs / delegated | 0 / 10 | **2 / 13** |
| mixed-large delegated | 13 | **14** |
| raw mixed, 11 views: bends / crossings | 4435 / 4901 | 4427 / 4895 |

Nearly the whole gain comes from one shift: `crate:runtime` in the frozen code view, by −3101 with 31 more edges straight.
The views the step aimed at (deployment-cluster, network-vlan, org-chart) did not change at all: their containers' edges come
from everywhere (ns-app in deployment-cluster: 31 edges whose aligning shifts range from −1261 to +728), so no rigid shift
straightens more than a few.

The new delegations: four edges into `global.rs::is_empty` fail with `JointOrder`, a cycle in the order constraints of
longitudinal segments whose lane domains lie closer than a track. Requiring a gain of 2 or 3 edges changed nothing; limiting
the shift to 0.25–2 container widths removed the large shift and with it the gain (code view at one width: crossings
6792 → 6984). Shared line pairs rank above crossings, so the step is rejected in this form.

The large shift is worth having once the realization survives it: the code view would come within 10 % of elk-rs on crossings.
That needs the joint order to break such cycles instead of delegating the edges.

## Joint order without blocking cycles

The joint order of the realization (`constructive/abstract_realization/joint.rs`) orders longitudinal segments whose lane domains
lie closer than a track. When the selected orders closed a cycle, every bend on it and downstream of it stayed unordered and its
edge was delegated. Now a selection that closes a cycle is repeated with a cycle check: the given orders (chain cuts) come first,
an order the bounds allow both ways gives way to the opposite order, and an order that closes a cycle either way fails only the
edge of its second interval.

No view of the uniform, frozen or raw sets has such a cycle at `a06408e`: all are byte-identical. With the rejected container
shift (above) the frozen code view delegated 13 edges without the check and 11 with a first version of it (selected orders
before the given ones; the final version, given orders first, was measured only on the side-lane trial below); the 2 left close a cycle in both
orders (their segments must overlap along the flow and their lanes are fixed less than a track apart).

## Side ports in the order of their lanes

A leaf's ports are ordered by their neighbours' places, and for an edge into a container by the place of the container's
border node. An edge through a side node (ADR 0017) had no such place: its ports took the order of the edge ids, while the side
ports lie the other way (the first side node of a rank farthest along the flow). On org-chart the edges of a team to its lead
crossed each other. The place now comes from the side node's lane, as the realization puts it.

Mixed datasets (raw task, 11 views, against `a06408e`, one run): crossings 4901 → 4850 (org-chart 119 → 70, deployment-cluster
and network-vlan −1 each); bends, hits, shared lines, hugging and delegations unchanged. Uniform and frozen views and the default
are byte-identical (the default on all 31 datasets at threads 1 and 4).

## Rejected for now: side ports as lanes of the placement

The parent's placement knew the lanes of a container's border nodes but not of its side nodes, so it aligned an edge with a share
of the container's side instead of the real port. Giving it the side ports (as `prepare_leg` places them) saved 284 bends and
354 crossings on the raw set (org-chart 436 → 330 bends, 119 → 74 crossings; synthetic-2k 3895 → 3521 crossings), but
workflow-dag got 549 → 606 crossings and 3 shared line pairs, cloud-architecture 40 → 50 crossings, and three views delegated 1–2
edges: the placement now aligned a side port exactly with a lane another edge runs on through the same channel, and the joint
order found no room between them. It needs the channel planning to keep such lanes a track apart before it can be used.
