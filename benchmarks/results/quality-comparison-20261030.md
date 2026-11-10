# Quality comparison: layerweave (refined) against elkjs, 2026-10-30

Source: `compare-20261030.jsonl` (`compare.mjs --limit 150 --repeat 3`; metrics from the first run). Machine and revision as in
`compare-20261030.md`: Apple M3, revision 0bcfd56 plus uncommitted changes, load about 5.

Method: the same metric code scores every engine. Objectives are compared lexicographically in the order of
`docs/configuration.md`: overlaps and lines through nodes (sum), shared lines, border hugging (margin 12), crossings, bends,
length. The first objective where the two engines differ decides.

Result: layerweave (refined) is better in 12 of 17 datasets, elkjs in 5. The decisive objective:

| decided by | layerweave | elkjs |
|---|---:|---:|
| border hugging | 10 | 2 |
| overlaps + lines through nodes | 2 (layerweave-code, uml-class) | 1 (mixed-layouts) |
| crossings | 0 | 1 (network-vlan) |
| bends | 0 | 1 (er-diagram) |
| shared lines | 0 | 0 |

Not included: dagre. Its flat variant drops every edge to a container and draws only leaves, so its metrics come from a
different graph (for example uml-class and er-diagram have length 0). It is shown in the summary table but not scored here.

Caveats:
- Border hugging decides 12 of the 17 datasets, but the two engines differ by only a few pairs on most of them (for example
  bpmn-pools 5 against 7, petri-net 14 against 19, layerweave-code 134 against 144 in favour of layerweave). It is a soft
  criterion (margin 12), and the decision rests on small differences. Whether this is the right weight is a project decision.
- One view size (150 nodes), one run of each engine for the metrics. Results at other sizes are not measured here.
- Per-metric values disagree (see `compare-20261030.jsonl`): layerweave has more crossings on 7 of the 17 datasets, more
  bends on 10, and a larger area on nested-tree-radial (42 M against 7 M) and bpmn-pools (1.7 M against 0.9 M); it has shorter
  edges on petri-net, activity-order, network-vlan and layerweave-code (for example 486 k against 932 k).
