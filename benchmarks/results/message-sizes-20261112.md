# Message sizes of the JSON protocol (2026-11-12)

Apple M3, macOS 26.0, Rust 1.99.0, `--release` `layout-command`, `main` 24c1778. Sizes in bytes of the JSON as sent
(`json.dumps`, no whitespace); no timing.

Graphs: `python3 benchmarks/overview/generate.py --nodes N --seed 1` (directories, files, symbols, calls). The snapshot
is an import with `initialCollapse: {depth: 1}`, so the view shows only a few boxes.

| nodes | edges | graph | snapshot (visible boxes) | one `toggle` request | `overview` response |
|---|---|---|---|---|---|
| 2 000 | 3 047 | 0.37 MB | 0.54 MB (25) | 0.54 MB | 0.13 MB |
| 10 000 | 15 205 | 1.88 MB | 3.86 MB (105) | 3.86 MB | 0.72 MB |

- Every edit sends the whole snapshot and gets a whole snapshot back, however small the view: the snapshot holds the
  source graph, the projected graph, boxes, routes and plans.
- The overview's response is columnar (parallel arrays per node) and about 5× smaller than the snapshot for the same
  graph.
- Growth is about linear in the graph; at 100 000 nodes an edit would carry tens of megabytes each way. That is an
  extrapolation, not a measurement.
