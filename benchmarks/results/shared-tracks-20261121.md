# Experimental `sharedTracks: "target"` (2026-11-21)

The edges into one node share one track in the last channel before it (option off by default). This file records the
quality of the option; the default output is byte-identical (86 cases: all 41 benchmark datasets with `defaultRouting`
`polyline` and `orthogonal`, two private inputs; 15 refinement fingerprint views).

Setup: Apple M3, macOS, loaded; release `layout-command`; single runs. Crossings count a horizontal and a vertical route
segment of two edges crossing strictly inside both, leaving out pairs of edges into the same node (the branches of a
shared track are no crossings). Inputs: the benchmark datasets. Also checked on two private evaluation graphs (not in the repository, figures not
published): on a graph with long edges into shared targets the option saves much height for a few more crossings; on a
dense flat graph the shared bends make the lane orders cyclic for many edges and the layout is computed again without
groups (diagnostic `sharedTracks: … laid out without them`), so the option costs one extra attempt there.

Benchmark datasets (all 41): no error, no route through a node, none slower than 1.5× + 300 ms. 18 change; their area
is 94–100 % of the default. On the 13 smaller of them crossings 3 383 → 3 398 (+0.4 %: layerweave-code +4,
petri-net +4, synthetic-2k +7) and bends 3 936 → 3 796 (synthetic-2k −144, layerweave-code +4).

Judged in the order of the objectives (crossings before bends before length and area), the option trades a few
crossings for much less area on graphs with long edges into shared targets. It stays experimental: a group takes the
place of its first member in the channel order, not the place with the fewest crossings, which is the next step.
