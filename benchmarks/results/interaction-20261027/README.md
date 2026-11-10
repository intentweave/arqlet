# Interactive edits, baseline of 2026-10-27

Produced by `target/release/bench --graph benchmarks/datasets/<name>.json --limit <L> --reps <R> --threads none --name <name>`;
the `interaction` field of each JSON file holds the scenarios (`crates/runtime/src/bin/bench.rs`, `interaction()`).

Environment: Apple M3 (8 logical CPUs), macOS, rustc 1.99.0, `--release` (thin LTO), single run per scenario on a machine that
was **not idle** (load average 4 to 7, other jobs running): read the numbers as indicative, not as a stable measurement.
Revision: `f221c1a` (mixed-layouts, nested-tree-radial, petri-net-shapes; limit 100, 100, 200, repetitions 3) and `81f161d`
(layerweave-code; limit 1000, one repetition after one warm-up). The engine code is the same in both.

Every edit is one request to the engine with the whole snapshot. Times are the median of the repetitions.

| model | view nodes / edges | import | drag update | size changed | same size again | pin | snapshot |
|---|---|---|---|---|---|---|---|
| mixed-layouts | 18 / 16 | 23 ms | 0.78 ms (1 of 4 areas) | 0.82 ms (4 of 4) | 0.009 ms (0 of 4) | 0.79 ms (1 of 4) | 14.7 kB |
| nested-tree-radial | 160 / 121 | 15 ms | 14.4 ms (36 of 36) | 14.4 ms (36 of 36) | 0.075 ms (0 of 36) | 14.4 ms (36 of 36) | 140 kB |
| petri-net-shapes | 24 / 41 | 798 ms | 37 ms (1 of 1) | 109 ms (1 of 1) | 0.018 ms (0 of 1) | 3.6 ms (1 of 1) | 31 kB |
| layerweave-code | 585 / 921 | 65.4 s | 65.4 s (56 of 56) | 65.5 s (56 of 56) | 0.7 ms (0 of 56) | 80.7 s (56 of 56) | 1.13 MB |

"Same size again" is a measurement with a newer geometry revision and an unchanged size. Before the change of 2026-10-27 (commit
`d04f7df`) it advanced the layout version and laid out again; the cost of that old path was not measured separately on this
machine (it equals the "size changed" column by construction, since both ran the full layout).

Findings:

- On `layerweave-code` every edit that reaches the layout costs about as much as the import (all 56 areas are laid out again), so
  a drag step is not interactive in any mode that sends it to the engine. Only the no-op is cheap.
- On `nested-tree-radial` a drag step lays out all 36 areas although it changes one.
- `petri-net-shapes` spends 798 ms on import for 24 nodes.
- Where the time goes inside the 65 s import was not measured yet.

## After the grid-router prefilter (same day, same machine, still not idle)

A profile of the 65 s import (`sample`, 20 s, release) put about 68 % of the samples in `routing::hits_outline`, 20 % in
`hits_probe` and 10 % in `grid_route`: the legacy repair router's grid search tested every grid step against every obstacle
and built a route for each test. `grid_route` now tests only the obstacles whose box meets the step's box (the prefilter
`hits_outline` applies first anyway), without building a route when none does.

| measurement | before (`81f161d`) | after |
|---|---|---|
| `layerweave-code` import, limit 1000 (585 nodes), one run after a warm-up | 65.4 s | 13.5 s |
| refinement fingerprints, `layerweave-code` limits 25, 50, 100 (thread 1) | `5c2035c32237ca79`, `c8e25bedd0e934cc`, `44dab2d6e9fc5795` | identical |
| geometry fingerprint, limit 100 | `465ebfe7d1c779ae` | identical |

The load differed between the two runs (4 to 7 before, similar after), so the factor is indicative. The edits on this model
still lay out all 56 areas (see above); this change makes each layout cheaper, not smaller.

## After the obstacle grid index and the local pin/size edit (`d999edb`, same machine, load 2 to 4)

`layerweave-code`, limit 1000, three repetitions after a warm-up (`layerweave-code-after.json`):

| step | before (`81f161d`, one run) | after (`d999edb`, median of 3) | areas laid out again |
|---|---|---|---|
| import | 65.4 s | 3.77 s | |
| add node | 67.7 s | 3.88 s | |
| drag begin | 65.5 s | 3.84 s | 56 of 56 |
| drag update | 65.4 s | 3.84 s | 56 of 56 |
| size changed | 65.5 s | 3.78 s | 56 of 56 |
| same size again | 0.7 ms | 0.58 ms | 0 of 56 |
| pin | 80.7 s | 3.84 s | 56 of 56 |

All 19 configurations of the fingerprint check (`benchmarks/fingerprint_compare.py`, thread 1, old and new binary on this machine)
are identical after both router changes.

The pin/size edit now places only the node's area and the containers above it (`d999edb`), and the test
`a_local_edit_gives_the_geometry_of_a_complete_layout` shows that the geometry equals a complete layout on layered fixtures. It
does not reduce the 56 areas here, because the corridor widening (`required_clearance` is set by default) re-places the whole
drawing from scratch as soon as one corridor is too narrow, which is the case on this model. A profile of one edit puts the
time in the repair router's grid search (the heap and the search steps), not in the placement. Making an edit cheaper than
that needs routing only the affected edges, which gives a different result than a complete routing and is a decision for the
owner, not an exact optimisation.

## Default outputs after the self-loop, history, selection, port and label changes (`27a38ef`)

`benchmarks/fingerprint_compare.py`, thread 1, the binary from before the router changes against the current one: 16 of the 19
configurations are identical. `layerweave-code` reports "input changed" because that dataset is generated from the repository
and grew from 585 to 599 nodes while the work added modules; on the new input the old and the current binary give identical
refinement and geometry fingerprints at limits 25 (`5d6c2f2504c941b0`, `8cac5a991749e17a`), 50 (`fb34aa6d42f5090a`,
`6f8ea8277281721f`) and 100 (`e68f88d05878f667`, `05bb1601004a041a`). None of the 19 inputs has a self-loop, so the loop rule,
the circle ports and the bundling exclusion change no fingerprint here; the fixtures with loops are not part of this check.

## Local routing and local corridor widening (M9, first part)

An edit that lays out only part of the drawing now keeps the routes it does not touch: an edge whose two ends moved by the same
distance keeps its route, carried along by that distance; every other edge, and every kept route that now runs into a node or
too close to one that did not move with it, is routed again by the repair router (`repair_subset_counted`,
`local_routes` in `crates/layout/src/lib.rs`). The corridor widening re-places only the areas it widens and the containers
above them, starting from the widths the earlier drawing had (new `Snapshot.corridorGaps`) and widening only corridors the
edit touched. After the edit the routes that run into a node are counted; if there are more than before, the complete layout
decides and is kept when it has fewer.

Contract of this path (it is not byte-identical to a complete layout): the same edit on the same drawing gives the same
result; the boxes, pins and container constraints stay binding; the routes of edges the edit does not touch stay as they were;
the result has no more routes through nodes than the drawing before the edit, or than a complete layout with the same options.

`layerweave-code`, limit 1000 (599 nodes), release, Apple M3, load average 4.5 to 7, three repetitions (`layerweave-code-m9.json`,
revision `5a8b3c1` plus the working tree of this change, measured again after `local_dirty` ignored the measurement revision
so that a size report is a local edit too; load average 3.2 to 6.4):

| step | before (`d999edb`) | after |
|---|---|---|
| drag begin / update | 3.84 s, 56 of 56 areas | 0.46 s, 3 of 59 areas |
| pin | 3.84 s, 56 of 56 areas | 0.46 s, 3 of 59 areas |
| size changed | 3.78 s, 56 of 56 areas | 0.47 s, 3 of 59 areas |

`crates/runtime/tests/local_edit_large.rs` (ignored; `cargo test --profile fast -p layerweave-runtime --test local_edit_large
-- --ignored --nocapture`) compares one edit against a complete layout with the same options on this model:

| edit | local edit | complete layout | node hits (base / local / complete) | routes unchanged from the base (local / complete) |
|---|---|---|---|---|
| leaf wider by 60 | 0.48 s, 3 areas | 4.15 s, 59 areas | 43 / 43 / 55 | 952 / 944 of 955 |
| leaf wider by 400 | 0.35 s, 3 areas | 4.18 s, 59 areas | 43 / 41 / 53 | 952 / 945 of 955 |
| leaf wider by 1500 (a corridor must be widened) | 5.95 s, local result rejected, complete layout kept | 4.24 s | 43 / 151 / 151 | 221 / 221 |

In the last row the local result had 819 node hits, more than the 43 before, so the check fell back to the complete layout; the
time is the local attempt plus the complete layout. Without the check the edit would have been wrong, not slow.

Findings:

- **A cliff in the repair router.** The grid search gives up above 120,000 cells (`grid_route`) and keeps the route unrepaired.
  This model's grid is about 247 x 491 = 121,000 lines already before any edit, so a few more lines flip many routes. The limit was
  not changed here (it would change the output of every view that is near it).
- **Any pin on this model makes the layout much worse**, with or without the local path and also in a complete layout:
  the node hits go from 43 to over 3,000 (checked at commit `c09bf47`, before this change). The cause is not investigated.
  The pin edit above therefore compares against a size edit instead.
- **Fingerprints.** 14 of the 19 configurations now give a different full refinement fingerprint and the same geometry
  fingerprint: the snapshot carries `corridorGaps` where a corridor was widened. Two are identical, three (`layerweave-code`)
  have another input because the dataset is generated from the repository.
- The check that the edit did not leave the drawing worse costs about 0.12 s on this model.

## M9 completed: new node, preview, settle (revision of this change)

| step on `layerweave-code` (599 nodes, release, load 4.3 to 5) | before | after |
|---|---|---|
| add node (`add_node`) | 4.84 s, 59 areas | 0.48 s, 1 area |
| drag step, pin, size change | 0.47 s, 3 of 59 areas | 0.47 s, 3 of 59 areas |

- The comparison "more routes through nodes than before" counts only routes of edges the earlier drawing had: the route of a new
  edge may be unresolved without the edit being worse. With the new edge counted, every added node fell back to the complete
  layout (4.9 s).
- A step of a drag that would leave more routes through nodes than before is a preview (`editReport.preview`); the step that ends
  the gesture decides. Checked on the 1500-unit leaf: the ordinary edit falls back (40 routes through nodes, against 121 before),
  the same edit as a drag step is a preview (155) and keeps its cost.
- The base import of this model leaves 121 routes through nodes by this count; a second pass over it (the complete path with the
  earlier drawing as start) leaves 6. The repair is a single pass, and a second one improves it. Not followed up.
- The distances of an organic area are kept for the next call with the same children and links
  (`organic::distance_memo_stats`), and are not computed when a warm start holds the positions.

## M3 completed: string lookups out of the hot path

A profile of one local edit (`sample`, release) put about 60 % of the samples in `memcmp`: string comparisons in map lookups and
linear searches. The callers, by size: `corridor_gaps` (all pairs of all nodes, each looking the parent up by name), the check
`routes_hitting_nodes` (a set of ancestors and an obstacle for every node, for every route), `labels::decorate`,
`validate_geometry` and the repair of kept routes (the obstacles of every node were built for every route).

Changes, none of which changes the output (the 19 configurations of the fingerprint check are identical to the run before them):

- `corridor_gaps` groups the nodes by parent once and pairs only siblings.
- `routes_hitting_nodes` and the repair test only the nodes whose box, grown by the clearance, meets the route's box, and build the
  obstacles of all nodes only for a route that does hit one.

`layerweave-code`, limit 1000 (607 nodes at this revision, the dataset grows with the repository), release, load average 4.6 to 7,
five repetitions (`layerweave-code-m3.json`): drag step, pin and size change 0.27 s (3 of 60 areas), new node 0.27 s, against
0.47 s before this step. Remaining in a profile of an edit: the repair of the routes that are made again, `validate_geometry`,
`labels::decorate`, and the second count of routes through nodes. `LookupTables` as a type was not introduced: the measured cost was
in these loops, and a shared table would have changed their signatures for no further gain.

## M10: publication contract, keyed viewer, slim moves

Measured in headless Chrome 155 (macOS, Apple M3, 1400 x 900, real mouse input), `browser-checks/latency.mjs`, 20 moves of a leaf,
load average 4 to 8:

| model | pointer to preview (median) | requests / replies | bytes up / down |
|---|---|---|---|
| `layerweave-code`, 548 nodes drawn, drawing everything each frame | 59 to 65 ms | 1 / 0 in the window (the engine step takes longer than the drag) | 1.24 MB (one full snapshot) |
| same, drawing compared element by element (`keyed: true`) | 479 to 553 ms | | |
| `workflow-dag`, 64 nodes | 46 ms | 27 / 27 | 3.3 MB / 2.8 MB |

- **Keyed viewer.** Comparing the drawing with the host by markup serialises every element (about 1500) and costs about eight times
  what drawing everything again costs. It is therefore an option of `renderSvg` (`keyed: true`, tested in `keyed.test.mjs`), not
  used by the page. Stable event targets are not gained by default; they would need a key from the scene's data before the elements
  are made. The pointer capture does not depend on it (it sits on the drawing, and `gesture-robustness.mjs` passes).
- **Slim moves (not built).** The engine boundary is a JSON string: a worker that kept the last snapshot would still serialise it
  for every step, so only the structured copy of the message would be saved (about half of the bytes of a drag), and the backend
  mode is stateless by design. The engine step itself (0.27 s on the 607-node model) dominates. Revisit if the copy shows up in a
  profile.
- **Publication contract.** A streamed publication is a typed `PublicationEnvelope` (job, sequence, `kind`, input identity, snapshot,
  score, archive; `publicationVersion` 4), generated into the TypeScript contracts. The page counts results that arrive for a view
  that has changed or for a superseded job and shows "Stale results discarded: N". The page's policy (`session.js`) and the Rust
  policy (`session.rs`) run the same nine cases from `examples/fixtures/session/cases.json`.

## Against origin/main at the end of M11 (`729d818`, the merge of PR #88)

`benchmarks/fingerprint_compare.py`, thread 1, a release `bench` built from `origin/main` against one built from this branch, both on
the current datasets: the geometry fingerprint is identical in all 19 configurations. The full refinement fingerprint differs in 17
(`metadata only`): the snapshot of a view with a widened corridor carries `corridorGaps`. `state-order` and `state-tcp` are byte-identical.
(The stored `fingerprints-20261021T103351Z.jsonl` is older than `main` and no longer the reference: it reports `geometry changed` for
15 configurations against any build of the current line.) Earlier figures of "14 of 19" in this file compare with a binary of the same
branch before the router change, where the three `layerweave-code` rows had another input.

## The 120,000-cell cliff, the pin finding and the second repair pass were one cause

The repair router's grid search (`grid_route`) built its grid from every obstacle and gave up above 120,000 cells, leaving the route
as it was (through the nodes it ran into). The code model sat just below that limit (about 247 x 500 lines in the whole view) and any
coordinate that is not on the grid of the others (a pin at a moved place, a widened corridor) pushed many routes over it.

What each earlier observation was:

- **Any pin raised the routes through nodes from 43 to over 3,000.** A pin at the current place changes nothing (all 608 boxes and 990
  routes are equal to the layout without it). An earlier probe that "pinned at a shift of 0, 4 and 40 with the same result" did not apply
  its shift (the environment variable was no longer read after the test was rewritten); that part of the earlier note was wrong. A pin at a moved place
  (+40, +10) brought the lines in y from 500 to 566 and 3,851 node hits in a complete layout.
- **The import left 121 routes through nodes, a second pass 6, the new node report said 121 before the edit.** The same routes: the
  first pass could not repair them, the second pass had other coordinates. The base layout of the 608-node model had 228 node hits.
- The drift of the numbers between the measurements (43, 55, 121, 228) is the dataset growing with the repository: it is generated
  from the repository's own source.

Change: when the grid of the whole view is over the limit, the lines are taken only from the obstacles near the route, in windows that
grow (120, 480, 1,920 units around the route's box); the freedom of every step is still tested against all obstacles. Views within
the limit take the same path as before, so their output is unchanged: 16 of the 19 sweep configurations are byte-identical to the run
before, and the geometry of `layerweave-code` at limits 25, 50 and 100 is identical to `origin/main` on the same input (the three
rows the comparison reports as "input changed" because the dataset grew between the runs).

`layerweave-code` at limit 1000 (608 nodes, release, load average 9 to 24, `layerweave-code-windowed.json`):

| | before | after |
|---|---|---|
| node hits of the layout (import) | 228 | 0 |
| node hits after a pin at a moved place, complete layout | 3,851 | 0 |
| node hits after a pin at a moved place, local edit | 1,346 | 0 |
| routes through nodes before an edit (`editReport`) | 121 | 0 |
| import | 4.2 s | 4.3 s |
| drag step, pin, size change, new node | 0.27 to 0.29 s | 0.28 to 0.29 s |
| local pin edit against a complete layout (`local_edit_large`, `LW_EDIT=pin`) | 1.64 s against 1.33 s | 0.53 s against 1.68 s |

The windowed search does not slow the import measurably; where it applies it is smaller than the whole grid.
