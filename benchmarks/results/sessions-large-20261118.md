# Resident sessions on large graphs: what ADR 0019 still buys (2026-11-18)

Machine: Apple M3 (8 cores, 16 GB), macOS (Darwin 25.0.0), loaded (load average 15 to 17; single runs, indications).
Build: `cargo build --release -p arqlet-runtime --bin layout-command` on `a836a37` (main after #29). Command:
`python3 benchmarks/scaling/sessions.py --sizes 20000 100000 300000 1000000`: one `layout-command --sessions` process
per size, the graphs and first view of [scaling-series-20261118.md](scaling-series-20261118.md). Raw rows: `sessions-20261118T180412Z.jsonl`.

| nodes | message | time | sent | answer | one-shot request, for comparison |
|---|---|---|---|---|---|
| 100 000 | import (first view, with scene) | 2.0 s | 19.5 MB | 1.7 MB | 2.1 s, answer 20.6 MB |
| 100 000 | toggle, with scene | 1.0 s | 61 B | 1.7 MB | 1.4 s, 22.9 MB sent, 20.6 MB back |
| 300 000 | import | 5.9 s | 59 MB | 1.9 MB | 5.9 s, answer 58 MB |
| 300 000 | toggle, with scene | 2.7 s | 61 B | 2.1 MB | 3.7 s, 64.6 MB sent, 58.5 MB back |
| 1 000 000 | import | 22.0 s | 201 MB | 2.2 MB | 22.8 s, answer 187 MB |
| 1 000 000 | toggle, with scene | 8.5 s | 61 B | 2.6 MB | 14.1 s, 208 MB sent, 189 MB back |
| 1 000 000 | toggle back, no scene | 8.1 s | 77 B | 0.4 MB | – |
| 1 000 000 | columns | 0.07 s | – | 0.2 MB | – |

Resident memory of the session process at 1M: 1.6 GiB (one-shot processes peaked at 3.1 to 3.3 GiB).

## Where a toggle's time goes in a session (1M, `sample` over three toggles)

Inclusive shares, overlapping: `structured` (the view's communities and hierarchy on the whole source graph, done in
`toggle_container` and again in `project_view`) 44 %, of it the community containers 31 %; `arqlet_view::project`
on the whole source graph 23 %; cloning and dropping the graph about 25 % (the session hands a clone of its snapshot to
every operation); `validate` 10 %; the layout of the view 16 %. The scene is a small part (with and without it: 8.5 and
8.1 s).

## What is open in ADR 0019 after #29, and its share on large graphs

| open item | share at 1M | note |
|---|---|---|
| Resident *prepared* source: structure (communities) and projection index kept per source revision, no clone per operation | about 75 % of a toggle (8.5 s → about 2 s expected, not measured) | not named as a step; part of step 3 "resident state" |
| Interned ids for the source graph (decision 4) | import: validation, structure, projection and JSON on string keys, most of the 22 s | large change; helps import and every toggle |
| Columnar input / binary frames (step 5) | the import message (201 MB JSON, parsed once per session) | only the import; Arrow is a dependency decision |
| Size limit of the session store (step 3) | 1.6 GiB per 1M session; the backend allows 32 sessions | safety, cheap |
| Deltas instead of whole scenes (step 3) | under 5 %: the answer is 2.6 MB at 1M | matters for large views, not large graphs |
| Typed fields of the read operations (step 1) | none | hygiene |
| Shared memory (step 6) | none for a static large graph | live positions, progressive publications |

Not measured: the browser (WASM memory is 32-bit; a 1M session needing 1.6 GiB natively is near its limit) and frame time.
