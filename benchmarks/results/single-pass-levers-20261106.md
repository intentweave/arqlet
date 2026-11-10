# Single pass: optimization levers, 2026-11-06

Block O of the plan to make `singlePass` the default: known levers first; output may change if quality does not get
massively worse. Revision `d14db45` plus uncommitted changes, Apple M3 (8 logical CPUs), release builds, load 1.5–6
(shared machine). Cost is judged by instruction counts only; no time is claimed here.

## Baseline

Retired instructions per import, `/usr/bin/time -l bench --graph VIEW --limit 1000 --reps N --threads none
--pipeline-only 1 --import-options '{"abstractRouting":true,"singlePass":true}'`, (T(5) − T(1)) / 4, plain release build
(no `benchmark-timings`). Repeated runs of one binary differ by about 1 %, so smaller changes are not a result.

| View | instructions per import |
|---|---:|
| layerweave-code | 784 M |
| workflow-dag | 145 M |
| org-chart | 115 M |

Sampled profile of the frozen code view (`sample`, 6 s over repeated imports, 2352 samples), inclusive, each recursive
function counted once per stack:

| Part | Share |
|---|---:|
| abstract completion (`completion::run_impl`) | 45.6 % |
| `realize_plan` | 27.9 % |
| `place` | 24.7 % |
| hierarchical sweep | 16.4 % (`Model::crossings` 6.5 %) |
| `growth::widen_planned` | 12.8 % |
| `joint::reserve` | 9.1 % |
| `planning::Session::plan` | 6.5 % |
| `Geometry::rect` | 3.8 % |
| `memcmp` including its stub, all callers | 26.3 % |
| `malloc`/`free`, all callers | 16.8 % |

The parts overlap. `memcmp` is spread out (string-keyed maps): the largest named caller is `Geometry::rect` with 2.9 %,
and no other named caller exceeds 1.6 %. String keys and allocation together are the largest cost, and no listed lever
addresses them.

## Lever: end a sweep run after the first sweep without improvement (adopted)

`hierarchical_sweep` ends a run after four sweeps without improvement. Limit 1 was tried under `singlePass`, with an
experimental idle limit and shuffle seed from the environment (both removed again).

Corpus (31 views, uniform direction, 150-node cut), mean over five seeds:

| idle limit | crossings | bends | hits | geometry + routing issues |
|---:|---:|---:|---:|---:|
| 4 (current) | 6092 | 6476 | 95 | 7 |
| 3 | 6093 | 6476 | 95 | 7 |
| 2 | 6095 | 6473 | 95 | 7 |
| 1 | 5503 | 6398 | 95 | 6 |

Frozen views, mean over eight seeds (crossings / bends):

| View | limit 4 | limit 1 |
|---|---|---|
| layerweave-code | 3708 / 1615 | 3896 / 1623 |
| workflow-dag | 233.9 / 452.3 | 235.8 / 453.0 |
| org-chart | 31.5 / 187.5 | 31.9 / 185.5 |

Against the limits of the plan, over all 34 views with five seeds each (31 corpus views, 3 frozen views):

- Sum of crossings 9984 → 9552 (−4.3 %), sum of bends 8729 → 8670 (−0.7 %), hits 95 → 95, corpus geometry issues 4 → 4.
- No view is over the per-view limit. Largest increases in crossings: activity-order +11.4 % (14 → 15.6), network-vlan
  +6.7 %, frozen code view +4.3 %, deployment-cluster +3.0 %.
- With the fixed seed that the engine actually uses, the frozen code view goes from 3369 to 3830 crossings (+13.7 %), and
  the corpus sum from 6650 to 5453 (−18 %).

Cost, instructions per import: code 785 → 802 M (+2 %), workflow 145 → 133 M (−8 %), org chart 115 → 107 M (−7 %). Limit 2
gives 827 M on the code view, so a changed order moves the cost of the later phases by more than the sweep saves; this was
not measured phase by phase.

The lever stays within the limits, and quality improves in sum, so it was adopted for `singlePass`
(`IDLE_SWEEPS_SINGLE_PASS`). It was first rejected because of the frozen code view; that was stricter than the limits of the
plan. The frozen code view remains worse.

After adoption, with the engine's fixed seed: imports and refines of default and `abstractRouting` are byte-identical to the
baseline build in all comparisons, and `singlePass` differs in 6 comparisons, the others being unchanged. Raw files:
[single-pass-idle-20261106.json](single-pass-idle-20261106.json) and
[single-pass-idle-corpus-20261106.json](single-pass-idle-corpus-20261106.json). Frozen views: code 3830 / 1674, 0 hits, 10
delegated edges, 9 strict track pairs; workflow 234 / 448; org chart 31 / 184. Corpus: 5453 crossings / 6361 bends, 95 hits,
6 issues.

## Lever: descendant scan in `widen_planned` (adopted, output-identical)

Every shifted container scanned all graph nodes and walked each one's parent chain. It now reads a children list built
once and walks down from the container (`growth.rs`).

Output: import and refine (threads 1) byte-identical to the baseline build on all 31 datasets, each normalized and
unmodified, in the arms default, `abstractRouting` and `singlePass` (186 comparisons: 133 complete drawings, 53 rejected
with the same error by both builds). The stored default fingerprints were not rerun.

Cost, instructions per import, (T(3) − T(1)) / 2, baseline → now:

| View | shifted containers | nodes walked | instructions |
|---|---:|---:|---:|
| layerweave-code | 16 | 235 | 776 → 773 M (within noise) |
| workflow-dag | 0 | 0 | 145 → 145 M |
| org-chart | 0 | 0 | 115 → 116 M (within noise) |
| synthetic-2k, normalized, all 1269 nodes | 184 | 4776 | 22466 → 21786 M (−3 %) |

## Lever: occupancy check of a route (adopted, output-identical)

Line-level profile (`sample` on a build with debug line tables, after the sweep change; samples without an inner frame
attributed to their call site): the call of `Session::build` in `realize_plan` held 14.7 % of the samples of synthetic-2k
and 2.9 % of the code view, the largest single line on synthetic-2k. Each segment of a route was compared with every
accepted segment. `Occupancy` (`construction.rs`) now keeps the accepted segments by track line, and a check reads the three
neighbouring lines. A conflict needs two coordinates less than `track - EPS` apart, so they cannot be further apart than
that. The vector in acceptance order stays for the failure report, which names the first conflicting route.

Checked: `occupancy_finds_exactly_the_conflicts_of_a_scan_over_all_segments` against the scan (four track widths, boundary
coordinates at and just under a track), and the same 186 comparisons as below against the commit before: all identical.

Instructions per import, before → now: code 791 → 780 M (−1.4 %), workflow 133 → 132 M, org chart 107 → 107 M, synthetic-2k
20909 → 19396 M (−7.2 %). The saving on synthetic-2k is smaller than its share of samples; no time was measured.

## Lever: linear lookups in the demand and the main realization (adopted, output-identical)

The plan was to reuse the demand realization of the channel growth in the main realization. The line-level profile (each
sample charged to the nearest line of our own source) showed linear searches that run in both realizations first:

- all ports per edge end, in `prepare_edge_inner`, `route_from_ports` and the check at the end of the completion: now
  `ports::PortEnds`;
- all channels per section, in `joint::reserve`, `joint::assign` and `continuity::assign`: now a lookup by area and rank;
- the scan of longitudinal pairs in `joint::reserve` compared the edge ids of every pair first: now it compares the numbers
  first.

A lookup keeps the first of several matches, as a scan does. The same 186 comparisons against the commit before: all
identical.

Instructions per import, before → now: code 780 → 754 M (−3.3 %), workflow 132 → 130 M (−1.5 %), org chart 107 → 106 M (−1 %),
synthetic-2k 19390 → 15907 M (−18 %). No time was measured.

### Reuse of the demand realization (not attempted)

After these changes `widen_planned` is 12.5 % of the code view and 11.9 % of synthetic-2k. Of that, `joint::demand` is 4.9 % and
4.8 %, nearly all of it `joint::reserve`. It cannot simply be reused: the bounds and the longitudinal intervals of
`joint::reserve` read coordinates along the flow, and those are what the growth changes. What could be shared is the rest:
edge preparation 0.6–0.7 %, channel slots at most 0.7 % (already memoized when the inputs are equal), and the members map.
That is a few percent at most, for sharing state across a change of geometry. Not worth it now; no experiment was run.

## Lever: `ports::boundaries` (adopted, output-identical)

Every route walked every container and looked up its rectangle by string key. The rectangles are now resolved once, and a
container that the route's bounding box does not reach is skipped: a crossing lies on the route and on the border, and a
circle lies inside its box. The bounding box is one tolerance (1e-6) wider than the route.

Checked: `a_container_out_of_a_routes_reach_has_no_crossing` (20000 routes on a grid that holds the sides of the box and
points beside them, box and circle), and the same 186 comparisons against the commit before: all identical.

Instructions per import, before → now: code 754 → 743 M (−1.5 %), workflow 130 → 129 M, org chart 106 → 104 M, synthetic-2k
15912 → 14871 M (−6.5 %). No time was measured.

## Lever: `abstract_routing::build_with` (not attempted)

`Session::plan` is 7.1 % of the code view, but there is no search to remove: the pairwise comparison in
`abstract_routing::channel` 1.6–2 %, hash maps about 1.8 %, the sections 1.2 %, the rest in small parts.

## Lever: string lookups in `Geometry::rect` and `main_span` (not attempted)

Calls per import:

| View | `rect` | `main_span` | different channels |
|---|---:|---:|---:|
| layerweave-code | 25608 | 538 | 101 |
| workflow-dag | 4446 | 124 | 21 |
| org-chart | 3304 | 330 | 69 |
| synthetic-2k, normalized | 462928 | 9772 | 882 |

`main_span` runs about five times per channel on the frozen views and eleven times on synthetic-2k: in the growth, and once
per realization. Part of this is on changed geometry, so not all of it can be cached; how much was not measured.
`Geometry::rect` is 3.8 % of the code-view import, which limits the gain there.

## End of the block

Revision `a25eb7c`, plus a comment in `lib.rs`.

Default fingerprints: 19 configurations of the sweep (all but `synthetic-20k` and the local export), `bench --reps 1 --threads
1,4`, against a build of `d14db45`, the commit before this work: all identical, and identical for both thread counts in both
builds. The stored `fingerprints-20261021T103351Z.jsonl` was not the reference; the reports before this one also compare with a
build of `HEAD`.

`sh apps/playground/build.sh examples/fixtures/small.json`: exit 0, including the smoke test, the three deployment modes of
`ui-regression.mjs`, and `server_test.py`.

Timing in one process, `node benchmarks/eval/single-pass-timing.mjs --repeat 21`, plain release `bench`, medians of 21, Apple M3,
load 1.5 at the start and 2.1 at the end (few other processes, not idle). Raw:
[single-pass-levers-timing-20261106.json](single-pass-levers-timing-20261106.json).

| View | default | abstract | singlePass import + refine | elk-rs | elkjs |
|---|---:|---:|---:|---:|---:|
| layerweave-code | 1263 ms | 602 ms | 59 + 13 = **72 ms** | 44 ms | 256 ms |
| workflow-dag | 81 ms | 81 ms | 10 + 1 = **11 ms** | 9 ms | 58 ms |
| org-chart | 66 ms | 64 ms | 8 + 1 = **10 ms** | 7 ms | 41 ms |

These are the three frozen views: uniform direction, the simplified task. They say nothing about the views that `singlePass`
rejects. On the code view the output is now 3830 crossings against 3369 before the sweep change and 3057 for elk-rs, so this
timing is not at equal quality.

## Open

- Placement pass with geometry (code 26.5 %, synthetic-2k 23 %, spread over many functions), chain probe.
- String keys and allocation (see the profile): not a listed lever; it would be a larger change.
- Now: code 743 M instructions per import (baseline 784 M, −5 %), workflow 129 M (145 M, −11 %), org chart 104 M (115 M, −10 %),
  synthetic-2k at 1269 nodes 14.9 G (22.5 G, −34 %), about 20 times the code view at 188 nodes.
- The overview of phases and levers in the code that the plan asks for.
