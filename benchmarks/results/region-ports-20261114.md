# Border ports for regions (`regionPorts`, experimental), 2026-11-14

Apple M3, Darwin 25.0.0 arm64, rustc 1.99.0, Node 24.21.0; release `layout-command` from the branch
`single-pass-regions` (ADR 0020, step S1-2). Command: `node benchmarks/eval/region-ports.mjs [--json]` (raw rows:
`region-ports-20261114.jsonl`); single runs, so the times are indicative only. The graphs are synthetic and deterministic
(generator and seeds in the script): a root flowing left to right through layered stages and region containers of
the named strategies, with edges into every region, out of it, one back edge and one edge passing it. The existing corpus
has almost no such case (only can-bus, which has buses and is left alone by the option). Metrics: the shared `metrics`
operation with node shapes. "Border ends off side": ends of edges that cross a region's border on a side other than the
one the root's flow enters (left) or leaves (right) through, of all such ends.

| shape | variant | nodes/edges | line–node hits | crossings | bends | area (k) | border ends off side | single pass | geometry issues | ms |
|---|---|---|---|---|---|---|---|---|---|---|
| organic-only | default | 27/37 | 0 | 12 | 52 | 1265 | 0/14 | no | 0 | 18 |
| organic-only | singlePassMixed | 27/37 | 0 | 12 | 34 | 1374 | 3/14 | yes | 0 | 21 |
| organic-only | regionPorts | 27/37 | 0 | 13 | 48 | 1261 | 0/14 | no | 0 | 25 |
| organic-only | singlePassMixed + regionPorts | 27/37 | 0 | 13 | 43 | 1374 | 0/14 | yes | 0 | 32 |
| tree-only | default | 25/28 | 0 | 13 | 70 | 943 | 0/14 | no | 0 | 4 |
| tree-only | singlePassMixed | 25/28 | 0 | 19 | 58 | 1046 | 2/14 | yes | 0 | 5 |
| tree-only | regionPorts | 25/28 | 0 | 9 | 69 | 1037 | 0/14 | no | 0 | 5 |
| tree-only | singlePassMixed + regionPorts | 25/28 | 0 | 9 | 65 | 1003 | 0/14 | yes | 0 | 5 |
| radial-only | default | 25/28 | 0 | 11 | 48 | 2019 | 0/14 | no | 0 | 6 |
| radial-only | singlePassMixed | 25/28 | 0 | 9 | 38 | 2190 | 3/14 | yes | 0 | 9 |
| radial-only | regionPorts | 25/28 | 0 | 8 | 48 | 1931 | 0/14 | no | 0 | 9 |
| radial-only | singlePassMixed + regionPorts | 25/28 | 0 | 8 | 42 | 2190 | 0/14 | yes | 0 | 9 |
| mixed-chain | default | 60/80 | 0 | 49 | 140 | 5814 | 2/36 | no | 0 | 95 |
| mixed-chain | singlePassMixed | 60/80 | 0 | 40 | 118 | 6068 | 11/36 | yes | 0 | 99 |
| mixed-chain | regionPorts | 60/80 | 0 | 43 | 128 | 5814 | 0/36 | no | 0 | 128 |
| mixed-chain | singlePassMixed + regionPorts | 60/80 | 0 | 43 | 116 | 7045 | 0/36 | yes | 0 | 149 |

Reading (after the port order, see below):

- Every variant with `regionPorts` keeps the border contract (no end off side); `singlePassMixed` alone has 2 to 11 off
  side, because its edges into regions are delegated and completed by the older router, which may run around the box
  and enter it from above or below.
- Against `singlePassMixed` alone: fewer crossings on tree-only (9 against 19) and radial-only (8 against 9), one more on
  organic-only (13 against 12) and three more on mixed-chain (43 against 40); bends 43 against 34, 65 against 58, 42
  against 38, 116 against 118. mixed-chain takes 16 % more area (the parent's order now follows the ports).
- Against the pipeline before single pass (default): fewer crossings on tree-only, radial-only and mixed-chain, one more
  on organic-only; fewer bends everywhere with `singlePassMixed`.
- Each layout runs twice (automatic ports, then ports at the nodes' places) and keeps the better by the published key,
  so it takes about twice as long.

The port order. Ports in the order of the nodes inside keep the inner legs straight and apart, but the parent's elements
(the stage nodes) were ordered without knowing them, since a region is one element of the parent's plan: the border
edges then crossed outside (mixed-chain: 21 crossings outside among edges of one region side). Tried and measured on these
graphs:

| port order | organic | tree | radial | mixed-chain (crossings / bends) |
|---|---|---|---|---|
| nodes inside, parent order as before | 13 | 10 | 11 | 46 / 112 |
| rank sum of inside and the first layout's order | 14 | 12 | 7 | 61 / 143 |
| per side, fewest estimated crossings (inner legs counted, outside as pairs against the first layout) | 13 | 12 | 12 | 46 / 115 |
| nodes inside, the parent's order seeing the region at its ports (kept) | 13 | 9 | 8 | 43 / 116 |

(`singlePassMixed + regionPorts`.) Kept: the hierarchical order of the parent sees the region at its explicit ports
(fixed sub-places, as it sees a container at its border nodes), and the placement takes that order as it is for the areas
that hold such a region (otherwise its own ordering, which sees every element at its middle, undoes it).

Open: regions inside regions, and regions at an organic root (S1-3).
