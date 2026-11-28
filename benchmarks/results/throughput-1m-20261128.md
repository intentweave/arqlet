# Throughput at 1M nodes: the baseline on main, with a phase split (#249, step 1, 2026-11-28)

What a resident session, a one-shot request and the WASM engine cost per operation at 100 000, 300 000 and 1 000 000
nodes on main, where the time goes, and which of the candidate steps of #249 could save how much. No engine change.

## Machine, build, inputs, load

- **Machine:** Apple M3 (8 cores, 16 GB), macOS 26.0 (Darwin 25.0.0), rustc 1.99.0 (b940084d7 2026-09-28), Node
  v24.21.0. **Shared and loaded**: other agents built and tested Rust during the runs. The 1-minute load before every row
  is in the raw files and in the tables below. The driver waited up to 180 s (600 s for the first repetition) for the load
  to fall below 4 before each repetition, and went on when it did not. Memory was tight too: the machine swapped during the 1M
  runs (5.9 to 6.4 of 7 GB of swap in use towards the end), so resident memory of a process (`ps`) is an indication,
  see "Memory" and "Which numbers to trust".
- **Revision:** `0ee5c68` (origin/main when the runs started; clean tree, worktree of that commit). Main moved on during the runs
  (#245 Leiden as an option beside Louvain, #246 the `estimate` operation, #258); the default (Louvain) is what was
  measured, and none of these is in the numbers.
- **Builds:** `cargo build --release --locked -p arqlet-runtime --bin layout-command` (release, `lto = "thin"`);
  `cargo build --release --locked -p arqlet-wasm --target wasm32-unknown-unknown` (the playground's build, not the
  smaller `wasm-size` build of the npm package).
- **Graphs:** `benchmarks/overview/generate.py` (code-index shaped: directories, files, symbols, calls mostly local; 1 %
  unresolved calls), seed 1, written by the driver to `benchmarks/datasets/throughput-<n>-s1.json` (git-ignored):

  | nodes | edges (unresolved) | containers | JSON | SHA-256 of the file |
  |---|---|---|---|---|
  | 100 000 | 151 865 (942) | 5 294 | 17.5 MB | `2e32aa733a378bf0dfe5b6e221fe617749bc19aebdf5854411c9e6325cf494ee` |
  | 300 000 | 453 770 (2 815) | 15 239 | 53.1 MB | `9660ab99a5d0d1408b7e3eceaf7ef8e602994d9f909dbb883bac52a6a5d1acf2` |
  | 1 000 000 | 1 511 361 (9 385) | 48 198 | 180.6 MB | `29444589717c9b431fef95eb64062e45810b9fde06b67b47e6d15318159d307e` |

- **First view** as in the earlier scaling files: `{op: "import", limit: 10000, options: {levelOfDetail: {nodes: 1000,
  edges: 3000}, liftEdges: true}}`. An abstracting view: every patch is laid out from scratch (`patch.rs`).
- **Drivers:** `benchmarks/scaling/throughput.py` (native: sessions, one-shot requests, `sample` profiles),
  `benchmarks/scaling/throughput-wasm.mjs` (the raw WASM module in Node, the engine-client worker's path),
  `benchmarks/scaling/throughput_summary.py` (tables). Raw rows: `throughput-1m-20261128.jsonl` (native) and
  `throughput-1m-20261128-wasm.jsonl` (WASM); the overview's lists of packed containers are stored as counts. Rows of
  attempts that failed or were interrupted stay in the files and are left out of the tables (`throughput_summary.py`).

```sh
cargo build --release --locked -p arqlet-runtime --bin layout-command
cargo build --release --locked -p arqlet-wasm --target wasm32-unknown-unknown
python3 benchmarks/scaling/throughput.py --sizes 100000 300000 1000000 --reps 3 --profile --max-load 4 --max-wait 180 \
  --out benchmarks/results/throughput-1m-20261128.jsonl
node benchmarks/scaling/throughput-wasm.mjs --sizes 100000 300000 1000000 --reps 3 --max-load 4 --max-wait 180 \
  --out benchmarks/results/throughput-1m-20261128-wasm.jsonl      # at 1M: --no-overview, then --overview-only (below)
python3 benchmarks/scaling/throughput_summary.py benchmarks/results/throughput-1m-20261128*.jsonl
```

### What each message is

A session (one `layout-command --sessions` process, or one WASM instance, per repetition) gets, in this order:
`import` (with its scene, no timings); `toggle-open` (the first collapsed container the scene shows) and `toggle-close`
(the same again, `scene: false`); `reveal` of a symbol inside a collapsed container; `columns`; `patch-small` (5 symbols
and 8 edges into one file); `patch-large` (2 000 symbols and 4 000 edges into 20 files); `memory`; `overview-first`
(the graph, `workBudget` 10^9, 4 threads natively, 1 in WASM) and `overview-byid` (continued from the first answer's
places by id, `start.nodes`, same budget); `import-timed` (the import again with `timings: true`, for its layout phases).
Every message but `import` asks for `timings` (layout and its phases). One-shot: one `layout-command` process per request
(`/usr/bin/time -l` for the peak): import, toggle of the same container on the import's snapshot, the small patch on it.
Wall time is from writing the request to reading the answer (pipe and JSON included). Three repetitions; medians, with
min–max.

## Baseline

Seconds, median of three repetitions (min–max), and the range of the 1-minute load before the rows. Native sessions ran
the overview with 4 threads, WASM with 1. In the one-shot column, the `toggle-open` row is the one-shot toggle.

| nodes | message | session, native: s (min–max) · load | one-shot, native | WASM session (Node, 1 thread) |
|---|---|---|---|---|
| 100,000 | import | 1.77 (1.34–2.60) · 7.8–9.3 | 1.40 (1.33–1.85) · 6.6–8.2 | 2.37 (2.30–2.39) · 3.8–4.0 |
| 100,000 | toggle-open | 0.83 (0.64–0.93) · 8.3–9.3 | 1.27 (1.18–1.72) · 6.6–8.2 | 0.79 (0.77–0.81) · 3.8–4.2 |
| 100,000 | toggle-close | 0.74 (0.57–1.00) · 8.3–9.3 | – | 0.70 (0.69–0.71) · 3.8–4.2 |
| 100,000 | reveal | 1.44 (1.12–1.49) · 8.3–9.3 | – | 1.38 (1.32–1.39) · 3.8–4.2 |
| 100,000 | columns | 0.03 (0.02–0.04) · 8.3–9.8 | – | 0.02 (0.02–0.02) · 3.8–4.2 |
| 100,000 | patch-small | 1.75 (1.42–1.75) · 8.3–9.8 | 1.52 (1.44–2.18) · 6.6–8.2 | 1.68 (1.63–1.70) · 3.8–4.2 |
| 100,000 | patch-large | 2.01 (1.61–2.01) · 8.1–9.8 | – | 1.93 (1.86–1.95) · 3.9–4.3 |
| 100,000 | overview-first | 4.40 (3.79–4.85) · 8.1–9.8 | – | 13.59 (13.21–14.20) · 3.9–4.3 |
| 100,000 | overview-byid | 0.81 (0.66–0.83) · 9.5–9.8 | – | 0.77 (0.77–0.78) · 4.0–4.2 |
| 100,000 | import-timed | 1.73 (1.45–1.85) · 9.6–9.8 | – | 1.66 (1.65–1.68) · 4.0–4.2 |
| 300,000 | import | 3.05 (2.94–3.08) · 4.0–6.1 | 3.12 (3.10–3.15) · 3.8–4.6 | 9.01 (8.97–9.36) · 3.6–4.8 |
| 300,000 | toggle-open | 1.19 (1.16–1.20) · 4.3–5.9 | 2.97 (2.84–3.12) · 4.0–4.5 | 1.72 (1.59–1.74) · 3.7–4.7 |
| 300,000 | toggle-close | 0.99 (0.98–1.06) · 4.3–5.9 | – | 1.49 (1.30–1.53) · 3.8–4.8 |
| 300,000 | reveal | 2.50 (2.49–2.52) · 4.3–6.1 | – | 4.21 (3.92–4.34) · 3.8–4.8 |
| 300,000 | columns | 0.05 (0.05–0.05) · 4.2–6.1 | – | 0.05 (0.04–0.05) · 3.9–4.9 |
| 300,000 | patch-small | 3.19 (3.16–3.21) · 4.2–6.1 | 3.66 (3.60–3.68) · 4.1–4.5 | 4.89 (4.53–5.57) · 3.9–4.9 |
| 300,000 | patch-large | 3.22 (3.21–3.22) · 4.2–6.1 | – | 4.46 (3.96–4.46) · 3.9–4.7 |
| 300,000 | overview-first | 7.27 (6.99–7.68) · 4.2–6.1 | – | 80.00 (78.38–81.94) · 3.9–4.5 |
| 300,000 | overview-byid | 1.67 (1.67–1.70) · 4.0–6.7 | – | 2.43 (2.18–2.51) · 5.0–8.3 |
| 300,000 | import-timed | 3.61 (3.58–3.74) · 4.0–6.7 | – | 4.52 (4.12–4.64) · 4.9–8.3 |
| 1,000,000 | import | 10.25 (9.50–10.96) · 3.9–5.3 | 9.42 (9.39–9.66) · 3.1–7.1 | 111.17 (107.86–118.60) · 3.5–3.9 |
| 1,000,000 | toggle-open | 2.54 (2.43–2.67) · 3.7–4.8 | 8.84 (8.63–9.24) · 3.0–7.1 | 3.26 (3.23–3.27) · 4.4–6.2 |
| 1,000,000 | toggle-close | 1.87 (1.86–1.87) · 3.6–4.8 | – | 2.59 (2.58–2.61) · 4.2–6.0 |
| 1,000,000 | reveal | 7.37 (7.34–8.17) · 3.6–4.7 | – | 17.54 (16.78–17.77) · 4.2–5.8 |
| 1,000,000 | columns | 0.16 (0.15–0.16) · 3.4–4.6 | – | 0.13 (0.13–0.14) · 3.5–5.2 |
| 1,000,000 | patch-small | 10.95 (9.94–11.93) · 3.4–4.6 | 11.53 (11.50–13.67) · 3.3–6.9 | 19.57 (18.87–19.98) · 3.5–5.2 |
| 1,000,000 | patch-large | 10.37 (10.27–10.69) · 3.1–4.4 | – | 12.59 (12.51–12.92) · 3.9–7.8 |
| 1,000,000 | overview-first | 16.94 (16.35–17.18) · 3.1–4.2 | – | – |
| 1,000,000 | overview-byid | 7.51 (7.29–7.73) · 2.8–4.1 | – | – |
| 1,000,000 | import-timed | 13.37 (13.21–13.76) · 2.8–3.9 | – | – |

WASM at 1M: the overview and `import-timed` are missing because they ran out of the 4 GiB of linear memory after the
session's earlier messages (`RuntimeError: unreachable` in `rust_oom`, after 619 s and 339 s of thrashing; stacks: the
overview's `graph_of` cloning the request's JSON value, and `communities` → `overview::build` inserting into a `BTreeMap`).
The three repetitions above ran with `--no-overview --no-import-timed`. The overview alone in a fresh WASM session at 1M did
not finish within the 25-minute limit (killed). See "Which numbers to trust" for the WASM import at 1M.

## Where the time goes at 1M

Seconds per category: the median wall time of the session rows times the share of the category in the `sample` profile
of the same message (one profiled repetition, 1 ms interval, load 3.3 to 4.0; `throughput.py --profile`). A sample
belongs to the first category in the driver's list that matches a frame on its stack (so JSON reading inside the
overview counts as JSON reading, an index built inside the structure as index). "memcmp" and "allocator" are the shares of
samples whose own frame is a string comparison or `malloc`/`free`/`memmove`, across all categories.

| message (1M, session) | wall | JSON parse | validate | structure | index | projection | layout | scene | JSON write | overview | other | memcmp | allocator |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| import | 10.25 s | 0.95 | 2.21 | 2.78 | 0.68 | 0.29 | 1.71 | 0.16 | 0.01 | – | 1.46 | 40 % | 16 % |
| toggle-open | 2.54 s | 0.02 | 0.06 | – | – | 0.34 | 1.88 | 0.15 | 0.03 | – | 0.05 | 30 % | 11 % |
| toggle-close | 1.87 s | – | 0.06 | – | – | 0.30 | 1.48 | – | – | – | 0.02 | 29 % | 10 % |
| reveal | 7.37 s | 0.01 | 0.97 | 2.66 | 1.28 | 0.32 | 1.48 | 0.17 | 0.02 | – | 0.46 | 39 % | 19 % |
| patch-small | 10.95 s | 0.01 | 2.05 | 4.50 | 0.66 | 0.33 | 1.49 | 0.23 | 0.02 | – | 1.67 | 37 % | 22 % |
| patch-large | 10.37 s | 0.01 | 1.98 | 4.22 | 0.63 | 0.35 | 1.52 | 0.23 | 0.02 | – | 1.41 | 38 % | 21 % |
| overview-first | 16.94 s | 0.94 | 0.53 | – | – | – | – | – | 0.41 | 13.77 | 1.29 | 4 % | 26 % |
| overview-byid | 7.51 s | 0.89 | – | – | – | – | – | – | 0.82 | 4.66 | 1.13 | 19 % | 22 % |
| columns | 0.16 s | – | – | – | – | – | – | 0.14 | – | – | 0.01 | 64 % | 3 % |

("other" includes the driver's small `clone/drop` category, 0.0 to 0.4 s; the profiled repetition's own wall times
were within 25 % of these medians: import 9.5 s, toggle 2.7 s, patch 12.5 s, overview 18.5 s.)

What the categories hold here (from the call trees in the profiles):

- **structure, import** (2.8 s): `select_visible` on the whole source graph: the community detection of the large
  containers (`with_community_containers` → `overview::hierarchy`, Louvain) about 40 % of it, the selection itself 25 %,
  the budget collapse (`level_budget_collapse`) 11 %, copying and dropping whole graphs about 15 %.
- **structure, patch and reveal** (4.5 and 2.7 s): `structured` again on the whole changed graph: community detection
  about 60 % (`ends_of`, `build`, `hierarchy`), whole-graph clones (`with_partial`, `Graph::clone`) and their drops about
  30 %. A reveal changes `partial`, which is part of the structure's key, so it derives the structure and its index
  (`GraphIndex::of`, 1.3 s) anew, as a patch does, but keeps the source graph (no validation of a new graph's change).
- **validate** (2.0 to 2.2 s on import and patch): `arqlet_model::validate` over 1M nodes and 1.5M edges, sets of string
  ids.
- **other, import and patch** (1.5 s): `import_graph_counted`'s own selection bookkeeping (`retain` of nodes and edges
  against `BTreeSet<String>`, mostly `memcmp`), and dropping the request's `serde_json::Value` (the session store reads
  every line as a value first).
- **layout** (1.5 to 1.9 s): the bounded view (about 1 000 boxes); the same with every message that lays out. Its phases
  (`timings`, medians at 1M): placement 0.64 to 0.82 s, routing 0.74 to 0.87 s (plan 0.19, channel growth 0.20,
  construction 0.31), port resolution 0.09 s, the rest (layering, ordering, finalization) about 0.1 s.
- **overview, first** (13.8 s of 16.9 s; the engine's own `timings.total` is 11.9 s): forces (`force`, 35 % of all
  samples), packing and `build`; the budget of 10^9 units ran out (`work.limited`, 55 551 containers packed). Outside the
  engine: the 180 MB request read as a JSON value and the 94 MB answer written.
- **overview, by id** (7.5 s for 3 M work units): mostly JSON: 57 MB of places in, 94 MB of answer out, and the
  hierarchy built again from the places.

## Largest shares at 1M, ranked

By seconds at 1M, over the operations the owner's goal names (import once, then many toggles, patches and overview
continuations); the shares are of the wall time of each message (session, native):

| rank | share | seconds at 1M | where |
|---|---|---|---|
| 1 | the structure derived again from the whole source graph: community detection (Louvain) of every large container, whole-graph clones and drops | patch 4.5 s (41 %), reveal 2.7 s (36 %), import 2.8 s (27 %) | `structured`, `select_visible`, `with_community_containers`, `with_partial` |
| 2 | validation of the whole graph | import 2.2 s (22 %), patch 2.0 s (19 %), reveal 1.0 s (13 %) | `arqlet_model::validate` |
| 3 | the layout of the bounded view (about 1 000 boxes) | toggle 1.5 to 1.9 s (72 to 80 %), import and patch 1.5 s (14 to 17 %) | placement 0.6 to 0.8 s, routing 0.7 to 0.9 s |
| 4 | the import's own selection bookkeeping on string sets, and dropping the request's JSON value | import and patch 1.4 to 1.7 s (14 %) | `import_graph_counted` (`retain` against `BTreeSet<String>`), `drop` of `serde_json::Value` |
| 5 | JSON of large messages | session import 1.0 s (9 %); overview 1.3 s first, 1.7 s by id; one-shot: a toggle costs 8.8 s against 2.5 s in a session, the difference is reading 208 MB and writing 189 MB (and validating and projecting again) | `serde_json`, the session store reading every line as a value |
| 6 | the structure's index built again | reveal 1.3 s (17 %), import and patch 0.7 s (6 to 7 %) | `GraphIndex::of` |
| 7 | the overview's forces and packing (with a budget of 10^9 units, limited) | overview-first 13.8 s | `arqlet_layout::overview` (`force` 35 % of samples) |
| 8 | projection and scene | 0.3 s and 0.15 to 0.23 s per message | `project_indexed`, `merge_aggregated_pairs`, `scene_of` |

Across all of them, string comparisons (`memcmp`) are 29 to 40 % of the samples of import, toggle, reveal and patch, and
the allocator 10 to 22 %: maps and sets keyed by `String` ids, on the source graph (import, patch, reveal) and on the
view (inside the layout).

## What each candidate step of #249 could save (estimates)

**These are estimates from the phase split above, not measurements.** Each says which seconds a step could reach and
what it could save at 1M on this machine; each step has to be measured before and after (CLAUDE.md, Benchmarks), and the
savings overlap (interned ids make validation cheaper, an incremental patch skips most of it).

| step of #249 | what it reaches at 1M | estimated saving | basis |
|---|---|---|---|
| 2. Interned ids for the source graph (ADR 0019 decision 4) | validate, structure, index, selection bookkeeping, projection: 7.4 s of an import, 9.2 s of a patch, 0.4 s of a toggle outside the layout | import −3 to −5 s (10.3 → 5 to 7 s); patch the same; toggle −0.2 to −0.5 s; the 0.76 GB heap per session (two copies of the graph and an index) maybe halved | `memcmp` alone is 40 % of an import's samples (4.1 s) and 37 % of a patch's; the allocator another 16 to 22 %. Not all of it goes (the layout's own maps are keyed by the view's ids) |
| 3. Incremental patch (#212 B, ADR 0029) | everything a small patch does today: 10.9 s, of which the layout is 1.5 s | small patch −8 to −9 s if the patch keeps the structure, index and validation of what it does not touch (communities of the touched containers only, no whole-graph copies), down to about a toggle (2 to 2.5 s); **an incremental layout alone (ADR 0029 B as written) saves at most part of 1.5 s (≤ 14 %)** | the patch's pre-layout part is 9.4 s: structure 4.5, validate 2.0, bookkeeping 1.7, index 0.7, projection 0.3 |
| 4. Memory per operation (#247, #248) | the native session store reads every line as a `serde_json::Value` (import, overview); the WASM session's import reads in one pass, but the overview (`graph_of` clones the value) and any request with `timings` take the value path | native session import −0.8 s (it takes 10.3 s against 9.4 s for the one-shot request, which reads in one pass), and its transient peak; **in WASM at 1M it decides whether the overview (and a timed import) runs at all after an import in the same session: both ran out of the 4 GiB linear memory** | session vs one-shot import times; the WASM failures below |
| 5. Columnar input / binary frames (ADR 0019 step 5) | JSON of the import message (180 MB) and of the overview (94 MB answer, 57 MB of places by id) | import −0.5 to −0.9 s; overview −1 to −1.7 s per message (part of it is already there: `columns: true` for the answer, `positionsOnly`); a toggle in a session nothing (its messages are under 100 bytes) | JSON parse 0.95 s of an import, 1.3 to 1.7 s of JSON per overview message |
| 6. Parallel layout of independent containers (`parallel_map`) | the layout of the view, 1.5 to 1.9 s of every message | toggle −0.3 to −0.9 s (2.5 → about 1.6 to 2.2 s); at most −1.4 s if all of the layout ran in 4 parts, which it cannot (routing crosses containers and is half of it). The same mechanism on the community detection per container (structure, 2.7 s in a patch) could save another −1 to −2 s on import, patch and reveal | layout phases by `timings`; communities are computed per container |
| 7. Write path (ADR 0023 W1) | where the cost lands, not how much there is | a view precomputed per revision answers like `columns`/`scene` (0.15 to 0.2 s); each change group still costs a patch (11 s today, 2 to 2.5 s after step 3) in the background | the patch rows; scene and columns rows |

Found on the way, not a step of #249 yet: **a reveal costs 3 times a toggle** (7.4 s against 2.5 s) because `partial` is
part of the structure's key, so a reveal derives the structure (communities) and its index again for the whole graph;
keeping them for a change of `partial` would save about 4 to 5 s per reveal (estimate). And the WASM import of a fresh
session is slower than a later one in the same instance (300 000: 8.7 s, then 5.8 s for the same import again; 9.0 s
against 4.5 s for `import-timed`; 100 000: 2.4 against 1.7 s), probably the growth of the linear memory, paid once per
instance (not verified).

Suggested order after this baseline (by the estimates; the owner decides): the incremental patch together with interned
ids (both remove the whole-graph work that dominates import, patch and reveal), the structure kept across reveals, then
#247/#248 (WASM at 1M), then parallel containers, then columnar input.

## The claim of #212 / ADR 0029: a patch costs a full layout

ADR 0029 (and #212) say a patch costs as much as a fresh import of the whole graph. ADR 0029 measured bus views (not
abstracting, up to 2 100 children); here the first view is abstracting (level of detail, communities), which `patch.rs`
lays out from scratch by design. At 1M that holds, and it is the main cost of a patch:

| 1M, native | session | one-shot |
|---|---|---|
| import (first view) | 10.25 s (9.50–10.96) | 9.42 s (9.39–9.66) |
| small patch (5 nodes, 8 edges into one file) | 10.95 s (9.94–11.93) | 11.53 s (11.50–13.67) |
| large patch (2 000 nodes, 4 000 edges into 20 files) | 10.37 s (10.27–10.69) | – |
| toggle (open / close) | 2.54 s / 1.87 s | 8.84 s |
| WASM, small patch / import / toggle | 19.57 s / 111 s (not trustworthy, swap) / 3.26 s | |

A 5-node patch costs 1.07 times an import in a session (1.22 times one-shot) and 4.3 to 5.9 times a toggle; a 2 000-node
patch costs the same as a 5-node one. The same at 100 000 and 300 000 (session: patch 1.75 and 3.19 s, import 1.77 and
3.05 s). But **the layout is only 1.5 s of the 10.9 s**: the rest is deriving the structure (communities) of the whole
changed graph again (4.5 s), validating it (2.0 s), the import's bookkeeping (1.7 s), the index (0.7 s) and the
projection (0.3 s). So at this scale the incremental patch of ADR 0029 (B, "keep what a patch does not touch") pays only
if it keeps the source-level structure, validation and index, not only the layout; B as an incremental layout would
save at most 14 %.

## Memory

**Heap of one session, counted exactly** (`benchmarks/session-memory`, the counting allocator of
[session-memory-20261121.md](session-memory-20261121.md); `EngineSession::run`, the one-pass import the WASM session
uses, `scene: false`; independent of the load). Built with `CARGO_TARGET_DIR=target cargo build --release --offline
--manifest-path benchmarks/session-memory/Cargo.toml` (its `Cargo.lock` is stale on main and was updated locally, not
committed), run as `target/release/session-memory benchmarks/datasets/throughput-<n>-s1.json 10000
'{"levelOfDetail":{"nodes":1000,"edges":3000},"liftEdges":true}'`:

| | 100 000 | 1 000 000 |
|---|---|---|
| kept by the session after the import | 78.5 MB (823 B/node) | 764.7 MB (802 B/node) |
| of it: source graph / the structure's view / its index | 30.9 / 30.9 / 12.7 MB | 313.9 / 313.9 / 127.5 MB |
| peak during the import, above the request text | 132.2 MB | 1 256.5 MB |
| the session's own estimate (`memory`) | 77.3 MB (98 %) | 757.7 MB (99 %) |
| the graph as a `serde_json::Value`, for comparison | 174.2 MB | 1 719.6 MB |

The structure's view is a second copy of the source graph (the communities add containers to it), so a session keeps the
1M graph twice plus its index: about 0.8 KB per node, as at 100 000 in November.

**Resident memory of the processes** (`ps`, polled every 0.1 s; macOS, and the machine was swapping during the 1M runs,
up to 6.4 of 7 GB of swap in use near the end, so these are indications): a native session process at 1M held 2.2 to 3.6
GiB after the import, 2.4 to 3.4 GiB after the other messages, and peaked at 3.8 GiB during the import, 4.6 to 4.7 GiB
during reveal and patches and 5.3 GiB during the first overview (which keeps its own copy of the graph). The difference to
the 0.76 GB heap is the line read as a `serde_json::Value` (1.7 GB at 1M, #248's subject), the copies an edit makes, and
memory the allocator keeps. One-shot requests peaked at 4.0 GiB (import), 4.1 GiB (toggle) and 4.3 GiB (small patch).
WASM linear memory (it never shrinks) after the import at 1M: 1.30 GiB; after the reveal 1.9 GiB; after the patches 2.4
GiB; the overview or a timed import on top ran out of the 4 GiB.

Several sessions per machine (the owner's goal): at 0.8 GB of heap per 1M session plus transient peaks of 1.3 to 2.6 GB
per import or patch (more in the native store, which reads the line as a value), a 16 GB machine holds a handful of 1M
sessions only if their imports and patches do not run at the same time.

## Which numbers to trust

- **Trustworthy (for comparisons on this machine):** native 300 000 and 1M session and one-shot rows (load 2.8 to 6.1,
  min–max within 5 to 20 %); WASM 100 000 and 300 000 (load 3.6 to 5.0, spread under 10 % except the 300 000 patch);
  WASM 1M toggles, columns, reveal and patches (spread under 7 %); the heap counts (exact, load-independent); the phase
  split as shares (one profiled repetition per size; its wall times were within 25 % of the medians).
- **Indications only:** native 100 000 rows (load 6.6 to 9.8, spread up to 40 % on the import); resident memory of
  processes (swap, allocator); the overview rows natively (4 threads on a machine with other work).
- **Not trustworthy:** the WASM import at 1M (108 to 119 s in three repetitions, 338 s in a fourth): the machine was
  swapping heavily at the time (5.9 GB of swap in use, the Node process mostly paged out). A first WASM import is slower
  than a later one also at 300 000 without swap (8.7 to 9.4 s against 4.5 to 5.8 s), so part of it is the fresh linear
  memory, but at 1M the swap dominates and the engine's own share is unknown. Likewise the WASM overview at 1M (killed
  after 25 minutes) and the 619 s and 339 s before the out-of-memory traps.
- **Native overview with 4 threads vs WASM with 1:** not the same work per second; the earlier
  [overview-work-budget-20261127.md](overview-work-budget-20261127.md) found native 1 thread and WASM within 7 %. Here WASM
  takes 80 s at 300 000 with a budget of 10^9 against 7.3 s natively with 4 threads: more than the threads explain, not
  investigated (no single-thread native run at that size).

## Not measured

- The engine-client worker in a browser, frame times, and a browser's memory limits (only Node with the raw module).
- WASM at 1M: the overview (in the same session: out of memory; alone: over 25 minutes), a timed import (out of memory),
  and a trustworthy first import (swap).
- `sample` profiles of WASM (none of the tools here sees inside the module); the WASM phase split is the layout's
  `timings` only.
- Several sessions in one process or several processes at once (only one session at a time was measured).
- An idle machine: the load stayed above 4 for most of the 100 000 rows and was 2.8 to 7 at 1M; no number here should be
  quoted without its load.
- The scene answers' cost in a host (JSON parse of 2 MB per message in the backend or browser).
- The `estimate` operation and the memory-estimate tools of #246: merged to main after `0ee5c68`, so not part of this
  baseline (the driver can add a row for it on the next run).
- Main after `0ee5c68` (#245 Leiden, an option; the default is unchanged).
