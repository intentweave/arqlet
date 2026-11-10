# Global layering: the per-area order as the start of the common ranks' order, 2026-11-02

Machine: Apple M3, release, bench `--reps 3 --threads 1`, limit 100, revision e767c34 plus uncommitted changes; only the
datasets that global layering changes. layerweave-code is the graph of 0bcfd56 (a7636dbb). Default output unchanged (16 of
16 refinement fingerprints). The machine was not idle.

Change: before the hierarchical order, the area is placed once per area on copies (global layering off); a flattened
container's place in its parent's plan is its block's key, a unit's place in its container's plan its own key, a chain
takes the middle of its ends; `hierarchical_order` starts from these keys and keeps only what lowers the crossings.

| dataset | crossings: per area / iteration 3 / with the start | bends | refinement ms |
|---|---|---|---|
| uml-class | 5 / 6 / 2 | 61 / 79 / 77 | 19 / 22 / 18 |
| bpmn-pools | 6 / 7 / 8 | 46 / 46 / 52 | 19 / 14 / 14 |
| network-vlan | 62 / 43 / 43 | 106 / 104 / 104 | 32 / 41 / 42 |
| cloud-architecture | 30 / 31 / 31 | 83 / 82 / 82 | 196 / 21 / 24 |
| deployment-cluster | 107 / 138 / 135 | 136 / 140 / 146 | 87 / 108 / 96 |
| layerweave-code | 900 / 1098 / 1111 | 493 / 640 / 637 | 1317 / 875 / 1094 |

Hard violations: layerweave-code 10 → 8 (per area 0); the others 0.

A count after the import (temporary test) on layerweave-code: the order of the common ranks has 1006 bilayer crossings with
the start (1010 without), and the drawing 1007; the per-area layout has 216 within its areas and 867 in the drawing. The
common ranks are not deeper (7 ranks either way) and need fewer dummies (109 against 514), but about 100 functions share 7
ranks, and with every block contiguous the order stays near 1000 crossings from any start; per area, the routes between the
areas pass around the containers instead.

Reading: the start lowers uml-class below the per-area layout (2 against 5) and changes little elsewhere; layerweave-code is
not a matter of the start. Kept (it cannot make the order's crossings worse than the per-area order), at the cost of one
more placement per refinement candidate.
