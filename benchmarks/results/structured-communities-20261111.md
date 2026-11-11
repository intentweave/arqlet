# Communities in the structured view (`communities`, 2026-11-11)

Apple M3, macOS 26.0, Rust 1.99.0, `--release` `layout-command`, branch `structured-communities` from `main` fd7e1df.
Times are single observations of one process per request on a machine with other work running, not measurements.

**What it does.** With `options: {communities: N}` on `import`, every container with more than N children (the root
included) is divided into the communities the cluster overview finds (`overview::with_communities`, Louvain). They are
containers of kind `community` in the view only (`sourceGraph` unchanged), named after their member with the most edges
(`s7 +165`; children without links: `unlinked (77)`), collapsed at first and opened with `toggle`. Nested communities
are collapsed too, so opening one shows its sub-communities. Edges a collapsed community aggregates are drawn once per
pair and direction.

Scenes of `import` and of `toggle` on the largest community. Columns: scene nodes, routes, size, crossing route pairs
(proper crossings of two routes without a shared end, counted with `crosses` of `packages/viewer/routes.js`), bends,
route segments through foreign nodes.

| graph | view | s | nodes | routes | size | crossing pairs | bends | through nodes |
|---|---|---|---|---|---|---|---|---|
| flat call graph (1 866) | without communities | 1.2 | 1866 | 3029 | 15340×77792 | 114 555 | 6868 | 0 |
| flat call graph (1 866) | communities 40, collapsed | 0.1 | 33 | 236 | 4620×2902 | 2 711 | 766 | 0 |
| flat call graph (1 866) | open `s7 +165` | 0.1 | 37 | 267 | 5756×3805 | 3 399 | 952 | 0 |
| Intentweave export (8 874) | communities 50, collapsed | 1.3 | 8469 | 288 | 5772×310606 | 78 | 632 | 0 |
| Intentweave export (8 874) | open `shared.ts +204` | 0.7 | 8491 | 293 | 6546×310751 | 77 | 672 | 0 |

The collapsed level of the flat graph is still dense: 236 aggregated edges (one per pair and direction) between 33
boxes cross 2 711 times. Fewer crossings than the full drawing, but hard to read; see the open points below.

Flat call graph: `python3 benchmarks/overview/generate.py --nodes 2000 --seed 1 --flat` (1 866 nodes, 3 047 edges, all
at the root). The export is deeply nested: only `directory:packages/index/src/queries` gets communities at 50, so the
view stays large; with such data communities belong together with a node budget or the overview.

**Sizes of collapsed communities.** Content-sized boxes (as under `visibleNodeBudget`) made the first view of the flat
graph 35 106 px wide; plain boxes 4 560. Opening one of the five largest top communities moved every other one in both
cases (they make room), with 2–17 of 462 axis pairs changing order with plain boxes and 3–10 with content-sized ones.
Plain boxes were kept.

**Known limits.** A direction, strategy or routing set *for* a community is refused (the source graph has no such
container). A change of the graph can regroup the communities (accepted for now); collapsed ids that no longer exist
are dropped.

## Aggregated edges by weight, both directions on one route

Chosen after step 2 (both lossless): every pair of visible ends that a collapsed community joins is drawn by one route
for both directions (`merge_aggregated_pairs`); `aggregateWeights` in the options record, per route, the edges it stands
for in its own direction and in the other, the scene reports them as `weight` and `backWeight`, and the SVG viewer draws
the route wider and more opaque with their number (logarithmically), with an arrow at the start when some run back, and
writes `4 ⇄ 7` on it. Every edge between communities is counted exactly once (test
`routes_between_communities_carry_their_edges_by_direction`).

Flat call graph (1 866), communities 40, same measurement as above:

| view | routes | size | crossing pairs | bends | through nodes |
|---|---|---|---|---|---|
| collapsed, one route per pair and direction (before) | 236 | 4620×2902 | 2 711 | 766 | 0 |
| collapsed, one route per pair (now) | 168 (68 both ways; for 476 edges) | 4148×2469 | 1 164 | 538 | 0 |
| open `s7 +165` (before) | 267 | 5756×3805 | 3 399 | 952 | 0 |
| open `s7 +165` (now) | 197 | 5170×3442 | 1 669 | 684 | 0 |

Route weights (both directions together): median 2, largest 16. In the playground the collapsed level is lighter, and
zoomed in the strong links stand out; near the ports of a box the routes still crowd (orthogonal routes of a dense
graph).

**Open points (from the SVG playground, step 2).** The collapsed level of a dense graph is a mesh of aggregated edges.
Weight and both directions on one route are done (above). Not chosen: showing only the strongest links (lossy).
Still open: routes crowding near the ports of a collapsed box.

## Domain groups and initial collapse

`groups` (node id → path of group names) adds containers of kind `group` before Louvain; `initialCollapse` collapses
containers by depth or kind on import. The flat call graph's example brings, as its groups, the directories and the
file each symbol came from (the nested graph of the same generator and seed). With *Domain groups* on and no
communities, the first view in the playground is 3 collapsed directories and 3 routes (one of them `47 ⇄ 145`);
opening `d1` shows its subdirectories and files collapsed, with their weighted routes. Checked by eye in Chrome; tests:
`given_groups_replace_louvain_where_the_domain_says_and_open_level_by_level`,
`louvain_divides_what_the_given_groups_leave_large`, `an_initial_collapse_shows_containers_collapsed_by_depth_or_kind`.
Seen: weight labels of routes that meet at one point can overlap (SVG viewer).

## Path cut-out, steps 1–3

Flat call graph with domain groups, everything collapsed: `trace` from `f0/s0` to `f31/s3` finds one shortest path of 5
steps (6 nodes, 5 edges); in the first view it runs through 2 of the 3 collapsed directories and the route between
them, drawn in amber. `reveal` of the path's nodes opens the directories and files that hold them (56 visible nodes:
an opened file shows all its functions), after which the 6 path nodes and their 5 edges are highlighted and, with
*Fade others*, the rest recedes by distance. Checked by eye in Chrome (browser-only mode).

## Partial reveal

`reveal` opens the containers above the nodes asked for in part by default: only the children on the way are shown, the
others wait in a collapsed `+N more` box. The same 5-step path (`f0/s0` → `f31/s3`, domain groups): 17 visible nodes
instead of 56 with whole containers; the path's 6 nodes stand in their files beside dashed rests (`+14 more`,
`+6 more`, …). Opening a rest shows the whole container. Checked in Chrome; tests
`a_partial_reveal_shows_only_the_nodes_asked_for_and_a_rest`, `a_path_revealed_in_part_keeps_the_view_small`.

