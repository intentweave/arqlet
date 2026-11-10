# Repair in containers: default-output check and container measurement, 2026-10-25

Machine: Apple M3, macOS 26.0. Base: `b90a5e8` (unchanged HEAD, built in a separate worktree). Change: repair of organic areas
inside containers (`repair_areas`, `fit_containers`) and the 1e-9 inset of the disc test in `Outline::segment_enters`.

## Default outputs

Release `bench` binaries of both revisions, built with `--locked`, same dataset files (generated once; `layerweave-code` is
generated from the repository source, so it is the same file for both), one repetition, refinement threads 1 and 4, the
datasets of `benchmarks/run.sh` except `synthetic-20k`. `python3 benchmarks/fingerprint_compare.py base branch`:

```
summary: identical 19
```

All 19 configurations are identical in `refineFingerprint` and `geometryFingerprint`. Raw runs:
`fingerprint-check-containers-repair-base-20261025.jsonl` and `...-branch-20261025.jsonl`. Scope: these datasets use the
default strategies; the repair acts only on organic areas, which the sweep does not exercise. It shows that the defaults are
unchanged, not that the new path is right (the tests cover that).

## Containers with straight edges

`crates/runtime/tests/containers_straight.rs`, debug build: 10 diagrams (2 containers with 4 leafs each, 4 loose leafs, every
third leaf a circle, 16 straight edges, organic strategy, seeds 1 to 10). Straight edges through foreign nodes, from the
engine's route diagnostic: 81 in 9 of 10 diagrams before (only the root was repaired), 0 after. Sibling overlaps: 0 before and
after except seed 10, which keeps one overlap that the engine reports (`overlap`).

## Default-output check repeated after the performance and circle changes

The branch run was repeated with the final code (incremental scan, repair memo, spread stage, circle default diameter, circle
label and pin in the viewer). `layerweave-code` is generated from the repository source, so its input changed between the
two runs; those three configurations were compared again with a base binary on the same generated file: identical. All other
16 are identical to the base run. In total all 19 configurations are identical in `refineFingerprint` and
`geometryFingerprint`. The branch file `...-branch-20261025.jsonl` holds the final run.

## First computation of the Petri net with shapes

`examples/fixtures/petri-net-shapes.json` (24 nodes, 41 edges, 14 circular places, organic strategy, straight edges), release
`snapshot` binary, one run per line, Apple M3, macOS 26.0, base b90a5e8 plus this change. Wall time of the whole import:

| step | time | geometry issues |
| --- | --- | --- |
| repair as of the container change (places 180 wide, 4 identical repairs) | 13.1 s | 4 |
| scans answered from the cache (a trial that moves one node re-resolves only its own and its peers' ports) | 5.4 s | 4 |
| repeated repair on identical boxes answered from a memo | 1.3 s | 3 |
| finer step sizes of the search (12 instead of 7) | 0.6 s | 2 (`minimum_gap`, from a clearance of 6 against the validator's 12) |
| siblings kept at the validator's gap | 0.6 s | 0 |

The incremental scan is checked against a full scan in every test run (`assert_eq!` on the conflicts and on the bits of the
depth, `repair::tests`). A circle without a measured size is now 72 units across instead of 180.

## Steps of a move gesture on the measured Petri net

`petri_shapes.rs`, `gesture_step_times` (ignored test: `cargo test -p layerweave-runtime --test petri_shapes --release --
--ignored --nocapture`), `petri-net-shapes` with label sizes reported (text width 8 per character plus 24, height 36), release
build, Apple M3, macOS 26.0, three gestures (begin, three updates, finish) on `p0-start`, `lock` and `c1-take`. Each step is a
full recomputation.

| | before | after |
| --- | --- | --- |
| label measurement | 2557 ms | 74 ms |
| gesture steps that recompute | 590 ms to 7325 ms (median about 3200 ms) | 4 ms to 208 ms (median 9 ms) |

Cause: after the routes, the corridor-widening rounds placed the whole drawing again from scratch (stress and repair each
round) although an organic area reads no corridor gaps. They are skipped when every area is organic (`all_areas_organic`); the
warm start of the unedited nodes is kept as well. Default outputs are unaffected (no benchmark dataset is organic in every
area); the 19-configuration check was repeated, see below.

## Mixed layouts and circle containers

Release build, Apple M3, macOS 26.0, after the commit `a5f4d1d` plus this change.

`containers_straight.rs`, ignored test `measure_mixes`: 2 containers with 4 leafs each and 4 loose leafs, 16 random straight
edges, seeds 1 to 6 (the generator is the one of the container measurement above), three mixes of area layouts.

| mix | first layout | sibling overlaps and children outside their container | straight edges through nodes (all reported) |
| --- | --- | --- | --- |
| organic root, layered containers (lr, tb) | 0.17 to 2.7 s | 0 | 0 to 6 |
| organic root and container, one layered container | 0.07 to 2.6 s | 0 | 0 to 1 |
| layered root, organic containers | 2.0 to 2.2 s | 0 | 1 to 13 |

In a layered root the nodes cannot move for a straight edge, so crossings of the random edges remain there; the engine
reports every one (`straight-through-node`). Before the changes of this commit this mix produced overlaps between
containers and loose nodes (up to 9 pairs per drawing, unreported containers of 1e16 units after repeated growth) in
up to 62 s.

`mixed-layouts` (`examples/fixtures/mixed-layouts.json`: 18 nodes, 16 straight edges; organic root and a circle container,
a left-to-right and a top-to-bottom container): first layout 42 ms, no geometry or routing issue. Circle container 661 units
across for five children (two circles, three rectangles). Dragging one child outward in the browser (headless Chrome, six
pointer moves of 72 x 36 screen pixels) grows the disc by about 170 per drop (661, 832, 1000, 1169): the top-left corner
of the box stays, so a child dropped away from the centre needs a larger diameter. The playground keeps a grown size as a
minimum for a rectangle; for a circle container it does not (the diameter is always the content's), so the disc shrinks again when the child returns (661, 1286, 661 in headless Chrome).

## Default-output check after the mixed-layout and circle-container changes

Repeated with the final code (including the graph serialization that keeps an explicit container strategy). 16 configurations
identical to the base run; the 3 `layerweave-code` configurations (the dataset is generated from the repository source, so its
input differs from the first base run) compared again with a base binary of `b90a5e8` on the same generated file: identical.
All 19 configurations are identical in `refineFingerprint` and `geometryFingerprint` at 1 and 4 threads.

## Drops without a pin in a circle container, and border crossings

`circle_containers.rs`, `a_released_drop_in_a_circle_container_stays_where_it_was_dropped` (release build): four drops of a
child of the `mesh` circle container by 40 to 90 units, each finished without a pin. Distance of the node from where it was
dropped: 0 to 1 units; other children that moved: 0. Before: 66 to 108 units and all four other children moved (the block of
children was centred again in the disc), and where the layout still ran its corridor-widening rounds (a mixed graph with
only straight edges) the whole container was placed again from scratch. The rounds are skipped when every edge can only be
drawn straight (`all_edges_straight`). The disc now scales about its old centre in a continuation of an organic area.

Border crossings (the derived ports where a route meets a container's border), headless Chrome on `mixed-layouts`: 7 markers,
each on the drawn route of its edge (0 units off), and the 5 at the circle container on the circle (0 units off). Before: 11
markers, up to 130 units off their routes, because they were derived from the orthogonal routes that the straight segments
replace.

## Default-output check against origin/main with the routing styles (2edb00f), rustc 1.99.0

Release `bench` binaries of `origin/main` (2edb00f, with the routing styles roundedOrthogonal, polyline and roundedPolyline) and of
this branch after merging it, both built with rustc 1.99.0 (b940084d7) and `--locked`, the same generated dataset files, one
repetition, refinement threads 1 and 4, the datasets of `benchmarks/run.sh` except `synthetic-20k`, Apple M3, macOS 26.0:
`python3 benchmarks/fingerprint_compare.py main branch` gives `summary: identical 19`. The Rust tests (`cargo test --workspace
--locked --no-fail-fast`), `sh apps/playground/build.sh examples/fixtures/small.json` and the 94 JS tests pass.
