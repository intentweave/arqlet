# ADR 0017: Edges across an area border of another axis are routed in legs between border ports

Date: 2026-11-07.
Status: proposed; implementation in steps (M2b of the single-pass plan).

## Context

Single pass builds every edge in one coordinate frame: `prepare_edge_inner` takes the axis of the smallest area that holds
both ends, and `realize_plan`, the lane computation, the joint and continuity track assignment and the corner construction all
compute in that frame. An edge from a leaf in a `tb` container to a node in its `lr` parent runs vertically inside and
horizontally outside, so today it is delegated (`MixedFlow`) and drawn as a seed route. On org-chart and workflow-dag that is
every edge between containers ([measurement](../../benchmarks/results/single-pass-axes-20261107.md)).

Border nodes exist only in a container of the same axis as its parent: an extra rank before the first or after the last, so the
edge crosses the container's border along the shared flow.

Later the same border must also take tree, organic, radial and straight regions, and drill-down (an area seen as a box with
ports).

## Decision

At the border between an area and a child area of another axis, an edge is split into **legs**. Each area routes its legs in its
own frame; the legs meet at a **border port** on the child's side. The route of the edge is the legs joined, in logical
direction.

1. **Side.** A child is left and entered on the sides its parent's flow uses: for a `tb` child in an `lr` parent, an edge leaves
   on the right and enters on the left. A backward edge (its link reversed by the cycle break) uses them the other way round, as
   border nodes do.
2. **Side nodes.** In the child's plan every such edge gets a virtual side node, like a border node: in the rank after its
   leaf's rank (leaving) or before it (entering), last across the flow (for the right or bottom side) or first (left or top),
   with its rectangle on the child's side. Its link to the leaf takes part in the order and the placement like any other. The
   inner leg is the chain from the leaf to the side node: down the child's flow, across a channel to the side.
3. **Border port.** The port is the side node's place on its own lane, inside the child, not on its border: legs that all ended
   on the border ran along it and overlapped. The side nodes of one rank are staggered a track apart along the child's flow, the
   first across the flow farthest along it, so that a leg running on across the border passes the lanes of the others where
   those have already turned; a side node is as long along the flow as its rank's ports need. The outer leg starts (or ends) at
   the port as an edge of the child container to the outside: the container is an element of its parent's rank, so the outer leg
   is an ordinary chain of the parent, and its first (or last) segment crosses the child's border.
4. **Port sides of the edge's ends.** An end inside a child of another axis has its port on the side that child's own flow
   leaves or enters by (`layered_plan::end_sides`), not on the side of the smallest area holding both ends.
5. **Positions before routing.** The side nodes are placed with the child, so the border ports are known before either leg is
   routed; both legs route against fixed ends in one pass, without iteration.
6. **Legs in the realization.** A leg is prepared from its two ends (point and side) and its frame instead of from two graph
   nodes. Joint, continuity and corner construction stay one frame per leg. A leg that cannot be constructed delegates the whole
   edge.
7. **Join.** The edge's route is the inner leg, the border port, the outer leg (reversed for a backward edge), validated as one
   route with its logical source and target.

The contract between two areas is therefore: ports on the child's border, positioned by the child, used by the parent. A region
of another strategy can offer the same ports later.

## Steps

- S1: side nodes for cross-axis children in the placement (structure, order, rank, rectangle), stored in the plan.
- S2: legs in the abstract plan instead of `MixedFlow`.
- S3: preparing a leg from its ends; realizing legs.
- S4: joining legs into routes; diagnostics count edges, not legs.
- S5: measurement against the refined default on the 11 mixed datasets; then the automatic choice may take mixed axes. Done 2026-11-08: every mixed dataset is better than the refined default over five seeds, and `singlePassAuto` takes mixed axes ([auto](../../benchmarks/results/single-pass-auto-mixed-20261108.md)).

Each step keeps single pass on uniform views byte-identical, and the default's fingerprints identical.

## Consequences

- A child of another axis grows by its side nodes; its channels carry the horizontal runs of the inner legs.
- `MissingBorder` (an edge to a container whose plan has no border node for it) is a separate case and stays delegated.
- Nested borders of alternating axes split an edge into more than two legs; S2 has to allow that.

## Amendment A (proposed, 2026-11-07; A1–A3 implemented 2026-11-08): lateral exits beside the end

### Context

Measured on the 11 mixed datasets (seed 0, against the refined default,
[gate report](../../benchmarks/results/single-pass-gate-mixed-20261107.md)): edges across an axis border draw 1968 bends where the
default draws 1249. Two groups, one cause:

| Edges across axes | edges | bends single pass | bends default |
|---|---:|---:|---:|
| through a middle area (an end in `lr` inside `tb` inside `lr`, as db-a on cloud-architecture) | 28 | 342 | 110 |
| one axis border | 278 | 1626 | 1139 |

A side node stands in the rank after (or before) its end. The inner leg therefore first runs along the child's flow to the next
rank, changes lane in the channel to the side node, and turns at the port, although the side it leaves by lies beside the end.
Through a middle area the detour repeats: on cloud-architecture the four edges of db-a leave az-a at its bottom, run to its lower
right corner and back up (e28: 14 bends, the default 2), and cross each other 9 times instead of 4. The sweep's model is right
about these drawings (forced to the default's orders, single pass draws what the model predicts); the gap is the port model.

### Decision (proposed)

1. **Lateral side node.** An edge that leaves (or enters) a child of the other axis gets its side node **in its end's rank**,
   beside the end, on the side the parent's flow uses, when nothing of that rank stands between the end and that side: the end
   is the last element across the rank for the right or bottom side, the first for the left or top side. Otherwise the side node
   stays in the rank after (before) the end, as in decision 2.
2. **Inner leg.** From a lateral side node the inner leg is one straight segment across the child's flow, from the end's port on
   that side to the side port. It needs no chain, no channel and no track; the port lies on the end's side (decision 4 then
   gives the side of the parent's flow, not of the child's).
3. **Several edges of one end.** The ports of an end's lateral edges lie on that side in the order of their side ports; the side
   ports keep the end's port places across the flow, so each segment stays straight.
4. **Through a middle area.** An end that is itself a child area of the parent's axis (db-a's data-a inside az-a) leaves by its
   own flow side, which is the parent's: its border node on that side and the middle area's lateral side node line up, and the
   edge crosses the middle area as one straight segment.
5. **Order.** First version: a lateral side node only where the order already puts the end last (first) across its rank. Second
   version, measured separately: the sweep may prefer such ends at the rank's end, counted like any other place.
6. **Contract.** The side port stays the contract between the areas: positioned by the child before routing, used by the parent.
   Only its place changes (beside the end instead of after it).

### Steps

- A1: placement: lateral side nodes where the end stands last (first) across its rank; rank, rectangle and order of the side
  node; port sides of the ends.
- A2: realization: a leg from an end to a lateral side node is a straight segment, validated against the child's elements; the
  join takes it like any inner leg.
- A3: middle areas (decision 4).
- A4: measurement against the refined default and against the current single pass on the 11 mixed datasets, five seeds; uniform
  views and the default stay byte-identical.
- A5: the order preference (decision 5, second version), measured on its own.

Status 2026-11-08: A1 and A2 are implemented and measured ([lateral side nodes](../../benchmarks/results/single-pass-lateral-20261108.md)):
on the 11 mixed datasets over five seeds crossings 4845 → 4623 and bends 4329 → 3741 (default 5135 / 3110); 9 better and 2 worse
than the default, as before; 7 views slightly worse than before by crossings. The ports of a side follow their side nodes' ranks
first, and the side nodes of one child follow their edges' places outside. 

Status 2026-11-08, A3: implemented and measured ([middle areas](../../benchmarks/results/single-pass-a3-20261108.md)): crossings
4623 → 3862, bends 3741 → 3422; 10 better and 1 worse than the default (uml-class by 0.6 crossings). Where the axes alternate over
four levels, the outer area takes the edge through. The parent counts its channels' tracks at the exact ports but aligns by order.
A5 is open.

### Consequences

- Fewer bends and shorter routes at axis borders where the end sees its side; elsewhere nothing changes.
- A child grows less along its flow (no extra rank for lateral side nodes).
- The realization gets one more kind of leg (the straight lateral segment); joint and continuity do not see it.
