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

## Second round: boxes kept between trials, sort keys once, port ids once

Same machine and command, measured one after the other (before: the first round and the drag-step change committed;
after: this change in `crates/layout/src/ports.rs` and `crates/layout/src/repair.rs`):

| | median layout | min |
|---|---|---|
| boxes kept between trials (alone) | 894 → 854 ms | 890 → 852 ms |
| all of the second round | 893 → 808 ms | 892 → 796 ms |

- The port resolution keeps the boxes and absolute boxes of its last call; the repair names the nodes whose boxes changed
  since (the node the last trial moved, which it put back or committed, and the node it moves now), and after a rebuild
  of its cache every box is read again. Only those nodes and their descendants are computed again, in the same order of
  additions.
- The keys that order the ports of a node side are computed once per port instead of in every comparison of the sort (they
  look their peers up by id), and the sides are grouped by node index instead of a copy of the node id.
- The ids of automatic ports are formatted once per edge end in the context.

Output byte-identical on the seven organic datasets after import and a three-step move; default fingerprints identical on
all 15 views; 852 tests. From the start of this work: mixed-large 1147 → 808 ms (about 30 %).

Left: the checks of straight segments against outlines (`through_item_with`, `Outline::segment_enters`: about 40 % now)
are cheap each but many; making them cheaper without changing a result is not obvious (a squared distance instead of
`hypot` for discs could differ in the last bit at the boundary).
