# Consistent layered pipeline: output-identical hotspot work, 2026-11-03

Machine: Apple M3, rustc 1.99.0, `bench` release build with `--features benchmark-timings`, revision dac8190 plus
uncommitted changes. The machine was not idle (load average 3.8 to 15 during the runs), so every comparison was timed
interleaved: binary A and binary B alternate per view, 3 rounds, `--reps 5 --threads 1 --limit 150`, on the nine
uniform-direction views of `benchmarks/eval/compare.mjs`. Both `--options` and `--import-options` were set to
`{"layeredConstruction":true,"hierarchicalOrdering":true}`. Without `--import-options`, `bench` times the import with the
default engine and reports only the refinement of the pipeline. Raw rows for the final comparison:
`layered-pipeline-hotspots-before-20261103.jsonl` (dac8190) and `layered-pipeline-hotspots-after-20261103.jsonl`.

## Output identity

Each step was checked with the same check set. The set covers 16 datasets of `benchmarks/datasets` plus the nine views,
run with the pipeline options. `fingerprint_compare.py` reported 25 of 25 identical for every step. From step 4 on, every
non-timing field of the `bench` rows was also compared, including `inputGeometryFingerprint`, quality keys and counts, and
all were identical.

The final state was additionally checked without options (default engine): 25 of 25 refinement fingerprints and all
non-timing fields are identical to dac8190.

## Profile before the work

`sample`, layerweave-code, release build with line tables. Shared preparation was below 1 % of the run: validation,
`GraphIndex`, and port context. The fallback grid search took 8.8 ms of the refinement. Repeated full candidate layouts
dominate, as expected, but inside them the time is spread over placement, routing, ordering, and quality metrics.
Sharing preparation between candidates would therefore save less than 1 %. The work below targets the measured
hotspots inside each candidate layout instead.

## Steps

Times are interleaved medians for layerweave-code and the sum of the nine views, before → after each step.

| # | change | lwc import | lwc refine | sum refine |
|---|---|---|---|---|
| 1 | `lane_free` in `section_routes` via sorted rank spans | 186 → 170 | 454 → 441 | – |
| 2 | `search_on` lane scan with monotone pointers instead of binary searches per lane pair | 171 → 153 | 434 → 408 | – |
| 3 | ordering sweep in `place`: neighbour lists and sort keys computed once | – | 407 → 360 | – |
| 4 | level gap lookup by range, without clones | 152 → 148 | – | – |
| 5 | `GraphIndex.edge_ends` (endpoint indices) in the per-area edge loops and `plan_port_offsets` | 147 → 138 | 356 → 346 | – |
| 6 | scene metrics: bounding-box prefilters in `touches` and `node_stats` | – | 345 → 305 | 551 → 492 |
| 7 | `routing::Exclusions` and `PreparedRoutes` (probes once per route) in `quality_of` | – | 305 → 282 | 492 → 458 |
| 8 | `route_node_conflicts` with prepared outlines and routes | – | – | 458 → 439 |
| 9 | node overlap test before the nesting test in `measure_variants` | – | 269 → 252 | 439 → 406 |
| 10 | `align_lanes` sweep on indices instead of string maps | 134 → 124 | 251 → 241 | 406 → 390 |

Steps 1 and 2 were timed as the pair "import + refine" on layerweave-code only; "–" means not recorded separately. Gaps
between one step's "after" and the next step's "before" come from machine load: each pair was timed in its own run.

## Overall (dac8190 → final, same run)

| view | import ms | refine ms |
|---|---|---|
| layerweave-code | 187.1 → 123.7 | 454.6 → 241.9 |
| workflow-dag | 27.6 → 23.2 | 50.5 → 33.0 |
| org-chart | 20.4 → 17.0 | 53.3 → 32.1 |
| uml-class | 4.2 → 3.5 | 46.7 → 34.9 |
| deployment-cluster | 11.7 → 10.0 | 26.7 → 20.0 |
| cloud-architecture | 7.0 → 6.0 | 13.8 → 10.3 |
| network-vlan | 5.0 → 4.3 | 11.1 → 8.2 |
| layerweave-pipeline | 5.0 → 4.5 | 8.2 → 6.4 |
| petri-net | 2.5 → 2.3 | 5.0 → 4.0 |
| **sum** | 270.4 → 194.4 | 670.0 → 390.9 |

These are in-process `bench` times. They exclude process start and snapshot JSON, which `compare.mjs` includes through
`layout-command`. The elk-rs control for layerweave-code was about 61–70 ms on this machine in earlier runs on the same
day, so the gap is still roughly a factor of 2 for the import and larger once refinement is included.

## Remaining hotspots (profile after step 9, layerweave-code)

`place` 31 %, of which `align_lanes` takes about a quarter; `route_all` 18 %; `hierarchical_sweep` 13 %; `search_on`
9 %; `measure_variants` 8 %, of which the pair loop is 56 %. The pair loop still visits all route pairs and rejects
distant pairs afterwards; generating candidate pairs with a sweep would need to keep the (i, j) order of the float sums.

## Reuse within a layout pass and per-candidate reporting

Same machine and method. A = the final state above (step 10), B = this section's state. Raw rows:
`layered-pipeline-reuse-before-20261103.jsonl` and `layered-pipeline-reuse-after-20261103.jsonl` (load average 3.2 to 7.4).

### What a refinement of the pipeline runs

With `layeredConstruction`, `plan_first_stage` plans one ordering candidate (forward barycenter), the routing-only
candidate, and one routing round. The candidate log (`--candidate-log`) now counts the layout passes and placement passes
of each candidate. The new work units are `layoutPasses`, `placementPasses` and `reusedPreparations`. They are
deterministic; `importStages` has the same counts for the import. On every view, the import and the ordering candidate
each run one layout pass with 1 to 4 placement passes:

- the first placement;
- again for backward edges;
- again after the hierarchical sweep;
- again for each round of channel widening.

Port sizing added no pass on these views. The repeated work is therefore inside one layout pass, not between candidates.

The order- and plan-independent rank analysis in `place` (links, Kahn, network simplex, levels) took about 0.6 ms of 4
placement passes on layerweave-code (temporary timing marks, removed). That is about 1 %. Sharing it between passes or
candidates is a small lever. Skipping the coordinates of placement passes that are later discarded would not be
output-identical, because `align_lanes` reorders border-node ranks from coordinates.

### Changes (output-identical)

| step | change | 9-view import sum ms | 9-view refine sum ms |
|---|---|---|---|
| 11 | The routing starts from the preparation of the last track count, which found no narrow channel. That count already prepared every edge and built the section routes on the same boxes, ports and plans (`constructive::Preparation`, provisional). The reuse is checked bit for bit against boxes, ports and spacing, and is taken once by the next `route_all`. | 195.5 → 184.8 | 394.7 → 384.2 |
| 12 | `hierarchy_order::weights_of` visits, per edge, only the areas that hold one of its ends, without copying ids. Previously it visited every area for every edge. The counts do not depend on the visiting order. | 184.5 → 180.3 | 383.3 → 376.5 |

`reusedPreparations` is 1 for the import and for the ordering candidate on every view (lwc and nine views). The
hierarchical sweep builds its numeric model once per layout pass already. Its remaining cost is the eight sweep runs,
which are part of the configured search.

Output identity: after each step, 25 of 25 pipeline fingerprints and all non-timing fields were identical, and so were
the default-engine rows.

### Overall (step 10 → step 12, same run)

| view | import ms | refine ms |
|---|---|---|
| layerweave-code (limit 150) | 126.0 → 114.1 | 242.5 → 231.9 |
| workflow-dag | 23.2 → 21.8 | 33.3 → 31.8 |
| org-chart | 17.0 → 15.0 | 32.2 → 30.3 |
| uml-class | 3.5 → 5.6 | 35.5 → 35.1 |
| deployment-cluster | 10.4 → 9.2 | 20.6 → 19.4 |
| cloud-architecture | 6.1 → 5.7 | 10.3 → 10.1 |
| network-vlan | 4.4 → 4.0 | 8.2 → 7.9 |
| layerweave-pipeline | 4.4 → 4.2 | 6.5 → 6.1 |
| petri-net | 2.3 → 2.1 | 4.0 → 3.8 |
| **sum** | 197.3 → 181.7 | 393.1 → 376.4 |

layerweave-code at limit 100 (`benchmarks/datasets`, 5 interleaved runs of 5 reps, medians): import 60.6 → 55.8 ms,
refine 101.7 → 97.1 ms. The uml-class import is within the run-to-run spread of this load.

### Where a refinement's time goes (step 12, one run with `--reps 3`)

Per candidate, in ms: the stages overlap, and routing contains the fallback (`delegated`).
"relieve" is the Optimize-time crossing step after the candidates, now reported as `stageTimesMs.relieveCrossings`.

| view | refine | ordering job | place | route | delegated | scoring | routing-only job | round | relieve |
|---|---|---|---|---|---|---|---|---|---|
| layerweave-code (limit 100) | 99.0 | 65.9 | 28.3 | 28.0 | 1.4 | 5.6 | 8.0 | 4.2 | 6.3 |
| layerweave-code | 232.8 | 157.0 | 62.8 | 73.0 | 7.9 | 13.8 | 20.9 | 8.6 | 19.3 |
| workflow-dag | 32.0 | 20.0 | 12.6 | 3.6 | 0.0 | 2.1 | 2.8 | 3.0 | 2.6 |
| org-chart | 30.4 | 15.6 | 7.4 | 4.0 | 0.8 | 2.1 | 4.8 | 4.9 | 1.4 |
| uml-class | 34.6 | 3.6 | 2.0 | 0.7 | 0.1 | 0.4 | 0.6 | 0.6 | 29.1 |
| deployment-cluster | 19.4 | 13.8 | 7.9 | 4.3 | 0.0 | 0.8 | 1.2 | 1.4 | 1.1 |

The relieve column comes from a separate run with temporary timing marks, which have since been removed. Its search
result was not accepted on any of these views.

Findings:

- On the views with 100 to 300 edges, the ordering candidate is about two thirds of the refinement. Placement and routing
  are about equal within it. The fallback router on delegated edges is 1 to 8 ms (1 to 5 edges).
- The relieve step dominates uml-class: 29 of 35 ms. It is a side search with a budget of full routings
  (`OPTIMIZE_EVALUATIONS`). Making it cheaper per evaluation would be output-identical. A smaller budget would change
  the output.
- Scoring costs about the same for each complete candidate: 5 to 14 ms on layerweave-code for both the ordering and the
  routing-only candidate. The round reuses its evaluation (`reusedEvaluations`) and scores in less than half that time.
  Whether the routing-only candidate's scoring repeats raw metrics of another evaluation was not checked.
