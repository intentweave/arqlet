# ADR 0027: Area inside single pass: ranks by span where they cost no quality, then free sibling areas along the flow

Date: 2026-11-23.
Status: proposed. Step 0 done (2026-11-23): the plan-crossing guard **fails** (see "Outcome of step 0"). Revised
2026-11-23 after step 0: the per-area guard (former step 1) is dropped; `simplexRanks` becomes a refinement candidate
(step 1 below), and the composition of the areas (step 2) is the main lever. Step 1 implemented and measured
(2026-11-26) behind `simplexRanksCandidate`, off by default: no view worse than the default in objective order, but the
expected gain does **not** hold (see "Outcome of step 1"). Step 1 is not accepted; the default output is unchanged.

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

Two steps, each behind its own option, each measured and accepted before the next; step 1 starts with an offline check
of its guard (step 0).

### Step 0: test the guard offline, before any engine change

The guard of step 1 is read from the plan, which the probes can already compute without an engine change
(`benchmarks/eval/plan-crossings.mjs`). For every view of both poses, and per layered area: the plan's crossings and
dummies with longest-path ranks and with simplex ranks, and the choice step 1 would make. The check passes when, on the
3 (uniform) and 4 (configured) views where unguarded `simplexRanks` is worse in objective order, the guard rejects the
simplex ranks on the areas that cause the loss, and keeps them on the areas that carry the area gain (org-chart,
mixed-large, synthetic-2k). Recorded in a results file, per view and area.

If it fails, the fallback is decided before step 1 is built: a guard on the constructed routes of the area, whose cost (a
construction per area) is measured on the same views first; or no step 1.

#### Outcome of step 0 (2026-11-23)

Measured with `benchmarks/eval/simplex-guard.mjs` on both poses
([results](../../benchmarks/results/simplex-guard-offline-20261123.md)); the attribution of drawn crossings and bends to
areas adds up exactly to the `metrics` totals. The guard fails:

- **Most regressions are not in the area whose ranks changed.** Of 16 loss areas on the 7 worse views, 11 lie outside
  every changed area: 2 in ancestors, 5 in **children** (a parent's new ranks move where its edges enter a child) and 4
  **apart** (siblings and cousins). The split into a local guard and a global acceptance assumed ancestors only; a guard
  that compares two layouts of one area — from its plan or from its constructed routes — cannot see any of the 11.
- **Inside the changed areas the plan predicts poorly**: the guard rejects simplex on 2 of the 5 loss areas there, and
  its verdict agrees with the drawn one on 12 of 15 changed areas (uniform) and 11 of 17 (configured).

So the listed fallback, a guard on the constructed routes of an area, could catch at most 5 of the 16 and cannot pass the
acceptance. One fallback that sees every regression is a **choice per view**: lay the view out with both rankings and
keep simplex only where the view is not worse in objective order (ties: the smaller area). From the gate runs, exactly:
simplex on 7 of 17 views, crossings 3064 → 2929, area −9.4 % on the uniform task; 6 of 17, crossings 5310 → 5194, area
−3.2 % on the configured datasets; no view worse by construction. Its cost is a second layout per view, and more on views
that do not run single pass (configured mixed-large: `simplexRanks` 4.5 to 7 times slower on the legacy pipeline), so it
would be limited to views that run single pass and timed on an idle machine first.

Decided (2026-11-23): **no per-area guard, and no choice per view in the layout.** A choice per view in the layout would
double the cost of every layout, the interactive steps (open, toggle, filter) included, for at most 9.4 % area, which
ranks last in the objective order; and the choice could flip with any edit, so that one toggle moved every rank of the
view (ADR 0001, incremental layout), unless it were stored and kept. The choice per view is taken instead where its cost
is off the interactive path: as a refinement candidate (step 1 below). Step 0 also shows where the lever for #119 is:
rank changes act mostly on *other* areas, so the composition of the areas among themselves (step 2) comes before more
work on the ranks inside one area.

### Step 1: `simplexRanks` as a refinement candidate

The refinement already runs off the interactive path (a second worker or the backend, `refine`), compares candidate
drawings by the published key in a fixed order, and publishes a result only for the revision it started from, so a stale
result is refused (ADR 0006). Step 1 adds one candidate there: the view laid out again with network-simplex ranks
(`simplexRanks`); it replaces the drawing only when it is better by the published key. So:

- **No view worse, by construction**, in the published key's order (which follows the objectives). The gate runs above
  give the expected result: simplex kept on 7 of 17 views, crossings 3064 → 2929, area −9.4 % (uniform task); 6 of 17,
  crossings 5310 → 5194, area −3.2 % (configured datasets).
- **The interactive steps keep their cost and their ranks**: the layout is unchanged; only a refinement exchanges the
  drawing, as for every refinement candidate.
- **New for single pass:** the refinement plans no candidates on a single-pass drawing today (ADR 0016); this is its
  first. It gets its own candidate identity (strategy `simplex-ranks`, `execution::CandidateId`), counts in the
  deterministic work budget like the others, and is independent of the thread count.
- **Limited to views that run single pass**: on the pipeline before (configured mixed-large) `simplexRanks` is 4.5 to 7
  times slower, and that pipeline has its own candidates.
- **Kept across edits** (a requirement of the implementation): an edit after a refined drawing lays out with the ranking
  the refinement chose, stored with the snapshot and omitted when it is the default, so the ranks do not flip back with
  every toggle and the default output stays byte-identical; a new import starts from the default ranks.

Optionally the same choice as an option for hosts that lay out exactly once (the ELK drop-in, an export), where a second
layout per view does not matter: `rankChoice: "best"`, behind the same acceptance.

Cost: a second layout per refinement of a single-pass view, measured on an idle machine before acceptance.

#### Outcome of step 1 (2026-11-26)

Implemented as decided, behind the option `simplexRanksCandidate` (off, omitted): the candidate `simplex-ranks` is
planned on a single-pass drawing without `simplexRanks`, is the view laid out again from the start (as a view change
lays it out), counts as one job of the work budget, and replaces the drawing only when it is better by the published
key, also where the input reports geometry issues. The ranking is kept as `simplexRanks: true` in the result's options.
Measured on both poses, limit 150, against the current corpus
([results](../../benchmarks/results/simplex-refinement-20261126.md)):

| pose | views with simplex ranks | crossings | bends | area, sum | area / elk-rs (median) | expected above |
|---|---:|---|---|---:|---|---|
| uniform (17 views) | 5 | 4245 → 4181 | 4239 → 4287 | −5.5 % | 1.70 → 1.60 | 7 views, −9.4 % |
| configured (16 views, `can-bus` fails to refine with and without the option) | 3 | 6491 → 6483 | 4869 → 4847 | **+0.2 %** | not measured | 6 views, −3.2 % |

- **Acceptance 1 holds:** no view is worse than the default in objective order; three uniform views trade bends for
  fewer crossings.
- **Acceptance 2 recorded:** single pass and delegated routes are unchanged.
- **Acceptance 3 holds for identity:** with the option off, import and refinement answer byte for byte as the build
  without the change on all views of both poses, and the 15 reference fingerprints equal it; where the default ranks are
  kept, the snapshot differs only in the option and the count of evaluated candidates.
- **Times (noisy; the machine was loaded, load average 10 to 42 on 8 cores):** on the single-pass views of the uniform
  task the refinement took 39 ms in sum without the option and 522 ms with it, against 407 ms for the imports; per view
  it adds 1.1 times the import at the median (configured: 45 → 392 ms, 313 ms, 1.2 times). The measurement on an idle
  machine that this step asks for is still open.
- **Area plays no part in the choice:** the published key has no area (its last criterion is the route length), so
  all three chosen views of the configured pose, and three of the five uniform ones, get larger (by 0.5 to 11.6 %). The
  uniform sum comes from `org-chart` (−32.8 %) and `synthetic-2k` (−8.4 %).
- **Why fewer views than step 0 predicted:** step 0 counted a choice by overlaps, hits, crossings, bends and then area,
  on a corpus whose `layerweave-code` has since grown (1105/2461 then, 1474/3254 now; simplex is worse on it now). The
  same rule on today's drawings would take 6 uniform views (−7.5 %) and 5 configured ones (−1.8 %). The candidate takes
  8 of those 11: the published key ranks shared route pairs above bends (`synthetic-2k` configured: 9 → 13);
  the geometry rules refuse the candidate of the uniform `mixed-large`, whose default drawing reports the same kind of
  container-bounds issues; and the configured `mixed-large` does not run single pass.
- **Kept ranks after edits:** on the views that took them, 158 edits (containers collapsed, nodes added) all kept
  `simplexRanks`; against the same edit of the default drawing 135 were better in objective order, 6 equal and 17 worse.
  A later refinement does not try the default ranks again.

Open for the decision: whether this gain is worth the second layout; whether the choice should weigh area (a different
key for this candidate, not tried: it would not be "better by the published key"); whether a refinement of a drawing
with kept ranks should compare the default ranks too; and whether the candidate may move a continued drawing. It is a
layout from the start, as measured here: after an added node, the continuation differs from a layout from the start on 43
of 72 tries (default ranks, nine views), so a refinement there can exchange more than the ranks; the published key has no
movement term. A candidate warm-started from the drawing gives other drawings than the measured ones and was not taken.

### Step 2: unlinked sibling areas spread along the flow

The main lever (step 0: rank changes act mostly on other areas). For the stacking of unlinked sibling areas
(layerweave-code: gaps between stacked sibling areas 4235 of 9581 across the flow):
an element whose links leave it slack between its earliest and its latest feasible rank takes, within that slack, the
rank that balances the ranks' extents across the flow, counting an area by its box. This is width-bounded layering
(Coffman and Graham, Acta Informatica 1972; the minimum-width heuristic of Healy and Nikolov, "How to layer a directed
acyclic graph", GD 2001; ELK offers it as its `MIN_WIDTH` and `STRETCH_WIDTH` layerers). Only the ideas are taken, from
the papers; no code is copied. It changes the ranks inside one area, so ADR 0017's legs and border nodes are untouched.

It is not obviously a gain: on layerweave-code the unlinked areas are linked only to the border rank, so an area moved
away from the border lengthens its chain again, and dummies also cost room across the flow. The trade has to be
measured, not assumed. A cheap upper bound comes first: ELK's own `MIN_WIDTH` and `STRETCH_WIDTH` layerers, run through
the pinned elk-rs (`benchmarks/eval`) on the same views and poses, against elk-rs's default layering; if they buy little
area there, step 2 is not built.

#### Upper bound of step 2 (2026-11-26)

ELK's `MIN_WIDTH` and `STRETCH_WIDTH` layerers through the pinned elk-rs, default parameters, against its default
layering ([results](../../benchmarks/results/simplex-refinement-20261126.md)): on the uniform task `MIN_WIDTH` costs
60.6 % area in sum (smaller on 2 of 17 views; the smaller of both per view saves 0.2 %), `STRETCH_WIDTH` costs 39.8 % on
the 10 views it finishes within 90 s and is smaller on none; as configured elk-rs refuses 13 of the 17 views, and both
are larger on the other 4. By the rule above this speaks against building step 2. The bound is not tight: ELK layers the
whole hierarchy at once and counts leaves, step 2 would spread sibling areas inside one area and count an area by its
box; no width parameter was tuned. Whether to drop step 2 is the owner's decision.

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

- If step 1 is accepted, a refined single-pass view keeps the simplex ranks where they make it better by the published
  key: −9.4 % area on the uniform task, −3.2 % on the configured datasets, from the gate runs; the layout itself and its
  cost stay as they are.
- Neither step changes spacings, frames, the construction's margins or the straightness of long edges, so the reasons
  for which those were ruled out still hold.
- The remaining gap to elk-rs after both steps is expected to be the containers' frames and channels and the straight
  long edges — the cost of readability choices already made, not of placement.

## Open questions

- Answered by step 0: the plan-crossing guard does not predict the drawn regressions, and most of them lie outside the
  area whose ranks changed; no per-area guard can see them.
- Step 1: the time of the second layout per refinement on an idle machine (the times of 2026-11-26 were taken on a
  loaded one). The ranking is kept in the options (`simplexRanks`), which are omitted when default.
- Which views lose under step 2, and by how much length?
- `layerweave-code` is generated from the repository's source and drifts (measured copy: 1105 nodes / 2461 edges);
  record its size and hash with every figure.
