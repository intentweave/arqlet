# Single pass on mixed axes: legs across the axis border (M2b, S1–S4), 2026-11-07

Revision `cce04a7` plus these changes, Apple M3, release builds. No time is claimed. Design: [ADR 0017](../../docs/adr/0017-area-border-legs.md).

## What changed

- **Side nodes** (S1): in a container whose flow runs across its parent's, an edge to the outside gets a virtual node in the rank
  after its child (leaving) or before it (entering), last or first across the flow, as long along the flow as the ports of its
  rank need. Only under `singlePass`.
- **Legs** (S2): the abstract plan splits such an edge at the side node: the leg inside is a chain to the side node, the leg
  outside a chain from the container in its parent. An edge with one leg is planned as before.
- **Leg preparation** (S3): a leg is prepared from its two ends in the frame of its area. The port is the side node's place on its
  own lane, staggered a track apart along the flow so that no two legs share a lane, and the end of the edge inside such a
  container has its port on that container's own flow side (`layered_plan::end_sides`).
- **Join** (S4): the realization joins the legs into the edge's route in logical direction; an edge with a leg that cannot be
  built is delegated whole.

Single pass on the 31 normalized and 3 frozen views is byte-identical to the previous commit (34 of 34). The automatic choice
still keeps mixed axes on the other pipeline.

## Constructed edges

| Dataset | edges | constructed, M2a | constructed now |
|---|---:|---:|---:|
| org-chart | 82 | 0 | 76 |
| workflow-dag | 125 | 0 | 87 |
| deployment-cluster | 56 | 45 | 56 |
| synthetic-2k | 318 | 85 | 209 |
| network-vlan | 47 | 28 | 43 |
| activity-order | 40 | 20 | 33 |

Two intermediate states are part of the finding: with ports on the container's border the legs ran along it and overlapped
(`LongitudinalCapacity`), and with the ports of one rank at one place they met in one point. Both were fixed by the staggered
ports on the side nodes' own lanes.

## Quality on the 11 mixed datasets

Five seeds, against the refined default, lexicographic in the order of `DEFAULT_OBJECTIVES`. Raw:
[single-pass-legs-20261107.json](single-pass-legs-20261107.json).

4 better (org-chart, state-order, synthetic-2k, workflow-dag), 7 worse; M2a: 3 better.

| Dataset | shared line pairs | crossings | hugging | bends |
|---|---|---|---|---|
| workflow-dag | 217 → 15 | 610 → 1265 | 232 → 41 | 573 → 717 |
| synthetic-2k | 90 → 52 | 4100 → 5454 | 126 → 39 | 1563 → 1888 |
| org-chart | 8 → 3.4 | 122 → 148 | 52 → 4 | 308 → 400 |
| cloud-architecture | 0 → 0 | 31 → 55 | 0 → 0 | 80 → 137 |
| activity-order | 2 → 7 | 86 → 133 | 22 → 5 | 88 → 150 |

Shared lines and hugging fall sharply; crossings and bends rise. The order across an axis border is not yet crossing-aware: side
nodes stand fixed at the end of their rank, and the hierarchical sweep sees border nodes but not side nodes. A few routing issues
appear (org-chart 0.4, workflow-dag 0.6, on average).

## Crossing-aware order across the border

The hierarchical sweep now sees a container of another axis like a container with border nodes: its edges count in its parent at
the places of their ports (rank, then depth among the side nodes of the rank), the sweep that descends into it orders the side
nodes of every rank by their neighbours outside (the farthest deepest), the sweep inside keeps that order, and the final placement
takes it from the sweep's plan. Areas without side nodes are unchanged (uniform single pass 34 of 34 byte-identical).

Five seeds, same task. Raw: [single-pass-legs-order-20261107.json](single-pass-legs-order-20261107.json).

| | legs | + order |
|---|---:|---:|
| better / worse than the refined default | 4 / 7 | 5 / 6 |
| sum of crossings, 11 datasets (default 5185) | 7349 | 7137 |
| workflow-dag crossings / constructed edges | 1265 / 87 | 1526 / 74 |
| synthetic-2k crossings | 5454 | 5031 |
| layerweave-pipeline crossings | 56 | 39 |

The order helps on most datasets and makes workflow-dag worse; crossings stay 38 % above the default in sum.

## Side nodes in the order of their children

On workflow-dag the crossings were in the root's channels (756 against 12 for the default): the ports along a stage's side did
not follow the stage's leaves. A leg inside an area crosses no other leg when the side nodes of a block stand in the order of
their children (the first child's leg reaching the deepest port), so the placement now orders each block by its children, and
the hierarchical sweep models the side places the same way and orders the children (not the side nodes) by their neighbours
outside when it descends into such a container.

| | crossings, 11 datasets | bends | better / worse | workflow-dag crossings |
|---|---:|---:|---:|---:|
| order through the sweep | 7137 | 3748 | 5 / 6 | 1526 |
| + side nodes by children | 6177 | 4018 | 5 / 6 | 809 |
| + the sweep's model the same | 6004 | 4071 | 6 / 5 | 771 |
| default | 5185 | 3147 | | 610 |

Raw: [single-pass-legs-children-20261107.json](single-pass-legs-children-20261107.json). workflow-dag now constructs 119 of 125
edges (74 before), with 23 unresolved strict track pairs (298 before).

What remains on workflow-dag is a limit of the port model, not of the order: a leg leaves a `tb` stage only downwards and enters only
from above, so every pair of edges whose sources and targets are in opposite orders crosses in the root's channel. The default
routes freely and takes such an edge above or below the row of leaves; it has more such inversions (410 against 324) but only 136
crossings in all.

## Rejected: ports on both sides along the child's flow

Tried: the edges between two areas of the other axis split in two groups, one leaving after its child and entering after its child
(far side), the other leaving and entering before (near side), the far group being a longest chain without inversions between the
children's orders in the two areas. Two edges of different groups then do not cross between the areas. An estimate from the
drawing promised fewer inversions (workflow-dag 324 → 118, synthetic-2k 208 → 49; the other mixed datasets have none).

Measured on workflow-dag (proper crossings, inside the stages / in the root): 100 / 608 before, 755 / 218 with the groups, and 72
instead of 119 edges constructed. The crossings move from the channels into the stages: every stage has edges entering from the
stage before and leaving to the one after, and with two groups both kinds share both of its channels, where a leaving leg from a leaf
left of an entering leg's target always crosses it. The convention of entering before and leaving after keeps them apart, which is
why it is kept. synthetic-2k and org-chart also failed (a moved side node left a rank empty, which the growth rejects); that was not
fixed, since the approach does not pay off. The change was reverted.

## Port offsets of edges through side nodes

The import of synthetic-2k spent 75 % of its time in the conflict reroute after the construction, caused by the delegated edges'
seed routes (106 of 318). Most were delegated for `LongitudinalCapacity` at their first or last lane: two legs began or ended at the
same port. `plan_port_offsets` finds the side of a leaf's port from its chain to the other end, and for an edge to the outside it
looked only for a border node, not for a side node. Without a chain it put a source on the outgoing side and a target on the
incoming side, which is wrong for a backward edge; its port then shared the place of another port on that side. It now finds the
side node too. `port_lanes` also takes a leaf's offsets only from the plan of its own area.

| Dataset | constructed before | now |
|---|---:|---:|
| org-chart | 78 of 82 | 82 of 82 |
| workflow-dag | 117 of 125 | 125 of 125 |
| activity-order, bpmn-pools, cloud-architecture, layerweave-pipeline, network-vlan | some delegated | all |
| synthetic-2k | 219 of 318 | 279 of 318 |

The import of synthetic-2k fell from 483 to 196 ms (one run each, same machine, load about 2).

Five seeds against the refined default. Raw: [single-pass-legs-ports-20261107.json](single-pass-legs-ports-20261107.json).

| | crossings | bends | shared line pairs | hugging | better / worse |
|---|---:|---:|---:|---:|---:|
| before | 6004 | 4071 | 68.6 | 76.0 | 6 / 5 |
| now | **4925** | 4517 | **11.4** | **24.4** | **9 / 2** |
| default | 5185 | 3147 | 318.6 | 462.4 | |

The crossings are now below the default's in sum (workflow-dag 588 against 610, synthetic-2k 3884 against 4100). Worse: cloud-architecture
(40 against 31 crossings) and uml-class (9.8 against 8). Bends are 44 % above the default's.

## Rejected for now: ports in the channel

Most of the extra bends on the mixed views come from the ports: a leaving leg runs from its leaf into the channel, across to its side
node's lane, along that lane to the port in the side node's rank, and the leg outside then turns again (workflow-dag: 6.9 bends per
edge with legs against 4.6 per edge in the default; cloud-architecture 9.2 against 2.8). With the port in the channel, where the leg runs
across anyway, the edge would go on across the border without the two extra bends.

Tried twice (one sample per view):

- Ports in the channel, staggered from the channel's start where the joint compaction puts its bends: bends per edge with legs fell
  (org-chart 5.3 → 3.6, cloud-architecture 9.2 → 3.8, workflow-dag 6.9 → 6.0), but legs failed again (org-chart 19 of 82 delegated,
  workflow-dag 19 of 125), because the compaction did not put the leg's bend at its port, and the rest of the lane then collided.
- The same with the leg's bend fixed at its port in the joint compaction: the fixed bends collided with the other bends of the channel,
  which the compaction also puts from its start, and most legs failed (workflow-dag 8 of 125 constructed).

Reverted. Making it work needs the channel planning to know that the inner leg ends in the channel: its end as a track of its own,
planned with the other tracks of the channel, and the channel grown for it.

## Next

Bends on the mixed views (4517 against 3147 for the default); the remaining delegated edges of synthetic-2k (39: missing border nodes and
self-loops); then the automatic choice may take mixed axes, by the gate of ADR 0016.
