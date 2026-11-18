# A session's projection by index (2026-11-18)

Follow-up of [sessions-prepared-20261118.md](sessions-prepared-20261118.md) (ADR 0019 decision 4, interned ids, for the
projection). Machine: Apple M3 (8 cores, 16 GB), macOS (Darwin 25.0.0), load average 3 to 4 during both runs (single
runs; quieter than the earlier files, so compare within this table only). Build:
`cargo build --release -p arqlet-runtime --bin layout-command`; before: branch `prepared-source` (`ec5145a`), after:
branch `indexed-projection`. Command: `python3 benchmarks/scaling/sessions.py --sizes 300000 1000000` (from #34), the
baseline run with the `prepared-source` binary right before. Raw rows: `sessions-indexed-20261118.jsonl` (before first,
then after).

| nodes | message | before | after |
|---|---|---|---|
| 300 000 | import | 5.7 s | 5.5 s |
| 300 000 | toggle, with scene | 1.5 s | 1.2 s |
| 300 000 | toggle back, no scene | 1.4 s | 1.1 s |
| 1 000 000 | import | 21.5 s | 21.0 s |
| 1 000 000 | toggle, with scene | 3.9 s | 2.5 s |
| 1 000 000 | toggle back, no scene | 3.3 s | 2.0 s |

Resident memory of the session process at 1M: 1.8 GiB before and after (the index adds about one entry per node and
edge; within the run-to-run variation here).

What changed (byte-identical: imports of every benchmark dataset and fixture, and session imports, toggles, an export
and a one-shot toggle at 20K and 100K with three sets of options, compared with `ec5145a`; 15 refinement fingerprint
views): `arqlet_model::GraphIndex` holds a graph's nodes by dense index (id → index, parent and container by index, each
edge's ends by index). The structure kept with a snapshot keeps the index of its structured graph, built once;
`arqlet_view::project_indexed` walks it: the shown node of every node in one pass over arrays, the visible nodes by flag
instead of a set of copied ids, and an edge copied only when the view keeps it (edges inside a collapsed container were
copied and dropped). The edge-port bindings are mapped only when the graph has any. `project` is `validate` and the same
indexed projection, checked against the first-written projection in a test.

Where a session toggle at 1M goes now (`sample`): the layout of the view about 68 %, the projection about 16 %.
The import is still 21 s at 1M: validation, the structured view with its communities, the first view's selection and
JSON, all on string keys once per session.
