# ADR 0027: Area inside single pass: ranks by span where they cost no quality, then free sibling areas along the flow

Date: 2026-11-23.
Status: proposed. Nothing is implemented; the default output is unchanged.

Related: [ADR 0016](0016-single-pass-default-gate.md) (single pass is the default), [ADR 0017](0017-area-border-legs.md)
(edges between areas in legs, border nodes), [ADR 0020](0020-single-pass-for-every-area.md) (single pass for every area);
issue #119; the measurements in
[`placement-hierarchy-20261123.md`](../../benchmarks/results/placement-hierarchy-20261123.md).

## Context

On the task of the elk-rs comparison (17 layered views, one direction, no strategies, bounded to 150 nodes) Arqlet's
default is ahead on every gate above area — crossings 3064 against elk-rs's 4983, bends 4272 against 4312, 26 % less
edge length, no line through a node — and behind on area: 1.60 times elk-rs's at the median. Issue #119 reported the
symptoms: nodes far out at the edges, tall layered views.

Constraints (decided 2026-11-23): **single pass stays the default, and the drawing's quality must not get worse** —
per view, in the order of `DEFAULT_OBJECTIVES` (overlaps, line–node hits, crossings, then bends, then length and area).
As before, a change runs behind an option first, and the default output stays fingerprint-identical until a new default
is accepted.

Where single pass spends the extent (`benchmarks/eval/area-budget.mjs`, an exact breakdown of both axes):

| | along the flow | across the flow |
|---|---|---|
| leaves | 55 % | 32 % |
| container boundaries | 23 % (channel at a border rank 13 %, frames 8 %, border rank 2 %) | 20 % (gaps between stacked sibling areas 10 %, frames 10 %) |
| edges | ordinary channels 21 % | long edges 34 % (dummy tracks and the gaps beside them) |

Measured and ruled out under the constraints:

- **`globalLayering`** (shared ranks across areas, ELK's `INCLUDE_CHILDREN`) would buy most of the gap (1.24× elk-rs),
  but cannot run with single pass: abstract routing refuses a flattened placement, and the layout falls back to the
  legacy pipeline.
- **`packComponents`** repacks unlinked components as boxes and trades height for width: no area gain, more crossings
  and bends, also under single pass.
- **A narrower channel beside a border rank** (prototype `borderChannelGap`): four fifths of that channel are the two
  margins the construction needs; below them routes fall back to the search and crossings triple; the free fifth buys
  0.7 % area and costs bends.
- **Tighter spacings and frames**: rejected on 2026-11-22 (more bends and crossings, line–node hits).
- **Packing the long edges tighter across the flow**: they stand on the lines that keep them straight; packing costs
  bends, which rank before area.

What remains is the rank assignment inside an area. The existing experimental option `simplexRanks` (network simplex
instead of longest paths, after Gansner, Koutsofios, North and Vo, IEEE TSE 1993; `crates/layout/src/ranking.rs`) was
measured under single pass for the first time:

| | crossings | bends | length | area | area / elk-rs (median) | views worse in objective order |
|---|---:|---:|---:|---:|---:|---:|
| uniform task, default | 3064 | 4272 | 2482075 | 227248 k | 1.60 | |
| uniform task, `simplexRanks` | 2931 | 4290 | 2287983 | 206002 k (−9.3 %) | 1.39 | 3 of 17 |
| configured datasets, default | 5310 | 4934 | 2676331 | 196959 k | | |
| configured datasets, `simplexRanks` | 5213 | 4943 | 2570804 | 191298 k (−2.9 %) | | 4 of 17 |

Longest paths put every element in its earliest rank, so an element whose edges leave the area stands in the first rank
and its edges run as dummy chains through every rank to the border; minimising the summed spans moves it towards its
targets and removes dummies (the length falls by 8 % and 4 %). The gain is concentrated (org-chart −33 %, mixed-large
−18 %, synthetic-2k −8 %), and on 3 to 4 views it costs one to seven crossings. It does not resolve the stacking on
layerweave-code (−3.7 %): its 19 unlinked areas move together towards the border rank but stay in one rank.

## Decision (proposed)

Two steps, each behind its own option, each measured and accepted before the next.

### Step 1: simplex ranks per area, kept only where they cost no crossings

Single pass constructs every area on its own, so the ranking can be chosen per area. Each layered area is ranked both
ways — longest paths and network simplex — and ordered both ways; the simplex ranks are kept for that area only when
its ordered plan has **no more crossings** than the longest-path plan (the plan's crossings between neighbouring ranks,
`layered_plan::order_crossings`, as `benchmarks/eval/plan-crossings.mjs` reads them), ties broken by fewer dummies, then
by the longest-path ranks. The choice must be deterministic and independent of the thread count, and an area whose
choice is the longest-path ranks must lay out byte-identically to the default (a requirement of the implementation,
checked against the stored fingerprints).

The guard is a hypothesis to test, not a result: the plan's crossings are known before routing and moved with the drawn
crossings between the arms measured so far, but whether they catch the 3 to 4 views where `simplexRanks` costs
crossings is exactly what step 1 has to show. If they do not, the fallback is a guard on the constructed routes of the area, which
costs a construction per area.

Cost: a second ranking and ordering per area; recorded with the measurement.

### Step 2: unlinked sibling areas spread along the flow

For the stacking that step 1 leaves (layerweave-code: gaps between stacked sibling areas 4235 of 9581 across the flow):
an element whose links leave it slack between its earliest and its latest feasible rank takes, within that slack, the
rank that balances the ranks' extents across the flow, counting an area by its box. This is width-bounded layering
(Coffman and Graham, Acta Informatica 1972; the minimum-width heuristic of Healy and Nikolov, "How to layer a directed
acyclic graph", GD 2001; ELK offers it as its `MIN_WIDTH` and `STRETCH_WIDTH` layerers). Only the ideas are taken, from
the papers; no code is copied. It changes the ranks inside one area, so ADR 0017's legs and border nodes are untouched.

It is not obviously a gain: on layerweave-code the unlinked areas are linked only to the border rank, so an area moved
away from the border lengthens its chain again, and dummies also cost room across the flow. The trade has to be
measured, not assumed.

### Acceptance, for each step

Run `benchmarks/eval/option-gates.mjs` on both poses (uniform and configured):

1. **No view worse than the default in objective order** — overlaps, line–node hits, crossings, then bends — on either
   pose. A view may trade bends for fewer crossings, as the objectives allow; it may not trade either for area.
2. Area in sum and the median against the default and against elk-rs recorded; single pass and delegated routes
   unchanged or better.
3. Every view on which the option chooses the default ranks everywhere byte-identical to the default (the stored
   fingerprints); times recorded on an idle machine.

Only then the default question, with new reference fingerprints and the engine version raised as for earlier default
changes.

## Consequences

- If step 1 is accepted, part of the measured 9.3 % (uniform task) stays — the part on the areas where simplex ranks do
  not cost crossings. The upper bound is known; what the guard keeps is not.
- Neither step changes spacings, frames, the construction's margins or the straightness of long edges, so the reasons
  for which those were ruled out still hold.
- The remaining gap to elk-rs after both steps is expected to be the containers' frames and channels and the straight
  long edges — the cost of readability choices already made, not of placement.

## Open questions

- Does the plan-crossing guard predict the drawn regressions (step 1)? If not, is a per-area construction affordable?
- Which views lose under step 2, and by how much length?
- `layerweave-code` is generated from the repository's source and drifts (measured copy: 1105 nodes / 2461 edges);
  record its size and hash with every figure.
