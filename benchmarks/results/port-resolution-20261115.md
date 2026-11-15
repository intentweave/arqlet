# Port resolution in the repair: per-edge values computed once (2026-11-15)

Machine: Apple M3, macOS (Darwin 25.0.0), not idle (other work running; single-run times vary by up to 60 %).
Build: `cargo build --release -p arqlet-runtime --bin bench --features benchmark-timings`, revision da7bc02
(before) and the same plus the change in `crates/layout/src/ports.rs` (after), built and measured one after the other.
Command: `bench --graph benchmarks/datasets/mixed-large.json --limit 150 --reps 5 --threads none --layout-only 1`.

| | median layout | min |
|---|---|---|
| before | 1147 ms | 1139 ms |
| after | 914 ms | 901 ms |

Profile before (macOS `sample`, import of mixed-large): 96 % of the time in the geometric repair of the organic root,
53 % of it in `PortContext::resolve_nodes_into`, which a repair calls for every trial with a few nodes. Per edge and call it
built the chains of ancestors of both ends as vectors (`flow_side_pair`), looked the source's area mode up by id, cloned
the edge id twice for `end_sides`, and, when any node had a loop, computed the sides of every edge to count the load at
the loop nodes. Now the structural part of the flow sides (`flow_axis`), the source's area mode and the end sides are
computed once per edge in the context, the ends are read by index, and the loop corners are computed for the nodes asked
for only, from their own edges.

Output: byte-identical boxes and routes after the import and after a three-step move on mixed-large, mixed-layouts,
petri-net-shapes, self-loops, organic-50, organic-100 and can-bus; the default refinement fingerprints
(`fingerprints-20261115-stacked-default.jsonl`) identical on all 15 views; 852 tests.

Still there: the port resolution reads every node's box from the map and computes every absolute box in each call, and
looks ports up by string; the spatial hash sorts its candidates. A drag step on mixed-large repairs the whole drawing
(about 1 s a step); a repair local to the change is the next step.
