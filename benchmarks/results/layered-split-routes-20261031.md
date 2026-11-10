# Layered construction: routing edges across container borders in pieces (tried, reverted), 2026-10-31

Machine and method as in `layered-borders-20261031.md` (Apple M3, release, `bench --reps 3 --threads 1`, layerweave-code graph of
0bcfd56). Baseline: f5675c1 (border nodes; 13 layered datasets 1211 crossings, layerweave-code 941, deployment-cluster 117).
The change is in `layered-split-routes-20261031.patch` (its last variant). The code was restored to f5675c1; the geometry
fingerprint of layerweave-code equals the recorded one.

Idea (user, 2026-10-31): route an edge that crosses container borders in separate pieces, joined at fixed border points: from
the start port to the border point of each container it leaves, through the common area, and from the border point of each
container it enters to the end port. A border point lies on the container's border, on the lane of the edge's border node.
Each piece is searched with the constructor's crossing-aware search and must stay inside its container; when a piece fails,
the edge is searched as a whole.

| variant | crossings, 13 layered | layerweave-code | deployment-cluster | cloud-architecture | refinement layerweave-code |
|---|---:|---:|---:|---:|---:|
| baseline (border nodes, f5675c1) | 1211 | 941 | 117 | 23 | 2.69 s |
| pieces, split edges out of the port reordering | 1495 | 1221 | 141 | 27 | 2.96 s |
| + a track instead of the margin at border points | – | – | – | – | – |
| + split edges in the port reordering, pieces only when the port did not move (one run each) | – | 1045 | 123 | 23 | 2.84 s |

Pieces that succeed on layerweave-code's import: 95 of 186 attempts first, 143 of 248 with a track as the margin at border
points. Failures: the borders of nested containers lie about 26 units apart, which leaves no room for a lane change between
two border points with the routing margin on both sides (fixed by the track margin); the piece through the common area meets
tracks already taken; an edge that falls back loses the port reordering, which costs more than the pieces gain.

Reading: with border points fixed on the border nodes' lanes, every piece has two fixed ends, and the search cannot move the
crossing point along the border to where the routes around need it. Routing in pieces gave neither fewer crossings nor a
shorter search. A border point chosen together with the routes on both sides (or a border order that the parent's routing
decides first, as in the outside-in direction) would be needed.
