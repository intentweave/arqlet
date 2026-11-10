# Ports in the order of the planned ways (tried, reverted), 2026-10-31

Machine: Apple M3, 8 logical CPUs, macOS 25.0.0, release, bench with `--features benchmark-timings`. Revision 029b5e0 plus the change in
`port-coupling-20261031.patch`. `bench --reps 3 --threads 1`, medians; limit 100 (tree-100: 1000). Raw rows:
`port-coupling-20261031.jsonl`. Compared with `all-datasets-off-20261031.jsonl` and `joint-order-20261031.jsonl` (029b5e0).

Change tried (behind `layeredConstruction`): after the first planned ways, the constructed edges of one node side kept their port slots
but took them in the order of the lane they turn to first (source) or come from last (target); the ways were then planned again, and
the corridors ordered as before. The published ports took the ends of their routes, so ports and routes stayed consistent (the
integration tests passed).

| dataset | refinement ms: off / joint / ports | grid pops | crossings | length | bends |
|---|---|---|---|---|---|
| uml-class | 38 / 44 / 187 | 0 / 0 / 0 | 8 / 7 / 6 | 9782 / 11170 / 11112 | 49 / 55 / 55 |
| layerweave-code | 7189 / 2944 / 3330 | 2251396 / 909875 / 911940 | 1327 / 1706 / 1747 | 238956 / 286554 / 289478 | 748 / 551 / 567 |
| petri-net | 137 / 78 / 82 | 562 / 4158 / 3833 | 67 / 105 / 105 | 12988 / 13842 / 13842 | 126 / 114 / 114 |
| other layered datasets | within ±9 ms of joint | equal or +3 % | equal or −1 | equal | equal |

Totals over 13 layered datasets (off / joint / ports): refinement 8.17 / 3.80 / 4.35 s; grid pops 2.32 M / 1.02 M / 1.02 M; crossings
1615 / 2051 / 2089; length 415 k / 462 k / 465 k; bends 1530 / 1326 / 1342.

Reading: reordering the ports of the constructed edges changes nothing measurable in the crossings and makes the refinement 15 % slower.
After the corridor order, the dummies, the passages and now the ports, the next step should be a diagnosis of where the crossings arise
(between which kinds of routes, on which kind of segment), rather than another change.
