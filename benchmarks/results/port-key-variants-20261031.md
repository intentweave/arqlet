# Port order keys for the layered construction (tried, reverted), 2026-10-31

Machine and method as in `layered-ports-loops-20261031.md` (Apple M3, release, `bench --reps 3 --threads 1`, layerweave-code
graph of 0bcfd56). Baseline: the state of that file (13 layered datasets: 1910 routed crossings). The code was restored to
it afterwards; geometry fingerprints of layerweave-code, petri-net and uml-class equal the recorded ones.

Case: bpmn-pools, lane sales. `Request payment → Payment ok?` (e10, a backward edge) and `Payment ok? → Request payment`
(e13) cross above Request payment. In the refinement, e13's planned way runs straight from its port and changes lane only
in the last corridor; the port order key (the lane after the first change) is then its far end, which sorts it on the wrong
side of e10.

| variant | crossings, 13 layered datasets | changed datasets |
|---|---:|---|
| baseline: key = lane after the first change | 1910 | |
| key = lane held in the corridor next to the node (own port lane if no change there), ties by the other end, planning falls back to all gap tracks | 1948 | bpmn-pools 7 → 6, uml-class 7 → 5, petri-net 92 → 97, layerweave-code 1603 → 1639 |
| baseline key, ties by the other end, planning fallback | 2034 | petri-net 92 → 97, layerweave-code 1603 → 1722 |
| key = lane held next to the node, alone | 1962 | bpmn-pools 7 → 6, uml-class 7 → 5, layerweave-code 1603 → 1658 |

Reading: each key fixes some sides and breaks others; no fixed rule over one end's lane orders the ports of a side well.
The order of a side depends on where the ways of its edges run relative to each other over their whole length (the lanes
along the flow are still chosen freely by the track search, and the planned ways run on the gaps' centres, where several
edges share one lane). Next step: decide the port order of a side and the lane order of a corridor by counting the
crossings of the planned ways, not by one key.
