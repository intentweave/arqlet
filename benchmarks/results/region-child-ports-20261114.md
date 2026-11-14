# Layered containers in an organic area (`regionChildPorts`, experimental), 2026-11-14

Apple M3, Darwin 25.0.0 arm64, rustc 1.99.0, Node 24.21.0; release `layout-command` from the branch
`single-pass-regions` (ADR 0020, step S1-3). Command: `node benchmarks/eval/region-child-ports.mjs [--json]` (raw rows:
`region-child-ports-20261114.jsonl`); single runs, times indicative only. Graphs: 12 synthetic organic roots (generator and
seeds in the script: four layered containers of both axes, flat or with a nested container of the other axis, loose
nodes, random edges) and the playground examples mixed-layouts and mixed-large. Metrics: the shared `metrics` operation
with node shapes. "Ends off the axis": ends of edges that leave a layered child of the root across its flow's axis (top or
bottom of a left-to-right container), of all such ends.

| graph | variant | line–node hits | crossings | bends | overlaps | other geometry issues | ends off the axis | ms |
|---|---|---|---|---|---|---|---|---|
| organic-root flat 1 | straightDetours | 0 | 18 | 0 | 0 | 0 | 7/9 | 158 |
| organic-root flat 1 | straightDetours + regionChildPorts | 0 | 5 | 10 | 0 | 0 | 1/9 | 49 |
| organic-root flat 1 | singlePassMixed + straightDetours | 0 | 3 | 0 | 0 | 0 | 3/9 | 68 |
| organic-root flat 1 | singlePassMixed + straightDetours + regionChildPorts | 0 | 5 | 15 | 0 | 0 | 0/9 | 69 |
| organic-root flat 2 | straightDetours | 0 | 0 | 0 | 0 | 0 | 5/6 | 37 |
| organic-root flat 2 | straightDetours + regionChildPorts | 0 | 3 | 12 | 0 | 0 | 0/6 | 293 |
| organic-root flat 2 | singlePassMixed + straightDetours | 0 | 0 | 0 | 0 | 0 | 5/6 | 51 |
| organic-root flat 2 | singlePassMixed + straightDetours + regionChildPorts | 0 | 0 | 14 | 0 | 0 | 0/6 | 40 |
| organic-root flat 3 | straightDetours | 0 | 4 | 2 | 0 | 0 | 9/15 | 3312 |
| organic-root flat 3 | straightDetours + regionChildPorts | 0 | 6 | 23 | 0 | 0 | 0/15 | 1641 |
| organic-root flat 3 | singlePassMixed + straightDetours | 0 | 5 | 3 | 0 | 0 | 8/15 | 3091 |
| organic-root flat 3 | singlePassMixed + straightDetours + regionChildPorts | 0 | 5 | 29 | 0 | 0 | 0/15 | 2844 |
| organic-root flat 4 | straightDetours | 0 | 3 | 0 | 0 | 0 | 7/14 | 553 |
| organic-root flat 4 | straightDetours + regionChildPorts | 0 | 5 | 27 | 0 | 0 | 1/14 | 318 |
| organic-root flat 4 | singlePassMixed + straightDetours | 0 | 4 | 1 | 0 | 1 | 8/14 | 1174 |
| organic-root flat 4 | singlePassMixed + straightDetours + regionChildPorts | 0 | 6 | 26 | 0 | 0 | 0/14 | 1318 |
| organic-root flat 5 | straightDetours | 0 | 2 | 1 | 0 | 0 | 2/7 | 677 |
| organic-root flat 5 | straightDetours + regionChildPorts | 0 | 1 | 15 | 0 | 0 | 0/7 | 21 |
| organic-root flat 5 | singlePassMixed + straightDetours | 0 | 2 | 0 | 0 | 0 | 5/7 | 78 |
| organic-root flat 5 | singlePassMixed + straightDetours + regionChildPorts | 0 | 4 | 14 | 0 | 0 | 0/7 | 20 |
| organic-root flat 6 | straightDetours | 0 | 11 | 0 | 0 | 0 | 9/11 | 85 |
| organic-root flat 6 | straightDetours + regionChildPorts | 0 | 3 | 16 | 0 | 0 | 1/11 | 128 |
| organic-root flat 6 | singlePassMixed + straightDetours | 0 | 7 | 0 | 0 | 0 | 10/11 | 93 |
| organic-root flat 6 | singlePassMixed + straightDetours + regionChildPorts | 0 | 2 | 14 | 0 | 0 | 0/11 | 85 |
| organic-root nested 1 | straightDetours | 0 | 2 | 6 | 0 | 0 | 6/9 | 3428 |
| organic-root nested 1 | straightDetours + regionChildPorts | 0 | 6 | 28 | 0 | 0 | 0/9 | 3510 |
| organic-root nested 1 | singlePassMixed + straightDetours | 0 | 8 | 3 | 0 | 1 | 8/9 | 3496 |
| organic-root nested 1 | singlePassMixed + straightDetours + regionChildPorts | 1 | 8 | 35 | 0 | 0 | 0/9 | 2364 |
| organic-root nested 2 | straightDetours | 0 | 2 | 4 | 0 | 0 | 5/6 | 1544 |
| organic-root nested 2 | straightDetours + regionChildPorts | 0 | 1 | 20 | 0 | 0 | 0/6 | 1702 |
| organic-root nested 2 | singlePassMixed + straightDetours | 3 | 6 | 1 | 0 | 0 | 5/6 | 2272 |
| organic-root nested 2 | singlePassMixed + straightDetours + regionChildPorts | 4 | 0 | 18 | 0 | 0 | 0/6 | 1500 |
| organic-root nested 3 | straightDetours | 0 | 6 | 6 | 0 | 0 | 11/15 | 3068 |
| organic-root nested 3 | straightDetours + regionChildPorts | 0 | 12 | 47 | 0 | 0 | 1/15 | 3160 |
| organic-root nested 3 | singlePassMixed + straightDetours | 2 | 17 | 5 | 0 | 0 | 13/15 | 2954 |
| organic-root nested 3 | singlePassMixed + straightDetours + regionChildPorts | 3 | 8 | 47 | 0 | 0 | 0/15 | 2812 |
| organic-root nested 4 | straightDetours | 0 | 12 | 10 | 0 | 0 | 10/14 | 1469 |
| organic-root nested 4 | straightDetours + regionChildPorts | 0 | 7 | 46 | 0 | 0 | 0/14 | 1661 |
| organic-root nested 4 | singlePassMixed + straightDetours | 1 | 3 | 4 | 0 | 0 | 10/14 | 1086 |
| organic-root nested 4 | singlePassMixed + straightDetours + regionChildPorts | 2 | 11 | 42 | 0 | 0 | 0/14 | 2740 |
| organic-root nested 5 | straightDetours | 0 | 2 | 6 | 0 | 0 | 4/7 | 1841 |
| organic-root nested 5 | straightDetours + regionChildPorts | 0 | 1 | 33 | 0 | 0 | 0/7 | 2019 |
| organic-root nested 5 | singlePassMixed + straightDetours | 2 | 2 | 2 | 0 | 0 | 5/7 | 1440 |
| organic-root nested 5 | singlePassMixed + straightDetours + regionChildPorts | 1 | 6 | 23 | 0 | 0 | 0/7 | 2033 |
| organic-root nested 6 | straightDetours | 0 | 6 | 7 | 0 | 0 | 6/11 | 2277 |
| organic-root nested 6 | straightDetours + regionChildPorts | 0 | 4 | 32 | 0 | 0 | 1/11 | 2508 |
| organic-root nested 6 | singlePassMixed + straightDetours | 3 | 4 | 1 | 0 | 0 | 6/11 | 2650 |
| organic-root nested 6 | singlePassMixed + straightDetours + regionChildPorts | 2 | 10 | 20 | 0 | 0 | 1/11 | 2214 |
| mixed-layouts | straightDetours | 0 | 5 | 0 | 0 | 0 | 2/2 | 20 |
| mixed-layouts | straightDetours + regionChildPorts | 0 | 7 | 4 | 0 | 0 | 0/2 | 23 |
| mixed-layouts | singlePassMixed + straightDetours | 0 | 5 | 0 | 0 | 0 | 2/2 | 20 |
| mixed-layouts | singlePassMixed + straightDetours + regionChildPorts | 0 | 7 | 4 | 0 | 0 | 0/2 | 23 |
| mixed-large | straightDetours | 0 | 811 | 68 | 0 | 10 | 37/82 | 7587 |
| mixed-large | straightDetours + regionChildPorts | 0 | 901 | 254 | 0 | 7 | 9/82 | 8607 |
| mixed-large | singlePassMixed + straightDetours | 4 | 954 | 120 | 4 | 9 | 31/82 | 12270 |
| mixed-large | singlePassMixed + straightDetours + regionChildPorts | 2 | 1145 | 336 | 3 | 4 | 10/82 | 10204 |

Sums over the 12 synthetic graphs:
- straightDetours: hits 0, crossings 68, bends 42, overlaps 0, ends off the axis 81/124
- straightDetours + regionChildPorts: hits 0, crossings 54, bends 309, overlaps 0, ends off the axis 5/124
- singlePassMixed + straightDetours: hits 11, crossings 61, bends 20, overlaps 0, ends off the axis 86/124
- singlePassMixed + straightDetours + regionChildPorts: hits 13, crossings 65, bends 297, overlaps 0, ends off the axis 1/124

Reading:

- The border contract holds with the option: 5 of 124 ends off the axis on the synthetic graphs (81 without), 1 of 124
  with `singlePassMixed`; mixed-large 9 of 82 (37 without).
- Crossings: fewer on the synthetic graphs (54 against 68), more on mixed-large (901 against 811, and with
  `singlePassMixed` 1145 against 954).
- Bends: about seven times as many (309 against 42 on the synthetic graphs). Most are inherent to the contract: a leg that
  leaves a container along its flow and runs on straight to an end elsewhere in the organic area bends once where it
  leaves; legs whose other end lies across the flow go around the container's corner (one or two more), and legs that
  cannot run straight to the border follow the lanes of their chain. The order of the objectives puts lines through nodes
  and crossings before bends; against the bends this is a choice for predictable borders (drill-down, ADR 0020), not a
  measured quality gain, so the option stays off.
- No lines through nodes on the synthetic graphs and mixed-large without `singlePassMixed`; with it the counts are those
  of `singlePassMixed` (lines of edges this option does not touch).

How it works and what was tried:

- A container with a fixed flow inside an organic or radial area gets border nodes in its own plan (as a container of the
  same axis as its parent does). The side of each border node (start or end of the flow) is the side that faces the
  edge's other end in the area's abstract placement: the pivot MDS of the area's children over their links, where the
  organic placement starts. A correction pass from the drawn sides (a second layout from the first) changed the bends
  hardly at all and doubled the time on mixed-large, and was left out.
- After the layout, each such edge's straight route is replaced by: a leg inside the container from the node (a free
  straight lane to the border where there is one, else along the chain's dummies, changing lanes in the free channels
  between ranks), and a leg from the container's border to the other end: straight where free, else around one or two
  corners of the container. Where none of these is free, the edge keeps its route.
- A first attempt without border nodes (ports set on the container after the layout, inner legs drawn geometrically) left
  most ends off the axis on mixed-large (63 of 135) at more crossings and bends, because of the nested containers.
