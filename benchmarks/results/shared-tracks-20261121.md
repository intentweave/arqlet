# Experimental `sharedTracks: "target"` (2026-11-21)

The edges into one node share one track in the last channel before it (option off by default). This file records the
quality of the option; the default output is byte-identical (86 cases: all 41 benchmark datasets with `defaultRouting`
`polyline` and `orthogonal`, four private inputs; 15 refinement fingerprint views).

Setup: Apple M3, macOS, loaded; release `layout-command`; single runs. Crossings count a horizontal and a vertical route
segment of two edges crossing strictly inside both, leaving out pairs of edges into the same node (the branches of a
shared track are no crossings). Inputs: two anonymized private 500-node graphs (ELK JSON with the explorer's options),
and the benchmark datasets.

| Input | Option | Time | Tracks drawn | Crossings | Bends | Length | Area |
| --- | --- | ---: | ---: | ---: | ---: | ---: | --- |
| deep (500 nodes, 1 505 edges, long edges) | off | 505 ms | 753 | 41 544 | 4 194 | 24 951 019 | 111 276 × 23 372 |
| deep | `target` | 514 ms | 306 | 43 544 (+4.8 %) | 4 194 | 20 947 611 (−16 %) | 111 276 × **12 692 (−46 %)** |
| dense (500 nodes, 3 138 edges, almost one layer) | off | 567 ms | 2 611 | | | | 314 162 × 63 424 |
| dense | `target` | 1 363 ms | 2 611 | | | | the same drawing (fallback) |

On the dense graph the shared bends make the lane orders cyclic for over a thousand edges (groups interleaved in the
narrow gaps of a 488-node layer); the layout is computed again without groups (diagnostic
`sharedTracks: … laid out without them`), so the option costs one extra attempt there.

Benchmark datasets (all 41): no error, no route through a node, none slower than 1.5× + 300 ms. 18 change; their area
is 94–100 % of the default. On the 13 smaller of them crossings 3 383 → 3 398 (+0.4 %: layerweave-code +4,
petri-net +4, synthetic-2k +7) and bends 3 936 → 3 796 (synthetic-2k −144, layerweave-code +4).

Judged in the order of the objectives (crossings before bends before length and area), the option trades a few
crossings for much less area on graphs with long edges into shared targets. It stays experimental: a group takes the
place of its first member in the channel order, not the place with the fewest crossings, which is the next step.
