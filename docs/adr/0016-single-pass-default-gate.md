# ADR 0016: The gate for making single pass the default

Date: 2026-11-07.
Status: accepted on 2026-11-08. Single pass with the automatic choice is the default from engine 0.2 on.

## Context

`singlePass` is an opt-in that rejects 26 of the 31 unmodified datasets. Its results so far are on a simplified task with a uniform
direction. CLAUDE.md accepts algorithm changes by quality gates, not fingerprints, and the choice of a default strategy is an open
decision. [The first measurement on the real task](../../benchmarks/results/single-pass-raw-20261106.md) is the starting point.

## Decision

Single pass may become the default where all of these hold, measured on the unmodified corpus against the refined default.

Hard, without tolerance:

- valid geometry: no geometry issue on a view where the default has none;
- complete edges in logical direction; cycles, self-loops and parallel edges work;
- stale results are rejected; output is identical for any thread count;
- every operation of `dispatch` works in all three deployment modes, falling back rather than failing.

Quality:

- per view, lexicographic in the order of `DEFAULT_OBJECTIVES`, over seed means: no view worse than the default without a documented
  reason in this ADR;
- line–node hits: no view worse than the default. CLAUDE.md asks for 0 hits, which the default itself misses on 4 views; this stays
  the target. Decided 2026-11-08: "not worse" suffices for the switch.

Deliberate actions:

- what the user placed or configured holds: a moved or pinned node, a direction, a size, a port. This is a gate.
- the movement of the other nodes is reported, not gated: the more a drawing is optimized, the less stable it may be on an edit.
  Measured so far: single pass moves other nodes further than the default on shape, measure and toggle.

A dataset for which the mode does not apply stays byte-identical to the default, apart from option and diagnostic fields.

elk-rs remains the reference for the single-pass target on the uniform task; on mixed views it is no reference, because the adapter
sets one direction at the root.

## Consequences

The switch itself raises `engine_version`, so that old snapshots are rejected explicitly rather than changing meaning: defaults are
omitted from serialized snapshots, and `layoutVersion` is the stale counter of a drawing, not a format version.

## Outcome (2026-11-08)

Measured over five seeds on the 16 unmodified datasets the automatic choice takes, single pass is better than the refined default
on all 16 ([gate](../../benchmarks/results/single-pass-gate-20261108.md)); no view has line–node hits in either pipeline. The 15
other datasets (organic, tree and straight areas, a view budget) fall back and stay byte-identical to the pipeline before, apart
from the option and diagnostic fields. Timing: [at the switch](../../benchmarks/results/single-pass-timing-20261108.md).

The switch: `singlePassAuto`, `singlePass` and `abstractRouting` are on by default and omitted; a fallback writes the two flags as
`false`. `ENGINE_VERSION` is `0.2.0-prototype`, so snapshots of 0.1 are rejected. The reference fingerprints of the new default
are [fingerprints-20261108-single-pass-default.jsonl](../../benchmarks/results/fingerprints-20261108-single-pass-default.jsonl);
`singlePassAuto: false` keeps the pipeline before, and its fingerprints stay those of
`fingerprints-20261021T103351Z.jsonl`.

Known consequences: a pin and a multi-node drag are not single-pass inputs, so such an edit continues on the pipeline before
(the container's inside is then laid out by it); the refinement plans no candidates on a single-pass drawing. (Pins: see the amendment *Pins in single pass*
below; a drag step and a drop with release stay on single pass since #169 and the order hints.)

## Amendment: pins in single pass (2026-11-24)

Status: proposed. Nothing is built; the behaviour below comes behind an option first (`pinsInSinglePass`, off) and is
accepted by the quality gate as every algorithm change.

### Context

A pin is a node's position in its parent's frame (`LayoutOptions::pins`). Single pass refuses every pin today, and so the
whole view leaves single pass while the pin stays, not only the pinned node:

- a pinned **leaf**: "The hierarchical pipeline does not yet support pinned leaf placement; the previous layout pipeline
  was used";
- a pinned **container**: "Single pass was not used: Fixed-order channel growth does not support pinned nodes".

The pipeline before places the arrangement as it did and then overwrites the pinned boxes with their pins (free siblings
too close to a pin move aside, `PIN_CLEARANCE`). The pinned node is where it was asked to be, and the geometry is valid;
what gets worse is everything else. Seen in the playground and reproduced on `can-bus` (2026-11-24): after a drag with
the pin, routes run along container frames, nodes are entered from the side, and the explicit buses fall apart (`Bus can:
edge … leaves or enters across the flow and keeps its own route`; `limiter-out` is no longer drawn as a bus), and every
later edit, a collapse included, does the same while the pin stays.

Two drags already stay on single pass: a drag step is a preview that only re-routes what the move touches (#169), and a
drop with release becomes an **order hint** (`drop_order`: the node stands after the last node of its rank whose middle
across the flow lies before the drop). Only the drop that keeps the pin, and a pin set by hand, leave it.

### What a pin has to hold in a layered area

A layered area places a node by three things: its **rank** (where it stands along the flow), its **order** in the rank,
and its **coordinate across the flow** (placement). Its coordinate along the flow is its rank's band, which the channels
between the ranks give (channel growth by demand). An exact pin fixes both coordinates; the rank, the order and the cross
coordinate can be derived from it, the along-flow coordinate only within the rank's band.

### Options

1. **Overlay** (as the hierarchical pipeline does for a pinned container): the plan unchanged, the box overwritten with
   the pin after the placement. Exact, but single pass constructs every route from the plan's places, so the edges of a
   pinned node that stands elsewhere would all go to the completing router, and the channels around it would be sized for
   a node that is not there. Close to what the pipeline before draws now.
2. **Pin as an arrangement constraint**: the pin chooses the node's rank (the rank whose band holds its middle along the
   flow, else the nearest), its order (as `drop_order`), and fixes its coordinate across the flow in the placement; along
   the flow it stands in its rank's band. The view stays a layered single-pass drawing, every route is constructed, buses
   are drawn as buses. The node is exactly where it was put across the flow, and along the flow at most half a band away.
3. **Exact along the flow as well**: as 2, and the rank's band is grown so that the pinned node stands at its pin's
   along-flow coordinate inside it (the channel before and after the band shrink and grow accordingly). Exact on both
   axes; it needs channel growth to take a per-node offset inside a band, which "fixed-order channel growth" refuses today,
   and it moves the band for every node of that rank.

### Decision (proposed)

Option 2 first, option 3 only if the measurement shows that the along-flow difference bothers.

For a pinned node (leaf or container) in a layered area under single pass:

- **Rank**: fixed in the ranking, to the rank whose band holds the pin's middle along the flow (bands of the stored plan,
  else of the plan of this layout without the pin), or the nearest. An edge that then runs against the flow is drawn as a
  reversed edge, as an edge of a cycle is; logical direction is kept.
- **Order**: the node stands after the last other node of its rank whose middle across the flow lies before the pin's
  (the rule of `drop_order`), as an order hint that the ordering keeps; several pinned nodes of one rank take the order of
  their pins.
- **Across the flow**: the node's coordinate is fixed in the placement (a fixed variable), and its neighbours keep at
  least the spacing from it. The order above makes that feasible; a fixed container size that leaves no room is reported
  as a geometry issue, as today.
- **Along the flow**: the node stands at the start of its rank's band, as the other nodes do; the difference to the pin
  is reported per pinned node in the layout diagnostics (`Pin x: held across the flow, N along it (rank r)`), so a host
  can show it.
- Organic, tree and radial areas, which hold pins exactly already, and regions (ADR 0020) keep what they do.

The drag in the playground then stays on single pass whatever its end: finish with release gives an order hint, finish
with the pin gives the constraint above.

### Steps

- **P1**: leaf pins in layered areas behind `pinsInSinglePass`. Measured on the layered examples and `can-bus`: a pin by
  hand and a drag with the pin keep the view on single pass, the explicit buses stay buses, geometry valid, every edge in
  logical direction; the distance of each pinned node to its pin along and across the flow; crossings and bends against the
  pipeline before with the same pin.
- **P2**: pinned containers (the same mapping for the container as an element of its parent's area; its inside is laid
  out by single pass as before).
- **P3** (only if P1 shows that the along-flow difference bothers): option 3.
- Then the default decision; while the option is off, the default fingerprints stay identical (they have no pins).

### Open questions

- The ranking with fixed ranks: longest paths and network simplex both take a fixed rank as a constraint; how far a fixed
  rank against the flow may go before it should be refused rather than reversing edges.
- Pins in a view within a node budget or with communities: the pin names a node that may be hidden or aggregated; today
  `validate_pins` refuses unknown nodes, and the mapping would apply only to visible ones.
- Whether the diagnostic per pinned node is enough for a host, or whether a pinned node's offset belongs in the scene.
