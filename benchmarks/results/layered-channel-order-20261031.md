# Layered construction: ordering the built routes channel by channel (tried, reverted), 2026-10-31

Machine: Apple M3, 8 logical CPUs, macOS 25.0.0, release, bench with `--features benchmark-timings`. Revision e09206c plus
the change in `layered-channel-order-20261031.patch` (behind `layeredConstruction`; default output unchanged). `bench --reps 3 --threads 1`, medians; limit 100
(er-diagram and tree-100: 1000); layerweave-code is the graph of 0bcfd56 (669 / 1084). Raw rows:
`layered-channel-order-20261031.jsonl`. "before" is `layered-ports-loops-20261031.jsonl` (e09206c). The machine was not idle.

Change (step 1 of the plan in `port-key-variants-20261031.md`): after the construction, the built routes are ordered in their
channels, as an orthogonal router orders the segments that share a channel after routing. A channel is a node side (port
slots), a corridor (tracks across the flow) or a lane (tracks along the flow); corridors and lanes are the intervals between
neighbouring box edges of the drawing. Moves, accepted while the count of crossings, close parallel runs (closer than the
track spacing) and obstacle hits of the constructed routes falls:
1. neighbours in a channel exchange their coordinates (the set of coordinates of a channel stays, so tracks keep their
   spacing);
2. two edges that cross or run too close exchange their coordinates in a prefix of the channels they share;
3. two neighbouring edges of a node side that cross or run too close exchange their port slots and are searched again around
   all other built routes.

Tried first and dropped: a plan of all ports and tracks over the gaps' centres before building (petri-net 81 → 89 → 104
crossing points with the planned routes taken first and then the better of planned and searched). The planned ways are worse
than the ways the crossing-aware construction finds edge by edge.

Effect on the import (same placement, construction only; all crossing points, `benchmarks/eval/crossing-diagnosis.py`
style, e09206c against this state): 2753 → 2301 (−16 %) over 16 datasets; layerweave-code 1998 → 1645, network-vlan
145 → 101, deployment-cluster 184 → 164, state-tcp 11 → 5, petri-net 136 → 119. No dataset rises.

Effect after the refinement:

| dataset | refinement ms: off / before / now | grid pops | crossings | length | bends | hard |
|---|---|---|---|---|---|---|
| petri-net | 137 / 63 / 191 | 562 / 667 / 441 | 67 / 92 / 102 | 12988 / 13616 / 13688 | 126 / 106 / 104 | 0 / 0 / 0 |
| er-diagram | 21 / 52 / 53 | 1340 / 0 / 0 | 4 / 1 / 1 | 3658 / 4242 / 4242 | 33 / 32 / 32 | 0 / 0 / 0 |
| state-order | 13 / 5 / 5 | 0 / 0 / 0 | 0 / 0 / 0 | 2775 / 2762 / 2762 | 26 / 25 / 25 | 0 / 0 / 0 |
| activity-order | 113 / 112 / 107 | 17156 / 32463 / 32463 | 70 / 70 / 70 | 20757 / 19982 / 19982 | 108 / 102 / 102 | 0 / 0 / 0 |
| uml-class | 38 / 32 / 32 | 0 / 0 / 0 | 8 / 7 / 7 | 9782 / 9734 / 9734 | 49 / 51 / 51 | 0 / 0 / 0 |
| bpmn-pools | 31 / 26 / 28 | 0 / 738 / 738 | 8 / 7 / 7 | 10750 / 9923 / 9923 | 46 / 44 / 44 | 0 / 0 / 0 |
| network-vlan | 130 / 119 / 123 | 16345 / 21891 / 21595 | 71 / 72 / 72 | 28667 / 24832 / 24832 | 136 / 118 / 118 | 0 / 0 / 0 |
| layerweave-pipeline | 82 / 62 / 71 | 11572 / 17757 / 25714 | 20 / 22 / 20 | 14923 / 14090 / 14064 | 80 / 80 / 80 | 0 / 0 / 0 |
| cloud-architecture | 54 / 60 / 61 | 20806 / 20456 / 20456 | 35 / 31 / 30 | 24789 / 26073 / 26052 | 90 / 96 / 96 | 0 / 0 / 0 |
| mixed-layouts | 75 / 88 / 85 | 0 / 0 / 0 | 5 / 5 / 5 | 3715 / 3715 / 3715 | 0 / 0 / 0 | 25 / 25 / 25 |
| nested-tree-radial | 143 / 136 / 140 | 0 / 0 / 0 | 0 / 0 / 0 | 18099 / 18099 / 18099 | 88 / 88 / 88 | 0 / 0 / 0 |
| tree-100 | 146 / 82 / 86 | 0 / 0 / 0 | 0 / 0 / 0 | 24768 / 27885 / 27885 | 0 / 0 / 0 | 196 / 196 / 196 |
| state-tcp | 22 / 8 / 9 | 0 / 0 / 0 | 5 / 0 / 0 | 6101 / 5757 / 5757 | 47 / 38 / 38 | 0 / 0 / 0 |
| deployment-cluster | 157 / 147 / 152 | 49278 / 39324 / 38984 | 176 / 162 / 158 | 35854 / 42055 / 41997 | 200 / 156 / 156 | 0 / 0 / 0 |
| self-loops | 32 / 22 / 23 | 0 / 0 / 0 | 15 / 0 / 0 | 2671 / 2321 / 2321 | 30 / 30 / 30 | 21 / 23 / 23 |
| layerweave-code | 7189 / 2095 / 2801 | 2251396 / 638755 / 633406 | 1327 / 1603 / 1563 | 238956 / 300831 / 292347 | 748 / 561 / 563 | 0 / 0 / 0 |

Totals over the 13 layered datasets (off / before / now): refinement ms 8173 / 2932 / 3785; grid pops 2319177 / 732727 / 734813; crossings 1615 / 1910 / 1877; length 414626 / 475784 / 467325; bends 1530 / 1303 / 1303

Reading:
- After the refinement the gain is small: crossings −1.7 % against "before" (1910 → 1877), length −1.8 %. The refinement
  chooses among layout candidates and improves the delegated routes; the import gain does not carry over one to one.
  petri-net rises from 92 to 102 (another candidate wins).
- The refinement is 29 % slower than "before" (2.93 → 3.79 s; layerweave-code 2.10 → 2.80 s, petri-net 63 → 191 ms). With
  two rounds of moves instead of eight, layerweave-code is not faster (2.87 s) and has more crossings (1638).
- bpmn-pools (screenshot of `Request payment`): the crossing of e10 and e13 can be removed by exchanging their ports at
  `Request payment` and searching both again, but another crossing appears; the move does not lower the count in that
  placement.
