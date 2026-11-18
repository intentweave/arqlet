# Scaling series 20K to 1M with an abstract start (2026-11-18)

Scaling step 3 (roadmap section 7). Machine: Apple M3 (8 cores, 16 GB), macOS (Darwin 25.0.0), **heavily loaded** (load
average 18 to 23 throughout; single runs: the times are indications, and the overview, unchanged code, took 32 s and 44 s
at 1M in the two runs). Build: `cargo build --release -p arqlet-runtime --bin layout-command`, branch `scaling-series` on
`47811a6`. Command: `python3 benchmarks/scaling/series.py --sizes 20000 50000 100000 300000 1000000` (graphs of
`benchmarks/overview/generate.py`, shaped like a code index, seed 1). Raw rows: `scaling-20261118T170942Z.jsonl` (before
the projection fixes below) and `scaling-20261118T172408Z.jsonl` (after).

Requests, each one `layout-command` process (JSON parse and write included):

- **overview**: the cluster overview, 4 threads.
- **first**: the first structured view, `levelOfDetail: {nodes: 1000, edges: 3000}`, `liftEdges: true`, `limit` 10 000.
- **open**: `toggle` of the first collapsed container that view shows (the user's first click).

Peak memory is the process's maximum resident set (`/usr/bin/time -l`). *Boxes / edges* is what the view shows.

| nodes | edges | request | before | after | peak memory | request | response | boxes / edges |
|---|---|---|---|---|---|---|---|---|
| 20 000 | 30 212 | overview | 0.51 s | 1.13 s | 147 MiB | 3.8 MB | 1.6 MB |  |
| 20 000 | 30 212 | first | 0.66 s | 0.64 s | 172 MiB | 3.8 MB | 5.3 MB | 1000 / 1842 |
| 20 000 | 30 212 | open | 0.55 s | 0.51 s | 228 MiB | 5.9 MB | 5.3 MB | 1001 / 1843 |
| 50 000 | 75 856 | overview | 1.31 s | 2.02 s | 287 MiB | 9.6 MB | 4.2 MB |  |
| 50 000 | 75 856 | first | 1.34 s | 1.23 s | 350 MiB | 9.6 MB | 10.9 MB | 1000 / 2138 |
| 50 000 | 75 856 | open | 1.02 s | 0.84 s | 432 MiB | 12.2 MB | 11.0 MB | 1028 / 2194 |
| 100 000 | 151 865 | overview | 2.72 s | 4.52 s | 717 MiB | 19.5 MB | 8.6 MB |  |
| 100 000 | 151 865 | first | 2.67 s | 2.08 s | 690 MiB | 19.5 MB | 20.6 MB | 1000 / 2600 |
| 100 000 | 151 865 | open | 1.97 s | 1.36 s | 783 MiB | 22.9 MB | 20.6 MB | 1003 / 2605 |
| 300 000 | 453 770 | overview | 9.28 s | 12.79 s | 1.48 GiB | 59.2 MB | 27.1 MB |  |
| 300 000 | 453 770 | first | 10.03 s | 5.92 s | 1.72 GiB | 59.2 MB | 58.0 MB | 576 / 3000 |
| 300 000 | 453 770 | open | 7.93 s | 3.72 s | 1.51 GiB | 64.6 MB | 58.5 MB | 657 / 3306 |
| 1 000 000 | 1 511 361 | overview | 32.39 s | 44.29 s | 3.06 GiB | 200.7 MB | 93.0 MB |  |
| 1 000 000 | 1 511 361 | first | 71.21 s | 22.83 s | 3.21 GiB | 200.7 MB | 187.3 MB | 402 / 3000 |
| 1 000 000 | 1 511 361 | open | 73.67 s | 14.13 s | 3.24 GiB | 208.1 MB | 188.6 MB | 548 / 3719 |

## Fixes on the way (byte-identical)

The first profile of the 300K first view showed almost no layout: 37 % in `arqlet_view::project`, which checked for every
collapsed container that it has children by a search of all nodes (15 200 collapsed containers × 300 000 nodes), and
walked from every node and edge end to the root by id lookups. Now the containers are found once and every node's shown
node is computed once from its parent's; the edge-port bindings find their edges by a map instead of a search, and two
more searches of all nodes per id in the runtime (pulled containers at import, pins) use a set. The 300K first view on
its own: 24 s → 6.4 s (same output, compared byte for byte); the imports of every benchmark dataset and fixture are
unchanged, as are the 15 refinement fingerprint views.

## Findings

- **Every size works with an abstract start**, up to 1M nodes and 1.6M edges, with no line–node issues and almost no
  route left to the fallback router (0 to 5). The first view is within its budget at every size (at 300K and 1M the edge
  budget binds: 576 and 402 boxes for 3 000 edges).
- **Time and memory now grow about linearly with the graph, not with the view**: the first view takes 0.6 s at 20K and
  23 s at 1M for a view of the same size. What remains is the work on the whole source graph per request (validation,
  the structured view with its communities, the projection, JSON), done with string keys; the layout of the view is a
  small part (about 16 % at 300K).
- **Opening a container costs a new request with the whole snapshot**: at 1M the request is 208 MB (the snapshot carries
  the source graph, 187 MB as JSON) and takes 14 s, though the view changes by one container. That is the transfer and
  re-preparation ADR 0019 (resident sessions, columns) is for.
- **Memory**: 3.1 to 3.3 GiB at 1M for every request, about 3.3 KB per node; a browser tab with the WASM engine would
  need the same order (not measured here).
- The overview at 1M takes 32 to 44 s on this loaded machine (4 threads) and answers 93 MB; its levels computed when
  opened (roadmap step 6) would bound that.

## Next levers

1. Resident sessions (ADR 0019): keep the source graph and its structured view in the engine, so opening a container
   sends an id and receives what changed, instead of 200 MB each way.
2. An index-based source graph for the per-request preparation (validation, structure, projection) instead of string
   maps.
3. Communities and structure computed once per source revision and reused by later views.
