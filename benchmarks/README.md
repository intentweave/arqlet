# Benchmarks

Reproducible latency measurements for Arqlet's native engine. Every number reported
anywhere in this repository must come from a results file in `benchmarks/results/`, with the
machine and revision recorded.

## Run

```sh
sh benchmarks/run.sh                       # full sweep, writes benchmarks/results/sweep-<UTC>.jsonl
python3 benchmarks/summarize.py FILE.jsonl # Markdown table with machine line
TIMEOUT=900 sh benchmarks/run.sh           # longer per-run budget
```

`run.sh` builds `bench` in release mode with the native-only `benchmark-timings` feature,
regenerates the datasets, and records one JSON line per configuration. A run that exceeds `TIMEOUT`
(default 600 s) is recorded as timed out, so one slow configuration cannot stall the sweep. Measure
on an otherwise idle machine. Close heavy
applications, and do not run other builds while the sweep runs.

## What is measured

`crates/runtime/src/bin/bench.rs` times, for one dataset and view limit:

- **import**: `import_graph` (view selection, projection, and full layout of the bounded view). The
  input clone is part of each sample, so the figure is an upper bound on import itself.
- **refine**: `refine_snapshot_job` at each requested thread count (the same operation as the
  backend's `refine`). Each thread count is checked against the others by FNV-1a fingerprint of the
  serialized result. A mismatch aborts the run, because the thread count must not change output.
- **edit**: one `add_node` on the refined snapshot.
- **size**: serialized snapshot bytes; view size; the optimization report (before/after quality).
- **work**: deterministic work counters and cumulative stage times for placement, organic stress,
  separation, geometric repair, port resolution, routing, corridor-gap calculations and scoring.
  Stage times are enabled only by `benchmark-timings`; otherwise they are zero. The pure WASM core
  never reads a host clock. Timers are nested and can overlap; in parallel runs they sum candidate
  CPU time, not wall time.

Each timing has one unmeasured warm-up run, then `reps` measured runs. The figures are medians with
min and max. The sample count is recorded, so variance is visible.

## Datasets

`generate.py` writes `benchmarks/datasets/*.json` (git-ignored). The hashes, sizes and sources
are tracked in `benchmarks/manifest.json`. Generation is deterministic: two runs produce
byte-identical files (verified by SHA-256). The one exception is `layerweave-code`, which reflects
the repository state recorded in the manifest's `gitRevision`.

| dataset | source | what it stresses |
|---|---|---|
| `cloud-architecture` | hand-authored | nested regions, zones, subnets; parallel edges; retry cycle |
| `activity-order` | hand-authored | swimlanes as containers; forks, joins, decisions; rework loops |
| `deployment-cluster` | hand-authored | namespaces, deployments, pods; fan-in scrapes; cycles |
| `state-order` | hand-authored | composite state; self-loop; parallel transitions; cycles |
| `state-tcp` | hand-authored | protocol state machine, dense cycles, no containers |
| `workflow-dag` | seeded (7) | staged DAG; skip edges; retry loops |
| `arqlet-pipeline` | hand-authored | the engine's own layout and operation pipeline, feedback loops |
| `layerweave-code` | repository | crates, modules, functions; heuristic call graph of this repository |
| `org-chart` | seeded (5) | company hierarchy; reporting edges; dotted-line matrix edges |
| `uml-class` | hand-authored | packages; attributes as children; composition, inheritance, bidirectional associations |
| `er-diagram` | hand-authored | relationships as nodes with cardinality edges; a self-relation |
| `bpmn-pools` | hand-authored | pools and lanes; message flows; boundary timer; retry loop |
| `network-vlan` | seeded (9) | redundant core; VLAN groups; parallel links; allow and deny rules |
| `petri-net` | hand-authored | bipartite places and transitions; lock, buffer, inhibitor arc |
| `synthetic-2k` | seeded (11) | random hierarchy; locality; cycles; self-loops; parallel; unresolved edges |
| `synthetic-20k` | seeded (12) | scale probe for import and edit, 20,000 leaves; refinement not run |
| `organic-25` … `organic-300` | seeded (`20261027 + size`) | connected sparse organic graphs for the placement/repair size ladder |
| `tree-25` … `tree-300` | deterministic | balanced binary trees for the tree-placement size ladder |
| `intentweave-export` | local export | optional; included only if `exports/intentweave-initial/graph.json` exists |

`layerweave-code` extracts functions with a simple brace matcher and records `name(` calls to
functions defined in the repository (same file first, then a unique definition). It is a
heuristic, not a compiler-accurate call graph. It skips `#[cfg(test)]` sections.


## Measured environment

Results are only comparable within one machine and build. Each results file records the CPU model,
logical CPU count, OS, architecture, `rustc` version, profile, and git revision with a dirty flag.
Current results: `benchmarks/results/sweep-20261020T152137Z.jsonl` (Apple M3, 8 logical CPUs, release build, git b373802 with uncommitted changes). Regenerate the table with `summarize.py`.

## Profiling a bottleneck

Start with the per-stage times and work counters in each `bench` record. Keep the same input,
view limit, options and thread count when comparing runs; run on an otherwise idle machine.
For an organic size step:

```sh
python3 benchmarks/generate.py
cargo build --release -p arqlet-runtime --bin bench --features benchmark-timings --locked
./target/release/bench --graph benchmarks/datasets/organic-100.json --limit 1000 --reps 3 --threads 1
```

For a sampled call stack, run the benchmark in one terminal and sample its numeric process ID in
another. On macOS:

```sh
sample <PID> 10 -file /tmp/arqlet.sample
```

On Linux, capture the whole command:

```sh
perf record --call-graph dwarf -g -- ./target/release/bench --graph benchmarks/datasets/organic-100.json --limit 1000 --reps 1 --threads 1
perf report
```

For paired refinement-only experiments, `--refine-options JSON` applies options to the
already-imported snapshot without recomputing its layout. This isolates candidate behavior from
input-layout differences; it is intended for benchmarking, not as a substitute for reconfiguring a
real snapshot through the runtime.

`--import-options JSON` gives the import the options it lays out with, as the import operation does. Options
that change the first layout or its size bound (such as `sparseStress`) need it; `--options` only reconfigures the
imported snapshot.

Sampling is diagnostic, not a timing result. Do not commit profiler output or temporary benchmark
files; store only reproducible results and their machine/revision metadata in `benchmarks/results/`.

## Known limits of these numbers

- The thread count is the only parallel dimension. Only the four ordering candidates and the
  routing-only candidate run in parallel, so at most five jobs can overlap.
- Refinement is whole-view. Larger views are slower by a steep margin (see the scaling rows in the
  results). This is the measured baseline that the next optimization steps must improve.
- These are single-process, single-user measurements on one laptop. They say nothing about the
  browser, WASM, or a server under load.
- 100K-node graphs are not measured: view import and refinement are bounded by design, and
  refinement refuses views above 1,000 nodes.

## Optimization round 1 (same machine, before and after)

Before: `sweep-20261020T153150Z.jsonl`. After: `sweep-after-profiling-20261020T154914Z.jsonl`. Both are
single-thread refinement medians from the same configuration. Changes: one pass for absolute
coordinates instead of a per-call node map; references instead of route copies in the routing search;
lexicographic early exit in candidate scoring (the chosen route is identical); bounding-box prefilter
for segment tests (conservative, so no interaction is skipped).

Output was checked against the stored refinement fingerprints (`refineFingerprint`) for every
configuration whose input did not change. All of them are byte-identical.

| configuration | refine, before (ms) | refine, after (ms) | speedup | import, before (ms) | import, after (ms) |
|---|---|---|---|---|---|
| workflow-dag@100 | 1710 | 181 | 9.5× | 5.2 | 1.1 |
| org-chart@100 | 823 | 133 | 6.2× | 12.4 | 2.1 |
| synthetic-2k@100 | 6263 | 990 | 6.3× | 71.9 | 12.6 |
| intentweave-export@100 | 9232 | 1526 | 6.1× | 18.0 | 9.1 |
| synthetic-20k@500 (import only) | not measured | not measured | | 8176 | 362 |

`layerweave-code` is not in this table as a speedup. It is generated from this repository's own
source, and the source changed with the optimization, so the input changed too. Its outputs are
verified separately against the earlier generated file.

Remaining cost is still superlinear in view size, mostly in the routing candidate search. Measure a
new change against the same sweep before claiming a further speedup.

## Engine comparison (evaluation)

`benchmarks/eval/compare.mjs` runs Arqlet, elkjs and dagre on the same view and scores every
output with one metric implementation. Results and caveats: [`docs/comparison.md`](../docs/comparison.md).
The raw results are `results/eval-*.jsonl`, and the generated tables are `results/eval-*.md`.

## Optimization round 2 (routing)

Before: `sweep-after-profiling-*.jsonl`. After: `sweep-routing-*.jsonl` (same machine, single thread).
Changes: route bounding boxes computed once per candidate and per earlier route; parent map built once per
routing pass. Output checked against the stored fingerprints on all configurations whose input did not change.

| configuration | refine, before (ms) | refine, after (ms) | speedup |
|---|---|---|---|
| synthetic-2k@100 | 990 | 615 | 1.6× |
| intentweave-export@100 | 1526 | 1060 | 1.4× |
| workflow-dag@100 | 181 | 108 | 1.7× |
| org-chart@100 | 133 | 79 | 1.7× |
| state-order@100 | 7.4 | 3.4 | 2.2× |

`layerweave-code` changed input (its source changed), so it is excluded from the comparison.

## Option grid (strategy analysis)

`benchmarks/eval/grid.mjs` measures configuration variants on the same views and writes
`results/grid-*.jsonl`; `grid_report.py` summarizes them against the default. The analysis and the open
decision are in [`docs/configuration.md`](../docs/configuration.md).

## Profile runner and shared metrics

Two parts keep comparisons meaningful. Measurement and optimization priorities are separate: the
engine's objective list (`docs/configuration.md`) decides what is improved; the measurement below
decides what is reported, and it is the same for every engine.

**Metrics** (`crates/scene/src/metrics.rs`, reached through the `metrics` operation of `layout-command`).
Every metric is computed on every call, so no value is ever "not computed":

| criterion | measured as |
|---|---|
| sibling overlaps | node pairs under the same parent whose rectangles overlap |
| cross-branch overlaps | overlapping node pairs under different parents (containers excluded when nested) |
| line–node hits | (edge, leaf node) pairs where a segment passes through the node interior; includes the edge's own endpoints; containers excluded |
| shared lines | edge pairs with an unintended collinear overlap; overlap within 20 units of a shared port is intended |
| border hugging | (edge, foreign leaf) pairs with a segment within 12 units of the border without entering; length is the summed length inside that band |
| crossings | proper crossings between edges without a common endpoint node; crossings at a common endpoint are reported separately |
| touches | a segment endpoint lying on another edge's segment without crossing it |
| bends | direction changes after removing repeated and collinear points |
| length | total, mean and maximum edge length |

Foreign engines are scored with the same operation: the harness passes absolute node rectangles (with
parent ids) and edge polylines. `benchmarks/eval/lib.mjs` no longer contains its own metric code.

**Metrics version 3** (`crates/scene/src/metrics.rs`; diagnostic fixtures in `crates/scene/fixtures/`):

- Shared lines: an overlap counts unless both routes name the same explicit port (`sourcePort`/`targetPort`) or
  bundle at that end and it lies on the stub, the first or last segment of the normalized polyline. Overlaps are
  merged per line before they are measured. The former global 20-unit exemption is gone.
- Near-parallel tracks: parallel segments closer than `trackSpacing`, reported apart from overlaps.
- Container traversal: a segment through the interior of a container the edge does not belong to (rule 6); container
  border hugging is reported apart from node hugging.
- Crossings: counted as points and as pairs; shared-endpoint crossings are separate.
- A drawing with unknown or cyclic parents, unknown endpoints, edges without a route, or non-finite geometry is an
  error, never a score.

**Engine correspondence (checked by a test and on 35 stored snapshots).** The engine's objective counts are
the shared metrics in their own units:

- `routedCrossings` = `crossingPairs` + `sharedEndpointCrossingPairs` (pairs, not points).
- `edgeNodeHits` = `lineNodeHits` + `containerTraversals`.

Consequence: the optimizer ranks a shared-endpoint crossing and a foreign container traversal exactly like the
other kinds of conflict, and it counts pairs where the metric counts points. Unifying these units is the next
step (see the algorithm plan, S2). Until then, compare engine objectives only with engine objectives, and
shared-metric numbers only with other shared-metric numbers of the same version. Metric version 2 numbers (the
profile runs before this change) are not comparable with version 3.

**Shared candidate choice and round selection (A1).** Refinement now chooses among candidates by the shared
conflict key: the shared metric in the configured objective order (`published_key`, `conflict_key`). A failed
measurement excludes the candidate; it is never scored. A refined result replaces the input only if its key is
strictly smaller. Channel views also offer the best congestion round as a routing candidate (`routing-only`),
next to the last-round baseline (`routing-last-round`). The initial layout always uses the last round.

Default outputs: eight default refinement fingerprints change through the shared key choice. Each was compared
under the shared key (old and new binary, same import): seven are lexicographically better, one has an equal key
with a different snapshot (layerweave-code@50). The reference is refreshed in
`fingerprints-20261021T103351Z.jsonl`.

Acceptance, Lesbar, six frozen inputs, metric version 3, candidate set with the baseline against the last round
without selection (lexicographic per input, shared key in default order):

| input | verdict | first difference |
|---|---|---|
| network-vlan | equal | - |
| state-tcp | equal | - |
| workflow-dag | equal | - |
| org-chart | better (node hits 17 → 16) | node hits |
| bpmn-pools | equal | - |
| uml-class | equal | - |

No input is worse. Round selection won only in org-chart, because refinement keeps the baseline unless a candidate
is strictly better. An earlier version that replaced the initial layout by a round produced a worse result on
bpmn-pools (hugging 5 → 7); that version was rejected, and the initial layout now stays on the last round.

An earlier reading in this document said that a higher-priority metric was worse on four of six inputs. That was
wrong: the comparison is lexicographic per input, and the workflow-dag, org-chart and uml-class differences in the
table of that earlier run were improvements in a higher-priority criterion.

Results: `profiles-20261021T103016Z.jsonl` (last round, baseline) and `profiles-20261021T103217Z.jsonl` (candidate set).

**Full preset matrix (S3).** `profiles-20261021T103753Z.jsonl` (+ `.md`): eight frozen inputs × four presets, three
repetitions, Apple M3, release build, metric version 3. Results are per input; no average is reported.

- Vorschau and Raster have no node or container conflicts on any input; they show many shared lines (Vorschau) or
  crossings (Raster), as expected for their settings.
- Lesbar and Qualität had foreign container traversals on five inputs (up to 106 on layerweave-code). Node hits were
  zero. The channel router treated container interiors as open space. A follow-up (below) forbids them.
- Budgets: the runner now passes the preset budget into refinement as a cooperative budget. Rows with
  `interruptedRuns` are cut at a point that depends on time, so they are not reproducible: those are the rows
  marked `det False` (synthetic-2k, Qualität). All other rows are deterministic over three repetitions. A deterministic
  work budget belongs to A5.

**Local congestion rounds (A2, first step).** The channel router runs eight congestion rounds instead of four. From
round four on, only routes that still share a grid segment are ripped up and rerouted; the others keep their paths.
Lesbar on the eight frozen inputs (`profiles-20261021T145326Z.jsonl`, against `profiles-20261021T120249Z.jsonl`): better
on four inputs (network-vlan: shared 2 → 1; workflow-dag: shared 56 → 32; layerweave-code: shared 411 → 388; state-tcp:
hugging 5 → 3), equal on four, worse on none. The overlaps that remain are long stretches in the open channel (median 77
to 197 units), not port artefacts; no shared pair involves an identified port. Cost: network-vlan 5.7 s → 9.0 s; the other
views are within noise. Not done yet: spaceRequest, local rerouting of legacy routes, and grouping conflicts into local areas.

**Placement search on the published key.** The placement search (`placementSearch > 0`, off by default) accepted
candidates on the engine's quality values, which disagree with metrics v3 on shared lines. It now accepts a candidate
only if its published key (metrics v3, in objective order) is strictly smaller. Lesbar on the eight frozen inputs
(`profiles-20261021T151732Z.jsonl`, against `profiles-20261021T145326Z.jsonl`): all eight better or equal on the first
differing objective (network-vlan: hugging 26 → 25 and crossings 67 → 55; state-tcp: hugging 3 → 2; synthetic-2k: hits
21 → 7); run times unchanged. workflow-dag, synthetic-2k and org-chart are near the 30 s budget, so their values may not
reproduce. Local channel candidates are not merged; see `experiments/README.md`.

**Per-route conflicts.** `routeConflicts` (scene crate `route_conflicts`) lists, for one route, the nodes it passes
through, the foreign containers it traverses, the nodes and containers it hugs, and the partner routes it shares a line
with, runs parallel to within the track spacing of, or crosses. The totals of `measure` are computed from the same
functions; a test checks that the per-route lists add up to the totals.

**Work budget (A5, deterministic part).** `refine` accepts `workBudget`: the number of candidate jobs (orderings,
routing, routing-last, placement search, and the global routing rounds) evaluated in their fixed order. The same budget
gives the same snapshot on every run and every target, and the report sets `workLimited` when it dropped work. A budget
that covers every job changes nothing (tested). The presets carry `workBudget` values that cover all their jobs. The
time budget (`budgetMs`) still applies on top, and a run that hits it is `interrupted` and not reproducible. Placement
search is deterministic through `placementSearch`. So a row marked interrupted is time-limited; reproducing it needs a
larger time budget or a smaller search budget for that view.

**Foreign container rule in the channel router.** A grid segment inside a container's interior is usable only
if the container owns one of the edge's endpoints (rule 6). Four inputs re-run with Lesbar
(`profiles-20261021T110838Z.jsonl`): container traversals go to 0 on network-vlan (5 → 0), org-chart (16 → 0),
uml-class (1 → 0); state-tcp is unchanged. Crossings and hugging also fall on org-chart (108 → 69, 64 → 50) and
network-vlan (75 → 69, 32 → 27). Network-vlan gains two shared pairs (0 → 2); node hits rank higher, so the
lexicographic verdict is an improvement. The default paths are unchanged.

Still over budget at 30 s (Lesbar) and 120 s (Qualität): workflow-dag, synthetic-2k and layerweave-code. Those views
need a faster routing search before Lesbar can be called readable there.

**Profiles** are the presets in `presets/` (Vorschau, Raster, Lesbar, Qualität), shared with the browser app.
They differ only in direction, placement, candidates, routing and search budget. Labels, node sizes, minimum
port spacing and bundling are identical across the four. Earlier results named the profiles `preview`,
`raster`, `readable` and `quality`; those ids are retired, and the old profile folder is removed.

**Frozen inputs** (`benchmarks/inputs/`, with `manifest.json`): eight views, including the repository's own
code graph, copied once from `benchmarks/datasets/`. The runner refuses an input whose SHA-256 differs
from the manifest.

```sh
python3 benchmarks/profile_runner.py freeze            # only when the input set changes
python3 benchmarks/profile_runner.py run --reps 3      # writes results/profiles-<time>.jsonl and .md
python3 -m unittest benchmarks/profile_runner_test.py  # pure parts of the runner
```

Each record stores the input hash and view size, the profile file hash and resolved options, the
engine revision and dirty state, platform, CPU, rustc, thread count, repetitions, and the metrics for
the first arrangement, after configuration, and after refinement. It also stores the timings (median
over repetitions and all values), peak memory where the platform reports it, budget status, and the
final snapshot under `results/profiles-<time>-snapshots/` for visual checks. Metrics are deterministic
for each input and profile; the runner reports whether repetitions agreed.


### Cooperative profile budgets

The profile runner now passes the remaining `budgetMs` (after import/configure) to native refinement.
Results record `budgetPolicy: cooperative-refinement-v1` and `interruptedRuns`. This is a soft budget:
a running candidate can finish after the deadline, and import/configure are not interruptible yet.
Budgeted refinement is serial; `threads` remains the requested setting, not a guarantee of parallel
execution. Time-budgeted snapshots may differ across repetitions. Historical reports without this
policy measured budget overruns but did not stop refinement. Do not directly compare those runs as
identical execution policies. The outer operation timeout remains a process-group kill.
