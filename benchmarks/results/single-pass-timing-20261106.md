# Single pass without later correction, and in-process timing, 2026-11-06

## Change

`singlePass` already excluded every first-stage refinement candidate, but the refinement still ran the three
global routing rounds (`Job::Routing`, `Job::RoutingLast`) and the Optimize-time `relieve_crossings` step. Both
re-route the constructed drawing. Under `singlePass` both are now skipped
(`crates/runtime/src/lib.rs`); the refinement only validates and evaluates the input and returns it unchanged
(test `single_pass_refinement_evaluates_no_candidates_for_any_thread_count`: no routing round, same boxes and
routes). The default and `abstractRouting` paths are unchanged.

One correction remains in the import: when the construction leaves routes through nodes (code view 9 conflicts,
org chart 2, from the delegated edges drawn as seed routes), the hierarchical pipeline reroutes only the edges in
conflict, and all conflicts are resolved. Removing it would leave lines through nodes, so it stays until the
delegated edges are constructed.

## Quality, frozen views

`node benchmarks/eval/single-pass.mjs --repeat 3`, raw: [single-pass-once-20261106.json](single-pass-once-20261106.json).
Crossings (including shared-endpoint crossings) / bends / line–node hits.

| View | singlePass before | singlePass now | elk-rs |
|---|---:|---:|---:|
| layerweave-code (188 / 358) | 3373 / 1586 / 0 | **3291 / 1592 / 0** | 3057 / 1638 / 0 |
| workflow-dag (72 / 125) | 240 / 460 / 0 | 240 / 460 / 0 | 278 / 434 / 0 |
| org-chart (104 / 82) | 30 / 190 / 0 | 30 / 190 / 0 | 23 / 148 / 0 |

The global round accepted a reroute on the code view because its conflict key improved, while the drawn crossings
rose. Corpus (31 views, `--corpus --repeat 1`, raw:
[single-pass-corpus-once-20261106.json](single-pass-corpus-once-20261106.json)): the only view whose single-pass
result changed is self-loops (4 → 0 crossings, 42 bends). The `layerweave-code` dataset is generated from the
working tree and changed between the two corpus runs (197 / 338 against 196 / 345 nodes / edges), so its corpus
row is not comparable across runs; all other 30 views are identical in every arm.

## Timing in one process

`node benchmarks/eval/single-pass-timing.mjs --repeat 11`, raw:
[single-pass-timing-20261106.json](single-pass-timing-20261106.json). Apple M3, 8 logical CPUs, macOS, Node
v24.21.0, revision `d14db45` plus uncommitted changes, `bench` built with
`cargo build --release --locked -p layerweave-runtime --bins` (no `benchmark-timings`). Layerweave: `bench`
import and refine, threads 1, median of 11 after one warm-up. elk-rs 0.12.1 (`@openedges/elk-layout`, native
Node addon) and elkjs 0.12.0: `layout()`, median of 11 after one warm-up. No process start or JSON transport in
any arm. The machine was shared: load average 9.0 at the start, 6.0 at the end. Two earlier runs at load 2–8 and
4–24 gave the same values within 5 % for every figure this change does not affect. Treat the figures as diagnostic, not as an idle-machine result.

| View | default import + refine | abstract | singlePass import + refine | elk-rs | elkjs |
|---|---:|---:|---:|---:|---:|
| layerweave-code | 208 + 1290 = 1498 ms | 774 ms | 135 + 21 = **156 ms** | 47 ms | 270 ms |
| workflow-dag | 21 + 85 = 106 ms | 110 ms | 21 + 2 = **23 ms** | 10 ms | 59 ms |
| org-chart | 15 + 72 = 87 ms | 91 ms | 19 + 2 = **22 ms** | 8 ms | 42 ms |

singlePass is 2.3 to 3.3 times slower than elk-rs and 1.7 to 2.6 times faster than elkjs on these three views. Before
this change its refinement took 127 / 8 / 8 ms (code / workflow / org chart).

The public path of `single-pass.mjs` (two `layout-command` processes with JSON) adds the process starts and
transport: code view 187 ms, workflow 35 ms, org chart 32 ms in the same session.

## Where the single-pass import spends its time

macOS `sample` of `bench --threads none` on the code view (6058 samples, inclusive, shares of the import; the run
is sampled, so the shares are approximate):

| Part | Share |
|---|---:|
| placement (`place`), of which the geometry plan 15 %, `align_lanes` 9 %, Brandes–Köpf 2 % | 34 % |
| abstract realization (channel planning, track assignment, chains, growth) | 27 % |
| hierarchical order sweep (crossing counting 9 %) | 22 % |
| conflict reroute of the edges through nodes (see above) | 9 % |
| labels, validation, other | 8 % |

No single part dominates. Closing the gap to elk-rs needs work across placement, realization and ordering, not one
more skipped step.

## Output-identical import work: instruction counts

Wall-clock times on the loaded machine were bimodal for the same binary and input (the code-view import alternated
between about 130 and 400 ms; on the quiet machine below they vary by less than 1 %), so
the before/after comparison uses retired instructions, which repeat within about 0.5 % (hash maps are seeded per process).
Command: `/usr/bin/time -l bench --graph VIEW --limit 1000 --reps N --threads none --pipeline-only 1
--import-options '{"abstractRouting":true,"singlePass":true}'`, read `instructions retired`; one import is
(total at `--reps 5` − total at `--reps 1`) / 4. Apple M3, macOS, revision `d14db45` plus uncommitted changes, plain
`cargo build --release --locked`. Before: the state of the timing table above; after: the changes below.

| View | before (M instructions per import) | after | change |
|---|---:|---:|---:|
| layerweave-code | 1924 | 1447 | −24.8 % |
| workflow-dag | 294 | 232 | −20.9 % |
| org-chart | 269 | 217 | −19.5 % |

Changes, each checked against the stored singlePass fingerprints of all 31 corpus views (identical) and the default
fingerprints (`fingerprints-20261021T103351Z.jsonl`, 20 views, identical):

- hierarchical sweep: crossing count and sweep on reused buffers, a merge join of the two sides' edge places, each
  segment's rank computed once in the Fenwick count, sort keys computed before sorting;
- lane alignment, channel tracks and the lane set-up of the placement: borrowed keys and hash maps for lookups only,
  the plan's port offsets visited directly instead of every graph edge in every area;
- abstract channel planning: hash maps for the lookup-only position, port and slot tables;
- conflict reroute: each route's probe built once (`PreparedRoutes`) instead of once per pair;
- labels and geometry validation: the cheap geometric test first, a set instead of a quadratic scan for unknown
  rectangles.

Indexing the channel lookups of the joint and continuity track assignment was measured too and gave nothing (their
channel lists are short); it was left out. After these changes the sampled profile of the code-view import is flat:
the largest single source line (the Fenwick sum of the crossing count, run after every sweep of eight runs) takes
4 %, every other line 1.5 % or less. Further output-identical work of this kind gains about one percent per change;
larger steps need fewer executions of whole phases (sweep runs, placement passes, the conflict reroute), which changes
the output and has to pass the quality gates instead of the fingerprints.

## Timing on a quiet machine, after the output-identical work

Same harness and build as the first table (`node benchmarks/eval/single-pass-timing.mjs --repeat 21`, raw:
[single-pass-timing-quiet-20261106.json](single-pass-timing-quiet-20261106.json)), after a restart: load average
3.5 at the start and 4.0 at the end (background services after boot, no other user work). Eight separate runs of the
code-view singlePass import before it gave 99.1 to 100.4 ms, so the bimodal times above came from the load. Same
revision `d14db45` plus the uncommitted changes of the instruction-count section. Medians of 21 after one warm-up.

| View | default import + refine | abstract | singlePass import + refine | elk-rs | elkjs |
|---|---:|---:|---:|---:|---:|
| layerweave-code | 1257 ms | 646 ms | 106 + 19 = **124 ms** | 50 ms | 284 ms |
| workflow-dag | 90 ms | 93 ms | 17 + 2 = **19 ms** | 10 ms | 61 ms |
| org-chart | 74 ms | 75 ms | 16 + 2 = **18 ms** | 9 ms | 43 ms |

singlePass is 1.9 to 2.5 times slower than elk-rs and 2.3 to 3.2 times faster than elkjs on these views. The figures
mix two effects against the first table, the quiet machine and the 20–25 % fewer instructions, and do not separate
them.

Public path (`node benchmarks/eval/single-pass.mjs --repeat 5`, two `layout-command` processes with JSON transport,
raw: [single-pass-public-quiet-20261106.json](single-pass-public-quiet-20261106.json), load 3.5): code view 151 ms,
workflow 30 ms, org chart 27 ms, against elk-rs `layout()` 47 / 10 / 8 ms. Process start and transport add about
10 ms per view. Quality is unchanged: code 3291 crossings / 1592 bends, workflow 240 / 460, org chart 30 / 190,
0 line–node hits.
