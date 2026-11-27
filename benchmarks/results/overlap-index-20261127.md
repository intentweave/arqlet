# Faster second routing pass with byte-identical output (#229, ADR 0029 A1, 2026-11-27)

Step A1 of ADR 0029: make the second routing pass of a large organic bus view faster without changing its output. Gate
(owner): the second pass at least 3 times faster on the dense view of #212, with byte-identical output on every stored
fingerprint file and on the #212 views, the same on 1 and 4 threads and natively and in WASM.

**Result: the gate is met.** On the dense view the second pass takes 3.9 s instead of 15.9 s (median of 5, **4.0 times
faster**), the whole import 5.2 s instead of 17.1 s; every answer is byte-identical.

- **Machine:** Apple M3 (8 cores, 16 GB), Darwin 25.0.0, rustc 1.99.0, Node 24.21.0. Shared machine: 1-minute load
  2.1 to 6.9 during the timed runs (the `load` field of every row); the builds ran in turn, the first one alternating per
  repetition, so a load change hits both alike.
- **Revisions:** base `f59ec9f` (main, engine 0.10.0); new: branch `overlap-index` (this change on `f59ec9f`).
- **Builds:** `CARGO_BUILD_JOBS=3 cargo build --release --locked -p arqlet-runtime --bin layout-command`, both with
  `benchmarks/experiments/second-pass-timer.patch` applied (it prints `PHASE secondPass MS` on standard error around the
  second pass, `reroute` + `finish` in `compute_counted_inner`, and changes no output). `bench` the same way with
  `--features benchmark-timings`.
- **Inputs:** public and generated: `benchmarks/experiments/bus-mesh.mjs UNITS BUSES STRATEGY STYLE CHILDREN EDGES`
  (#212), imported whole; the benchmark datasets (`python3 benchmarks/generate.py --no-manifest`).
- **Driver:** `benchmarks/experiments/overlap-index.mjs` (one process per request, time limit and one retry, one row per
  run); table: `benchmarks/experiments/overlap_index_table.py`; fingerprints:
  `benchmarks/experiments/overlap-index-fingerprints.sh`.

```sh
git apply benchmarks/experiments/second-pass-timer.patch      # in both trees
CARGO_BUILD_JOBS=3 cargo build --release --locked -p arqlet-runtime --bin layout-command
node benchmarks/experiments/overlap-index.mjs --base BASE --new NEW --reps 5 \
  --cases 100/30/organic/lines/240/1000,60/12/organic/lines/60/250,280/107/organic/lines/480/2100,100/30/layered/lines/240/1000,100/30/organic/bar/240/1000 \
  --out benchmarks/results/overlap-index-20261127.jsonl
node benchmarks/experiments/overlap-index.mjs --base BASE --new NEW --reps 3 \
  --datasets cloud-architecture,activity-order,deployment-cluster,state-order,state-tcp,workflow-dag,arqlet-pipeline,org-chart,uml-class,er-diagram,bpmn-pools,network-vlan,petri-net,layerweave-code,synthetic-2k,vehicle-buses-organic,can-bus \
  --out benchmarks/results/overlap-index-20261127-reference.jsonl
sh benchmarks/experiments/overlap-index-fingerprints.sh BASE_BENCH NEW_BENCH OUTDIR
python3 benchmarks/experiments/overlap_index_table.py benchmarks/results/overlap-index-20261127.jsonl
```

## 1. Before and after (release, medians of 5, ms)

| request | runs | second pass base | second pass new | speedup | total base | total new | speedup | load (1 min) | answers identical |
|---|---|---|---|---|---|---|---|---|---|
| 100/30/organic/lines/240/1000 (dense, #212) | 5+5 | 15 871 | 3 938 | **4.03** | 17 083 | 5 190 | 3.29 | 2.4-6.9 | yes |
| 60/12/organic/lines/60/250 | 5+5 | 1 886 | 683 | 2.76 | 2 206 | 987 | 2.24 | 2.4-6.0 | yes |
| 280/107/organic/lines/480/2100 (no second pass) | 5+5 | - | - | - | 3 606 | 3 570 | 1.01 | 2.4-6.0 | yes |
| 100/30/layered/lines/240/1000 (no second pass) | 5+5 | - | - | - | 247 | 254 | 0.97 | 2.4-5.9 | yes |
| 100/30/organic/bar/240/1000 | 5+5 | 15 716 | 3 947 | **3.98** | 17 135 | 5 363 | 3.20 | 2.1-5.9 | yes |

Single runs of the second pass (sorted): dense base 15 632, 15 819, 15 871, 17 616, 21 325; new 3 784, 3 885, 3 938,
4 488, 7 513 (the highest of each in the first repetition, at load 6 to 7). At the lowest load (2.4 to 2.5, repetition 5):
15 632 against 3 885, 4.02 times.

"Answers identical": the hash of the whole snapshot (boxes, routes, bus lines, revision, diagnostics) is the same for all
ten runs of a request.

The reference views (benchmark datasets, import with limit 100,
[`-reference.jsonl`](overlap-index-20261127-reference.jsonl)): the second pass runs only on synthetic-2k (22 against 19
ms); all 17 imports are byte-identical and take the same time within 4 %. On the refinement of the fingerprint views the
answers are identical as well (section 3).

## 2. Where the time went, and what changed

Probe build (temporary wall-clock timers, not committed) on the dense view, release, at load 17 (so the absolute times are
about twice those above; the shares are what matters). The second pass is one `improve_until_counted` run over the 1 037
routes the conflicts select, among 370 nodes:

| step | ms | share | calls |
|---|---|---|---|
| `improve_pass` (two passes, `best_route` per route) | 5 443 | 16 % | 2 |
| `repair_with_counted` | 282 | 1 % | 1 |
| `spread_tracks` | 1 501 | 4 % | 1 |
| `resolve_overlaps` | 27 151 | 79 % | 1 |
| of it: `pair_move` | 15 912 | | 4 723 |
| of that: overlap counts of every move against every partner | 7 036 | | 166 428 partners |
| of that: candidate moves (`shifted_candidates`, node hits of each over every node) | 3 749 | | 5 507 |
| of that: the 13 x 13 pairs of moves | 2 121 | | |
| node hits of the single-route candidates (all 370 nodes each) | 5 233 | | 490 509 |
| corridor spans (all 370 nodes each, outline built per node) | 3 435 | | 62 499 |
| overlap counts of the single-route candidates | 1 779 | | 477 466 |

What made it expensive:

1. **Every test scanned every node.** A candidate's node hits, a segment's corridor and `spread_tracks`' span and hit
   checks walked all 370 nodes, and for each looked the id up in the excluded set (`BTreeSet<String>`) and the parent map
   and built its outline again.
2. **Overlap counts recomputed per partner.** A route that cannot move alone shares segments with 35 routes on average;
   for each partner `pair_move` counted the overlaps of all ~13 moves of both routes again (and cloned the move lists),
   although a move's partners do not depend on which partner is tried.
3. **The segment index looked in three buckets** per segment (floor of the line and its neighbours), and dense bus tracks
   fill them.
4. **`best_route` scored ~290 candidates against all routes and nodes**, most of them far from the edge.

The change (`crates/routing/src/lib.rs`), each step keeping iteration orders, float operations and results:

- `OverlapNodes` / `ObstacleGrid`: the nodes in a uniform grid (at most 64 x 64 cells) over their boxes, grown by the
  largest clearance plus one unit. `EdgeObstacles`: an edge's outlines built once per route (same filter and `obstacle`
  as before, in node order), with their corridor boxes. Node hits test only the rectangles in the cells of each segment's
  box, with the unchanged `hits_outline` on the whole route (a rectangle can only be hit by an axis-parallel segment
  inside it); discs and routes with an oblique segment are tested against every outline. Corridors visit the strip of
  cells over the segment's range and fold the obstacles **in node order**, so the same `max`/`min` run in the same order.
  Used by `resolve_overlaps`, `shifted_candidates` and `spread_tracks`.
- `pair_move`: the partners of each move are listed once (`ListCache`, keyed by route and segment like the move cache);
  a count without `index` and the partner is the list less those two. An entry is forgotten when a route moves into or
  out of its region (the box of its moves grown by one unit; a shared segment lies within `EPS` of it) and when its own
  route moves. A pair of moves whose counts alone cannot beat the best pair is not tested for the shared segment between
  them (which adds at most one).
- Single-route candidates: the overlap count first, then the stub directions and node hits, only for a candidate that
  would be kept (all conditions must hold; their order does not change which candidate wins).
- `SegmentIndex`: a segment is listed under every floor its line takes within `2 * EPS`, so a lookup reads one bucket.
- `best_route`: the obstacles and routes whose boxes are apart from the box around all candidates are left out once
  (every test begins with that box test), the rest keep their order.

After the change, the same probe on the dense view (fast profile, load about 3, before the last two steps): second pass
5.8 s, of it `improve_pass` 2.0 s, `resolve_overlaps` 3.3 s (pair lists 1.3 s, move pairs 0.5 s, single-route search
1.0 s), `spread_tracks` 0.3 s. The single-bucket index and the reordered candidate tests then took it to 3.7 s in the same
build. What remains is spread over `best_route`'s rails and scores, the pair search and the overlap lists; none of it scans
all nodes any more.

## 3. Byte identity

- **#212 views:** the whole snapshot is identical between the builds on all five bus meshes (50 runs, section 1) and
  natively against WASM (`wasm32-unknown-unknown`, release, through `packages/engine-client/wasm-client.js`): the hash
  of the whole answer of native new, WASM new and native base is the same on 100/30/organic/lines/240/1000,
  60/12/organic/lines/60/250, 100/30/organic/bar/240/1000, 280/107/organic/lines/480/2100 and
  100/30/layered/lines/240/1000.
- **Fingerprints** (`bench --limit 100 --reps 1 --threads 1,4`, both builds, each file's configuration; every row has
  `threadsIdentical: true`):

| configuration (stored file) | views | base vs new | stored file vs base, and vs new |
|---|---|---|---|
| default (`fingerprints-20261126-default-0.10.0.jsonl`) | 15 | identical 15 | identical 15, identical 15 |
| `loopSpace: false` (`fingerprints-20261115-stacked-default.jsonl`) | 19 | identical 19 | metadata only 14, input changed 1 (layerweave-code), both builds |
| `sharedBorderRanks: false, alignStacked: false` (`fingerprints-20261114-channels-by-demand-default.jsonl`) | 19 | identical 19 | metadata only 13, geometry changed 1 (state-order), input changed 1, both builds |
| `channelsByDemand: false` (`fingerprints-20261108-single-pass-default.jsonl`, dataset paths read as names) | 19 | identical 19 | metadata only 9, geometry changed 6, input changed 3, both builds |
| legacy pipeline: `singlePassAuto: false, singlePass: false, abstractRouting: false` (`fingerprints-20261021T103351Z.jsonl`) | 19 | identical 19 | geometry changed 14, input changed 4, both builds |

  The differences against the older files are main's (engine 0.5.0 and earlier against 0.10.0), the same for both builds.
  `singlePassAuto: false` alone is refused by `bench` on main (its interaction step: "Abstract routing requires unflattened
  hierarchical placement ..."), so the legacy pipeline was run with the two other options off as well. The default ran
  without `--options`: an empty object lands in the snapshot and changes its fingerprint.
  The new build's default rows: [`overlap-index-20261127-fingerprints.jsonl`](overlap-index-20261127-fingerprints.jsonl).
  The timed builds were made before four edits for clippy that change no behaviour (an entry API, `partial_cmp` for a
  negated `<`, a closure); the committed code was built again (release, without the timer) and checked once more: all five
  configurations identical to the base build (91 views, threads 1 and 4 identical), and the answers to the dense, small
  and `bar` bus meshes byte for byte equal to the base build's.
- **Platforms:** `tools/platform-parity.mjs` 26 of 26 identical between native and WASM (darwin-arm64), with a new
  request that runs the second pass (bus mesh 60/12, so CI checks the overlap resolution on every platform).
- **Unit test:** `the_obstacle_grid_agrees_with_a_scan_of_every_node` compares the grid's hit counts with a scan and its
  corridors bit for bit with `corridor_span`, on random nested boxes and discs, short, touching and oblique segments, with
  and without clearances (a grid with too small a margin fails it). The existing
  `the_segment_index_agrees_with_shares_segment` covers the single-bucket index.

## Not measured

- Other machines and loads below 2; WASM timing beyond single observations (the dense view took 8.4 s in Node's WASM,
  one run, not compared with the base build's WASM).
- Larger views whose first routing leaves more than 500 conflicts get no second pass (#219); the change does not alter
  that threshold, and on such a view (280/107) the time is unchanged.
- The patch path of #212 (`patch` into the overview) was not run separately; it lays out the dense view as an import
  (ADR 0029), so the second pass is the same.
