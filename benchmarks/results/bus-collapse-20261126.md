# Collapsing one-bus containers in a bus view (#150), 2026-11-26

Machine: Apple M3, 8 cores, 16 GB, macOS (Darwin 25.0.0), rustc 1.99.0. Other builds ran on the machine; the load is
given with each measurement. `main`: 70d744b. Branch: that plus the change below. Build:
`cargo build --release --locked -p arqlet-runtime --bin layout-command`.

Input: `benchmarks/experiments/bus-collapse.mjs`, a public generated view with buses as data and containers. CONTAINERS
areas of UNITS units on BUSES buses; every area has one home bus, every GATEWAY-th area also sits on a second one. The
collapsed view is the same import with `levelOfDetail: {kinds: ["single"]}`: every one-bus area collapsed.

## Cause

Measured first on daca8b0 (the same layout steps as `main` here), organic, `busStyle: "lines"`. Phase times in ms,
median run of 5, from a build with [`bus-collapse-phases.patch`](../experiments/bus-collapse-phases.patch) (load 19 to
33; [rows](bus-collapse-phases-20261126.jsonl)). "First layout": placement and first routing of the graph with the bus
nodes. "Conflicts": routes through boxes after it. "Bars": drawing the bars and their taps (`bus_line::draw`). "Roots":
finding the root of every bus network, done twice. The engine's `routing` phase does not include the second pass.

| Case | View | Total | First layout | Conflicts | Second routing pass | Bars | Roots | engine `routing` |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 40 / 5 / 3 / 2 | open | 205 | 62 | 564 | not run | 117 | 41 | 24 |
| 40 / 5 / 3 / 2 | collapsed | 729 | 84 | 43 | 556 | 76 | 19 | 58 |
| 48 / 3 / 3 / 2 | open | 137 | 55 | 583 | not run | 66 | 23 | 24 |
| 48 / 3 / 3 / 2 | collapsed | 475 | 70 | 308 | 340 | 56 | 13 | 43 |
| 48 / 5 / 4 / 2 | open | 307 | 98 | 887 | not run | 160 | 83 | 35 |
| 48 / 5 / 4 / 2 | collapsed | 1103 | 116 | 46 | 824 | 144 | 28 | 83 |
| 64 / 3 / 3 / 3 | open | 279 | 92 | 873 | not run | 157 | 46 | 40 |
| 64 / 3 / 3 / 3 | collapsed | 1096 | 91 | 1 | 912 | 81 | 15 | 56 |
| 64 / 3 / 4 / 2 | open | 223 | 67 | 0 | not run | 130 | 42 | 17 |
| 64 / 3 / 4 / 2 | collapsed | 940 | 120 | 31 | 677 | 128 | 23 | 78 |
| 64 / 3 / 5 / 3 | open | 178 | 73 | 781 | not run | 78 | 43 | 23 |
| 64 / 3 / 5 / 3 | collapsed | 987 | 90 | 2 | 813 | 72 | 17 | 55 |

The phase that grows is the second routing pass (`compute_counted_inner`: routes that share a segment or run through a box
are routed again, and the result is kept if fewer run through a box). It runs only for 1 to 500 routes through boxes: the
open views here have none or more, the collapsed ones 1 to 308. In these views every edge is a bus tap. In an organic
area a tap's route is never drawn: the lines are made from bars, and bars from the boxes. The collapsed views have no
other edges, so no edges are aggregated, and lifting the taps costs little.

## The change

The pass is left out only where all of this holds (`compute_counted_buses`, `undrawn_routes`):

- every bus of the view becomes lines of its own, so no bar is left. Later operations restore the kept tap routes
  (`busTaps`) only for bars (`bus_line::expand_snapshot`), so nothing reads them;
- every edge the layout sees is a bus tap;
- the engine choice cannot run behind the pass (single pass is used, or the work estimate is too high). It judges the
  drawings with the tap routes in them and can replace the drawing;
- the drawing is not compared with another one: `directionsFirst`, region ports, held memberships, and the layouts that
  refinement candidates and the composition operations compute (`compute_compared`);
- nothing else is derived from the routes: no edge or port label, no unmet `minPortSpacing` to report, only orthogonal
  routing, no shared tracks.

## Import time

Median of 5 runs, ms; the two builds in turn, load 6 to 15 ([rows](bus-collapse-20261126.jsonl)).
`sh benchmarks/experiments/bus-collapse.sh OUT MAIN BRANCH --reps 5`. "Boxes": nodes open and collapsed. In brackets:
collapsed against open.

| Containers / units / buses / gateway, view | Boxes | `main` open | `main` collapsed | branch open | branch collapsed |
| --- | --- | --- | --- | --- | --- |
| 40 / 5 / 3 / 2 organic/lines | 240, 140 | 212 | 754 (3.6×) | 227 | 191 (0.84×) |
| 48 / 3 / 3 / 2 organic/lines | 192, 120 | 148 | 517 (3.5×) | 150 | 150 (1.00×) |
| 48 / 5 / 4 / 2 organic/lines | 288, 168 | 338 | 1220 (3.6×) | 318 | 292 (0.92×) |
| 64 / 3 / 3 / 3 organic/lines | 256, 130 | 288 | 1128 (3.9×) | 306 | 203 (0.66×) |
| 64 / 3 / 4 / 2 organic/lines | 256, 160 | 240 | 1023 (4.3×) | 237 | 279 (**1.18×**) |
| 64 / 3 / 5 / 3 organic/lines | 256, 130 | 190 | 1059 (5.6×) | 184 | 181 (0.98×) |
| 40 / 5 / 3 / 2 organic/bar | 240, 140 | 316 | 858 (2.7×) | 331 | 880 (**2.66×**) |
| 48 / 3 / 3 / 2 organic/bar | 192, 120 | 210 | 570 (2.7×) | 209 | 573 (**2.74×**) |
| 48 / 5 / 4 / 2 organic/bar | 288, 168 | 470 | 1352 (2.9×) | 438 | 1273 (**2.91×**) |
| 64 / 3 / 3 / 3 organic/bar | 256, 130 | 440 | 1260 (2.9×) | 449 | 1273 (**2.83×**) |
| 64 / 3 / 4 / 2 organic/bar | 256, 160 | 342 | 1092 (3.2×) | 349 | 1121 (**3.21×**) |
| 64 / 3 / 5 / 3 organic/bar | 256, 130 | 244 | 1091 (4.5×) | 306 | 1198 (**3.91×**) |
| 40 / 6 / 5 / 8 organic/lines | 280, 70 | 586 | 69 | 171 | 27 |
| 80 / 6 / 10 / 8 organic/lines | 560, 140 | 868 | 298 | 913 | 102 |

The expectation of #150, a collapsed view that costs no more than the open one, holds for five of the six slow organic
lines views (`48 / 3 / 3 / 2` is level: 150.1 against 149.8 ms). It does not hold for `64 / 3 / 4 / 2` (18 % more; in the
phase table its first layout is the slower one) nor for any of the views drawn with bars, which are as slow as before.
The other default cases (20, 40 and 80 areas; layered lines, trunk and bar) were already faster collapsed than open and
are unchanged within the noise. All 48 pairs of rows have the same boxes, routes and bus lines (equal hashes).

## Same drawings as `main`

`sh benchmarks/experiments/bus-collapse-compare.sh MAIN BRANCH OUT` runs a chain of operations on both builds, each step
from the state the step before left in that build, stateless and as a session, and compares the answers whole except
`busTaps` (its roots compared) and `recomputedContainers`. Operations: import, scene, intent, move (kept and released),
pin, unpin, configure (appearance, `organicGap`, bar and back), collapse and expand, move after a toggle, reveal,
measure, shape, direction, settle, filter, patch (add and remove nodes), restore, compact bookmark and its reading,
trace, neighbourhood, refine (1 and 4 threads, up to 24 areas) with a move and a toggle after it, add, scene. Views: the
six styles open and collapsed at 8 to 24 areas, larger lines and trunk views, the six slow cases collapsed (stateless,
two also as a session), and lines views with options and graphs that keep the pass or sit beside it (labels, port
spacing, other engines and options, directions first, other routing styles, ordinary edges, organic areas in a layered
level).

247 runs, 11 379 steps ([rows](bus-collapse-compare-20261126.jsonl)): the answers are the same in all but two, which are
one step of one view in its stateless and its session run (the `layoutDiagnostics` line below). Boxes, routes, bus lines
and scenes never differ. In 45 runs `busTaps` and `recomputedContainers` differ at some step, so the pass was left out
there; with labels, `minPortSpacing`, `hierarchicalOrdering: false`, directions first, straight or bus routing and
ordinary edges, both stay the same.

94 steps are refused or fail, with the same answer from both builds: 12 restores of a view whose collapsed container the
patch had emptied ("Not a container"); the rest in the layered top level with organic containers, where both panic when
the other bus style is configured (below; a session ends with it, so its later steps fail too), and in one session with
straight routing at 24 areas, where both stopped answering at the same step. Straight routing was therefore compared at 8
areas (collapsed); the grid script leaves out the larger runs.

A snapshot made by one build and continued by the other gave the same drawing as when one build did both (56 operations
on lines and bar views of 16 to 32 areas).

## Snapshot fields that can differ

- `busTaps.routes`, `.resolvedPorts`, `.boundaryCrossings`: the taps' routes as first routed. Anyone who reads the
  snapshot JSON sees them (the contract type `BusTapLayout`); sessions hold them and an exported or stored snapshot
  carries them. In the crates only `bus_line.rs` reads them, and only for bars. `roots` is the same.
- `recomputedContainers`: the second pass empties it where its result is kept. The playground's status line shows its
  count.
- `layoutDiagnostics`: where the second pass on `main` leaves no route through a box, `main` omits the line "Engine choice
  not tried: …"; without the pass the line is there. It shows in the diagnostics of the snapshot and wherever a host
  lists them. Seen at one step of one of the compared views (a pin). Nothing reads the line.

## Default output and determinism

- The 15 views of `fingerprints-20261124-default-0.9.0.jsonl` (threads 1 and 4; `layerweave-code` generated from the
  v0.9.0 source, 1362/2673) are identical.
- `tools/platform-parity.mjs`: 19 of 19 requests identical, native (macOS arm64) against WASM. It has no bus view, so the
  same comparison was run on generated bus views (import, toggle, move, configure, and refinement up to 24 areas): all
  identical, and refinement with 1 and 4 threads identical wherever it ran.

## Still slow, and found on the way

- Views with bars (`busStyle: "bar"`, or any bus left as a bar, such as a bus with one tap) pay for the pass as before:
  later operations read their tap routes, so leaving it out would need new snapshot state.
- Outside the pass, open views spend most of their time on the bars (`bus_line::draw`) and the roots
  (`bus_line::root_of`, quadratic, run twice). The first routing also routes every tap, although no such route is drawn.
  The phase timings (`timings: true`) have no phase for the second pass, the bars or the roots.
- `main` panics ("no entry found for key", `crates/layout/src/lib.rs`, `absolute_with`) when a view with a layered top
  level, organic containers and `busStyle: "bar"` is imported or configured so. Not changed here.
