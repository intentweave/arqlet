# ADR 0029: A patch into a bus view: lay it out as an import, make that faster, and keep an incremental patch for later

Date: 2026-11-27.
Status: proposed (#212). Nothing is built; the owner decides before anything is.

Related: [ADR 0001](0001-incremental-first.md) (incremental first), [ADR 0006](0006-reactive-layout-session.md)
(delta-driven repair), [ADR 0012](0012-local-edits.md) (local edits), [ADR 0016](0016-single-pass-default-gate.md)
(gates), [ADR 0019](0019-engine-interface.md) (sessions), #156, #203 (`sparseRepairGrid`, #216), #219 (second routing
pass threshold), #220 (bus taps). Measurement: [bus-patch-20261127.md](../../benchmarks/results/bus-patch-20261127.md).

## Context

A `patch` that adds the children of some nodes and their edges to a bus view (an in-place drill-down) costs as much as a
fresh import of the whole graph. A host expects it to be much cheaper. The issue asks first where the time goes, then
for a choice between a faster layout of such views (A) and an incremental patch (B).

### What the measurement says

On the issue's public generated views (`bus-mesh.mjs`, Apple M3, release, load 4 to 11, see the results file):

| view (units/buses, children/edges) | patch | where the time goes |
|---|---|---|
| organic lines 100/30, 240/1 000 | 16.3 s | second routing pass 93 %, of it overlap resolution 74 % |
| organic lines 280/107, 480/2 100 | 3.4 s | placement 35 % (separation 23 %), first routing 37 %, bus-line drawing 26 %; no second pass (1 954 conflicts > 500) |
| organic lines 60/12, 60/250 | 2.1 s | second pass 86 %, overlap resolution 59 % |
| layered lines 100/30 and 280/107 | 0.17 s, 0.74 s | first layout; but the drawing is wrong (below) |
| organic bar 100/30, 240/1 000 | 2.0 s | first layout; drawing wrong (below) |

Why a patch equals a fresh import today: `patch_snapshot` lays out non-abstracting views with every node dirty from the
earlier positions. In an organic `lines` view that warm start never happens: an organic area starts warm only when every
child has an earlier box, and the bus nodes the layout adds get none back (`bus_line::expand_snapshot` restores the box
only of a bus that is still a bus line; `bars_to_lines` made each one an explicit bus with edges, its box sits in
`bus_boxes`). The root starts cold and every later step is deterministic, so the drawing is a fresh import's. No route
is ever kept (`routesKept` 0): `local_routes` refuses because the earlier graph is the view after `bars_to_lines`, and
the issue's patch adds every edge of the view anyway (the overview has only bus taps).

Where the warm start does happen, it is harmful: layered `lines` views end with 1 043 and 2 091 line–node hits where a
fresh import has 0 (earlier plans reused, no hierarchical-ordering rebuild), and an organic `bar` view with 1 374
against 36. Commit `771898d` (2026-11-13) already took abstracting views off this path for the same reason: a warm start
there made a container with 1 173 loaded member edges 45 times taller
([toggle-layout-20261112.md](../../benchmarks/results/toggle-layout-20261112.md) records the toggle side: the local layout
was the slowest and least clean in every case).

### Probes run for this ADR (probe build, not merged: `benchmarks/experiments/bus-patch-phases.patch`)

- **Incremental placement (B's placement without its routing):** only the added nodes and their containers dirty, bus
  boxes restored, so the unchanged nodes hold. With every edge new it gives 3 to 22 times the line–node hits of a
  fresh import (better only on the smallest view). Adding one more container to a view that has the others: 279 of 280
  units held; line–node hits 311 against 1 408 on the large view (better), 114 against 109 on the dense one (worse);
  shared lines 1.8 times and an eighth. Time: 2.5 s against 3.6 s on the large view, 13.4 s against 16.3 s on the dense
  one (the second pass still runs).
- **No second routing pass:** the dense view in 1.8 s instead of 16.8 s, with 10 more line–node hits and 4.2 times the
  shared line pairs. The pass is what keeps shared lines down.

## Options

### A. Make the layout of such views faster (helps the import too)

- **A1. A faster overlap resolution, output unchanged.** `resolve_overlaps` (`crates/routing/src/lib.rs`) is 59 to 74 %
  of the views that get the second pass. Per round and per overlapping route it builds the outlines of every obstacle
  the route may not cross and tests each candidate move against them and the segment index. Candidates: obstacles in a
  spatial index built once per call (the outlines filtered by the route's reach, `6 × track`), the per-route outline list
  cached across rounds while the route has not moved, a cheaper count of overlap changes per candidate. A pure speedup:
  byte-identical output, checked against every stored fingerprint file (CLAUDE.md, Layout options). Unknown before a
  profile: the factor it gives.
- **A2. The second-pass threshold (#219).** The 500-conflict switch decides both time and shared lines: views just under
  it pay 15 s, views over it nothing. Dropping the pass is not acceptable (above). A deterministic work budget
  proportional to the view (counted in candidate tests, not time) bounds the worst case; it changes output, so it goes
  behind an option and through the gates. #203 (`sparseRepairGrid`) moves views across the threshold and makes this more
  urgent, it does not save time by itself.
- **A3. The bus-line drawing (26 % of the large view).** `bus_line::collapse` searches taps (`tap_routed`); #220 changes
  how taps are found; a speedup belongs with it.
- **A4. Separation on the large view (23 %).** Not looked into; smaller than A1 to A3.

### B. An incremental patch

Keep the places of what the patch does not touch, lay out the new content inside its containers, let containers grow
and push their neighbours, route only the new edges and the routes the move affects, keep the bus lines.

- **Placement rule.** A grown container moves the siblings wholly right of its earlier box by its growth in width and
  those wholly below by its growth in height, up the tree (the rule `push_grown` in `crates/layout/src/lib.rs` already
  applies within a node budget); no stress on held nodes. Not the warm-started stress that gave drawings 45 times taller
  (`771898d`). New content inside a container is laid out as at an import (the container's own area, cold).
- **Routing.** Routes of edges whose ends moved together are carried along (as `local_routes` does, ADR 0012); new
  edges and carried routes that now meet a box are routed; the second pass and its overlap resolution are restricted to
  the routes in the region the change reaches. `local_routes` must first accept bus views (compare the earlier drawing
  on the graph the layout saw, before `bars_to_lines`).
- **Bus lines.** A bus whose members and their boxes did not move keeps its bar and taps; the others are drawn again
  (`bus_line::collapse` per bus).
- **Fallback.** As a local edit: when the result has more line–node hits than the drawing before (on the edges it had),
  the complete layout decides (`editReport.completeFallback`), so a patch is never worse there than an import.
- **Stale results and revisions.** Unchanged: `patch_snapshot` refuses a snapshot whose revision or layout version is not
  the expected one; the result has revision + 1, `baseRevision`, layout version + 1. The incremental path reads only the
  snapshot it is given; nothing is cached outside it.
- **Deployment modes.** The same `patch` operation through `dispatch`, so split, browser-only and backend-only (and
  sessions) run it alike; the option travels in the snapshot's `LayoutOptions`.
- **Determinism.** Pure function of snapshot, patch and options; the layout of one view runs serially, so any `threads`
  value gives the same output. Native and WASM: `PortableMath` for trigonometry, no `usize` overflow; add patch cases to
  `tools/platform-parity.mjs`.
- **Option.** `incrementalPatch` (provisional, off, omitted when off): the default output stays byte-identical until the
  gates accept it.
- **Gates.** Per view, against a fresh import of the patched graph with the same options: valid geometry (no geometry
  issue, no sibling overlap); every edge drawn, in logical direction; line–node hits not above the fresh import's; then
  the order of `DEFAULT_OBJECTIVES` (sibling overlaps, line–node hits, shared route pairs, routed crossings, near-node
  segments, projected crossings, bends, cost: bends before length and area, never traded the other way); units held
  reported, not gated; time with a results file (machine, revision, load). elk-rs is no reference for bus views (no
  buses or organic areas); it applies only to layered views without buses, on the uniform pose.

**What B can save here.** In the issue's patch every edge is new, so B routes everything anyway; the saving is the
placement (up to 35 %, large view) and part of the bus drawing. The probes say the held placement alone is not safe when
the patch brings most of the view (3 to 22 times the hits). B pays off for patches that add a small part to a drawing
that already has most of its routes, and only once the routing is local too.

### C. A patch lays the view out as an import

Extend what `771898d` did for abstracting views to every view whose warm start does not hold the measured quality (bus
views, and on the evidence of the layered result all layered views with reused plans): the patch is then exactly a fresh
import of the patched graph with the same view, which removes the line–node hits of layered and `bar` patches (1 043,
2 091 and 1 374 to 0, 0 and 36). Cost: layered patches take an import's time (0.23 s instead of 0.17 s); organic `lines`
patches are unchanged (they already are imports). It changes the default output of patches (to the accepted import
drawing), so the owner decides whether it counts as a fix or goes behind an option.

## Recommendation

1. **C now** (small): a patch into a view with buses (or every non-abstracting view, the owner's choice) is laid out as
   an import. Effort: about a day with tests (`graph_patch.rs`: patch equals import for layered and `bar` bus views).
2. **A1 next**, with a cheap probe first: profile `resolve_overlaps` on 100/30/organic/lines/240/1000 and try an
   obstacle index and the outline cache. **Go** if the second pass gets at least 3 times faster with byte-identical
   output on every stored fingerprint file and the bus-mesh views; **no-go** otherwise (then A2 is the only lever for
   these views). Probe 2 to 3 days; the change itself a few days more. It makes import and patch faster together.
3. **A2 with #219** behind an option, measured on the generated bus meshes and the reference views; **A3 with #220**.
4. **B later, only behind a probe B1** on patches that add a small part (the `--partial` cases, and a container opened
   in a corpus view): local routing for bus views plus the push rule. **Go** if on those cases line–node hits are not
   above the fresh import on any view, no view is worse in the order of the objectives without a recorded reason, and
   the patch takes at most a third of the fresh import; **no-go** otherwise (C stays). Probe about a week; the full
   option two to three weeks (placement rule, local overlap resolution, bus lines per bus, fallback, parity, docs).

Why this order: the measured time is in routing that a patch has to do anyway when it brings most of the edges; A1 is the
only step that cuts it without a quality risk, and C removes a quality defect of today's patch at no cost to the views of
the issue. B's main risk (more line–node hits, an earlier 45 times taller drawing) is real on the evidence, and its
benefit on the issue's patches is bounded by the placement share.

## Risks

- A1 may give less than hoped: the candidate tests themselves, not the obstacle lists, may dominate. The probe settles it.
- C makes layered patches slower by an import's difference and changes their output; hosts that compare drawings across
  versions see the change (the import drawing).
- A2 changes shared lines and time per view in both directions (#216 showed up to 13.7 times on single views); it needs
  the full gate run.
- B: local overlap resolution may leave overlaps at the region's border; the push rule may give wide or tall drawings in
  organic areas; `bars_to_lines` makes the earlier graph differ from the layout's, which `local_routes` must learn to
  compare. Each is a reason for the probe gate.

## Decisions for the owner

1. C: for views with buses only, or for every non-abstracting view; a fix of the default or behind an option.
2. Whether A1 is taken on before #219, and the factor that counts as go (proposed: 3 times on the second pass).
3. #219: a work budget (output changes, behind an option) or keep the threshold until A1 is measured.
4. Whether B is pursued at all for 0.11, and on which patches it is measured (proposed: the `--partial` cases plus an
   opened container in a corpus view; the issue's all-edges-new patch is not a case where B can win).
5. The tolerance of "units held" (reported only, per ADR 0016) and whether a patch may move other nodes at all.

## Not measured

elk-rs (no comparable task for bus views); WASM and other platforms (native probes only); repeated runs of the variants
(one run each, under load 6 to 11); views larger than 280 units and real exports.
