# Collapsing one-bus containers in a bus view (#150), 2026-11-26

Machine: Apple M3, 8 cores, 16 GB, macOS (Darwin 25.0.0), rustc 1.99.0. Other builds ran on the machine: the `main` run
had a one-minute load of 6 to 9, the branch runs of 10 to 33, so single times are noisy. The rows with the option off
agree with the `main` rows (medians within 4 % on the six slow cases). Build:
`cargo build --release --locked -p arqlet-runtime --bin layout-command`. Revisions: `main` at daca8b0; the branch is that
plus `skipTapReroute`.

Input: `benchmarks/experiments/bus-collapse.mjs`, a public generated view with buses as data and containers. CONTAINERS
areas of UNITS units on BUSES buses; every area has one home bus, every GATEWAY-th area also sits on a second one. The
collapsed view is the same import with `levelOfDetail: {kinds: ["single"]}`: every one-bus area collapsed.

```sh
sh benchmarks/experiments/bus-collapse.sh BINARY bus-collapse-20261126.jsonl LABEL --reps 5            # main
sh benchmarks/experiments/bus-collapse.sh BINARY bus-collapse-20261126.jsonl LABEL --reps 5 --skip 1 --ops move  # branch
```

## Reproduction and effect of the option

Organic, `busStyle: "lines"`. Total time of the import in ms, median of 5 runs
([rows](bus-collapse-20261126.jsonl)). "Boxes": nodes of the view, open and collapsed.

| Containers / units / buses / gateway | Boxes | main: open | main: collapsed | option off: open | option off: collapsed | `skipTapReroute`: open | `skipTapReroute`: collapsed |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 40 / 5 / 3 / 2 | 240, 140 | 212 | 751 (3.5×) | 206 | 727 | 205 | 174 |
| 48 / 3 / 3 / 2 | 192, 120 | 142 | 488 (3.4×) | 137 | 471 | 137 | 135 |
| 48 / 5 / 4 / 2 | 288, 168 | 318 | 1135 (3.6×) | 309 | 1104 | 308 | 279 |
| 64 / 3 / 3 / 3 | 256, 130 | 288 | 1129 (3.9×) | 279 | 1089 | 278 | 184 |
| 64 / 3 / 4 / 2 | 256, 160 | 232 | 970 (4.2×) | 222 | 934 | 223 | 262 |
| 64 / 3 / 5 / 3 | 256, 130 | 181 | 993 (5.5×) | 179 | 992 | 178 | 178 |

These cases were searched for among generated parameters. With the script's defaults (6 units, every 8th area on two
buses; 20, 40 and 80 areas) the collapsed view was faster than the open one in every style on `main`, so the effect needs
a view as described below. With the option, one slow case stays 17 % above its open view (64 / 3 / 4 / 2); the others are
at or below it.

## Cause

Phase times in ms, median run of 5, option off, from a build with
[`bus-collapse-phases.patch`](../experiments/bus-collapse-phases.patch) (load 19 to 33;
[rows](bus-collapse-phases-20261126.jsonl)). "First layout": placement and first routing of the graph with the bus nodes.
"Conflicts": routes through boxes after it. "Bars": drawing the bars and their taps (`bus_line::draw`). "Roots": finding
the root of every bus network, done twice. The engine's `routing` phase is shown for comparison: it does not include the
second routing pass.

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

The phase that grows is the second routing pass (`compute_counted_inner`: under `hierarchicalOrdering`, routes that share
a segment or run through a box are routed again, and the result is kept if fewer run through a box).

- It runs only for 1 to 500 routes through boxes (`MAX_REROUTED_ROUTES`). The open views here have none, or more than 500;
  the collapsed ones have 1 to 308. One conflict is enough for the whole pass, because every route that shares a segment
  moves, and taps of one bus share segments throughout.
- In these views every edge is a bus tap, and in an organic area (or with `busStyle: "bar"`) a tap's route is never drawn:
  the bar and its taps are drawn from the boxes (`bus_line::collapse`), which also drops the conflicts found on tap routes.
  In all six slow cases every conflict before and after the pass was on a tap route. In 64 / 3 / 3 / 3 the pass took
  912 ms for one conflict that remained.
- The routes are kept in the snapshot (`busTaps`) for later operations, so leaving the pass out is not byte-identical.

Lifting the taps to the collapsed containers itself costs little: the collapsed views have fewer taps, and their first
layout takes 70 to 120 ms. There are no other edges, so no edges are aggregated.

## The option and its quality

`skipTapReroute` (off) leaves the pass out where every edge of the graph the layout sees is a bus tap.

Drawing, compared field by field over 288 pairs (12 parameter sets, 4 sizes of 8 to 24 areas, `organic/lines`,
`organic/bar`, `layered/bar`, open and collapsed; `--ops move,refine`) and the 36 pairs of the timing run (import and a
move):

- **Import:** in all pairs the boxes, routes and bus lines are the same (equal hashes). Otherwise only `busTaps` and
  `recomputedContainers` differ (22 of the 288 pairs, and some of the timing run's).
- **Move** (the first top-level node by 400): 3 of the 288 pairs draw differently.
- **Refinement** after the move: 6 of the 282 pairs that refine draw differently. The other 6 fail with and without the
  option (below).

The 9 differing drawings ([rows](bus-collapse-drawing-20261126.jsonl),
`sh benchmarks/experiments/bus-collapse-drawing.sh BINARY OUT`; hits: lines through a box that is not their own):

| Case, view, operation | Without: size, bends, length, hits, issues | With: size, bends, length, hits, issues |
| --- | --- | --- |
| 16/2/3/2 organic/lines open, move | 2615×869, 134, 33051, 0, 0 | the same values |
| 16/2/3/2 organic/lines open, refine | 2615×869, 151, 31215, 0, 2 | the same values |
| 12/3/4/3 organic/lines collapsed, move | 2735×1022, 114, 26549, 0, 0 | 2735×1022, 152, 26671, **3**, 0 |
| 8/3/3/3 organic/bar collapsed, move | 2134×992, 9, 4456, 0, 0 | 1513×965, 13, 6039, 0, 0 |
| 24/3/3/2 organic/bar collapsed, refine | 1782×2449, 93, 36147, 37, 1 | 3226×1564, 91, 45587, **0**, 1 |
| 24/3/3/3 organic/bar open, refine | 3051×3144, 126, 50907, 0, 1 | 3610×2826, 105, 50629, 0, 1 |
| 16/5/4/2 organic/bar collapsed, refine | 2408×1589, 81, 36549, 0, 1 | 2799×2148, 50, 25725, 0, 1 |
| 12/5/4/3 layered/bar collapsed, refine | 1216×1504, 14, 11259, 0, 0 | 1216×1492, 12, 11100, 0, 1 |
| 16/5/4/3 layered/bar collapsed, refine | 1150×2266, 5, 12239, 0, 0 | 1150×2326, 4, 12444, 0, 0 |

The view of the third row also carries routes without an edge after the move, with and without the option (below).

So a later move or refinement can come out better or worse. There are two causes: in a view with bars, later operations
read the kept tap routes (`bus_line::expand_snapshot`); and the later layout leaves the pass out too, together with the
engine choice behind it (below), which can replace the drawing. That is why the option is off: whether it becomes the
default is open.

## Deferring the pass to the first later operation (considered, not built)

The idea: leave the pass out at an import and run it before the first operation that needs the repaired tap routes, so
that every drawing stays as on `main`. Probes with a temporary build (not committed) showed what stands against it:

- **The pass cannot simply run at every later operation.** Applied to its own result, it left fewer routes through boxes
  again in two of the six slow cases (`48 / 3 / 3 / 2`: 308, then 220, then 218; `48 / 5 / 4 / 2`: 46, then 23, then 22),
  so by its own rule that result would be taken too. A later operation therefore has to know whether the pass is still
  owed.
- **The snapshot does not say so.** `baseRevision` is absent only on an untouched import; a toggle (which lays out from
  scratch, like an import), a refinement and an appearance change all set it. Using it would cover imports only, and would
  make an existing field carry a new meaning. A general solution needs new snapshot state.
- **The drawing can depend on the tap routes at once.** Where single pass is not used (as in these organic views) and the
  work estimate allows it, the engine choice runs after the pass, judges the drawings with the tap routes in them, and can
  replace the drawing: in `16 / 2 / 3 / 2` open, a move on `main` takes the other engine's drawing (box coordinates differ
  in the last bit, 4 routes differ). Such a layout has to run the pass at once.
- **Not every view reads the kept routes.** Where every bus is drawn as lines (no bar left), `expand_snapshot` restores
  nothing, so no later operation reads `busTaps.routes`. All six slow collapsed views are of this kind, and their work
  estimate rules the engine choice out. Leaving the pass out there needs no deferral and no new state. This was read from
  the code and not compared over the operations; it also depends on what `expand_snapshot` restores.

## Default output

With the option off the 15 views of `fingerprints-20261124-default-0.9.0.jsonl` (threads 1 and 4; `layerweave-code`
generated from the v0.9.0 source, 1362/2673) are identical, and the 36 bus views of the timing run have the same geometry
as on `main`.

## Found on the way, not changed

- **`main` panics** on `refine` of a view whose buses are partly lines and partly a bar (here: a bus with one tap after
  collapsing): `crates/routing/src/lib.rs:1664`, no entry found for key. The refinement restores the kept routes of all
  taps (`bus_line::expand_snapshot`), also of those whose edge the view no longer has (the head of a bus drawn as
  lines). After a move such a view also carries routes without an edge. Reproduce:
  `node benchmarks/experiments/bus-collapse.mjs --sizes 12 --units 2 --buses 4 --gateway 3 --views organic/lines
  --reps 1 --skip 1 --ops move,refine`.
- Outside the second pass, the open views spend most of their time on the bars (`bus_line::draw`; in a sample, string
  comparisons in the clearance tests are a large part of it) and on the roots (`bus_line::root_of` searches from every
  member, and runs twice per layout). Both could be sped up with the same output; neither was changed.
- The first routing also routes every tap, although no such route is drawn.
- The engine's phase timings (`timings: true`) have no phase for the second routing pass, the bars or the roots; their
  time shows only in `layout`.
