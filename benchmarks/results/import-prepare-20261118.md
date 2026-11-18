# The import of a large graph, byte-identical speedups (2026-11-18)

Machine: Apple M3 (8 cores, 16 GB), macOS (Darwin 25.0.0), loaded (load average 8 to 23; single runs, before and after
right after each other). Build: `cargo build --release -p arqlet-runtime --bin layout-command`; before: branch
`indexed-projection` (#36), after: branch `import-prepare`. Request: the 1 000 000-node graph of
`benchmarks/overview/generate.py` (seed 1, 1.6M edges), `{op: "import", limit: 10000, options: {levelOfDetail: {nodes:
1000, edges: 3000}, liftEdges: true}}`, one `layout-command` process (200 MB of JSON in, 187 MB out).

| step | import at 1M |
|---|---|
| before | 22.5 s |
| the selection's structure reused | 17.8 s |
| + linear `validate`, request read in one pass | 15.8 s (22.1 s before in that run) |

Peak memory 3.9 GiB before and after.

What changed, all with byte-identical answers (imports of every benchmark dataset and fixture; session imports, toggles,
an export and a one-shot toggle at 20K and 100K with three sets of options; the 1M import above; 15 refinement fingerprint
views; the protocol test that compares 68 requests, refused ones included, with the earlier dispatcher):

1. An abstracting import above its limit structured the graph (communities by Louvain) for the selection and again for
   the view; the second time reuses the first when the selection keeps every node (the graph is then the same), and so
   do the containers the first view shows collapsed.
2. The selection walks every selected node's ancestors only up to one selected already, and keeps the edge-port bindings
   by a set instead of a search per binding.
3. `validate`: the containment check stops at a node whose walk ended well already (linear instead of a walk to the top
   from every node, the same first error; a test compares it with the full walk), and the edge-port bindings find their
   edge by a map.
4. Requests: a typed field is read from the JSON value in place, not from a copy of it; an `import` whose text is well
   formed is read straight into its types in one pass (otherwise the general way, with the errors it always gave).

What remains of the 16 s at 1M: string-keyed work on the whole graph (validation twice, for the source and the derived
structure; communities; the first view's collapse and budget; writing the 187 MB answer, which carries the source graph)
and the layout of the view (about a tenth).
