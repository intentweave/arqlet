# Reveal keeps the structure: toggle and reveal at 100k, 300k and 1M (#261, 2026-11-28)

A reveal used to rebuild the whole structure (communities, index, validation) of the source graph because `partial`
(the containers shown in part) was part of the structure's key. `partial` is out of the key now (except under a compacting
filter): the structure is kept under what is shown in part, and a reveal derives only the rests and an index of that
view (`structure_of`, `ViewStructure::base`). Answers are byte-identical (see below).

## Machine, build, load

- **Machine:** Apple M3 (8 cores, 16 GB), macOS 26.0, rustc 1.99.0, Node v24.21.0. **Shared and loaded**: other agents
  built and tested on it during the runs; the 1-minute load before every row is in the raw files and in the table
  (3.4 to 4.7; the driver waited up to 180 s for it to fall below 4). Memory was tight too (swap in use at 1M).
- **Revision:** `85b2216` = origin/main (`46a4c76`) merged with the #249 driver (`origin/throughput-baseline`).
  *Before* is that tree (`/tmp/lc-261-before`), *after* the same tree with this change (`/tmp/lc-261-after`; the raw
  rows say `dirty` for it). Both: `cargo build --release --locked -p arqlet-runtime --bin layout-command` (thin LTO).
- **Graphs, first view, driver:** as in [throughput-1m-20261128.md](throughput-1m-20261128.md) (seed 1, first view
  `levelOfDetail {nodes: 1000, edges: 3000}`, `liftEdges`), native sessions, 3 repetitions, overview left out.
  The `reveal` message reveals one symbol under a collapsed container of the first view (partial), after the two toggles.

```sh
python3 benchmarks/scaling/throughput.py --sizes 100000 300000 1000000 --reps 3 --mode session --no-overview \
  --max-load 4 --max-wait 180 --binary /tmp/lc-261-{before,after} --out benchmarks/results/reveal-structure-20261128-{before,after}.jsonl
python3 benchmarks/scaling/reveal_compare.py benchmarks/results/reveal-structure-20261128-before.jsonl benchmarks/results/reveal-structure-20261128-after.jsonl
```

## Result

Seconds, median of three repetitions (min-max), and the range of the 1-minute load before the rows. The before run came
first, then the after run, one after the other (not interleaved), so the load is similar but not equal.

| nodes | message | before: s (min-max) · load | after: s (min-max) · load | after / before |
|---|---|---|---|---|
| 100,000 | toggle-open | 0.58 (0.57-0.58) · 3.8-3.9 | 0.57 (0.56-0.58) · 3.6-3.8 | 0.98 |
| 100,000 | toggle-close | 0.52 (0.52-0.52) · 3.8-3.9 | 0.52 (0.51-0.52) · 3.7-4.0 | 1.00 |
| 100,000 | reveal | 1.01 (1.00-1.01) · 3.9-3.9 | 0.63 (0.62-0.64) · 3.7-4.0 | 0.63 |
| 300,000 | toggle-open | 1.23 (1.22-1.24) · 3.8-4.1 | 1.27 (1.20-1.31) · 3.5-3.9 | 1.04 |
| 300,000 | toggle-close | 1.02 (1.01-1.05) · 3.7-4.1 | 1.06 (0.99-1.10) · 3.5-3.9 | 1.04 |
| 300,000 | reveal | 2.54 (2.52-2.54) · 3.7-4.1 | 1.34 (1.27-1.40) · 3.5-3.9 | 0.53 |
| 1,000,000 | toggle-open | 2.57 (2.54-2.71) · 4.0-4.6 | 2.62 (2.58-2.65) · 3.4-4.7 | 1.02 |
| 1,000,000 | toggle-close | 1.92 (1.91-1.92) · 4.2-4.6 | 2.06 (2.04-2.52) · 3.4-4.7 | 1.07 |
| 1,000,000 | reveal | 7.94 (7.91-8.49) · 4.2-4.7 | 3.33 (3.20-4.89) · 4.1-4.7 | 0.42 |

A reveal at 1M goes from 7.9 to 3.3 s (the remaining excess over a toggle, about 0.7 s, is the rests' copy of the
structure's view and its index, one pass over the graph each); at 300k from 2.5 to 1.3 s, at 100k from 1.0 to 0.6 s. Toggles
are not changed by the code (a toggle that closes the rest of a container derives the same way; the toggles of this driver
see no partial view): their differences of up to 7 % at 1M are within the noise of this loaded machine (the repetitions of
one build differ by more, see min-max), not a change we claim.

Not measured: a second reveal in a row (it also skips the structure, as the first does now), WASM, the memory cost: a
view shown in part holds the structure's view and index twice (the structure and the derived one; counted in `memory`
as `structureBase`). The resident sizes of the rows are not comparable (swap).

## Byte identity

- `benchmarks/scaling/reveal_identity.py BEFORE AFTER`: the same session on both release builds (20 000 generated nodes;
  first views: node budget, depth, communities, communities with hubs, budget with communities), with partial reveals,
  reveal more, whole reveals, toggles, a compacting filter and a plain one: every answer line equal in all five views.
- `crates/runtime/tests/structure_cache.rs`: the same steps on a session (structure kept) and on the snapshot read back
  (structured anew) give equal snapshots, and the kept view equals `structured` of the source with the options (four
  option sets, among them a compacting filter).
