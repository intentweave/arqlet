# Quality floors at three view sizes, 2026-10-30

Machine: Apple M3, 8 logical CPUs, macOS 25.0.0, revision 0bcfd56 plus uncommitted changes, load about 5 (not idle).
Quality: `compare.mjs --limit 50|100|150` (150: `compare-20261030.jsonl`, three runs for the times; 50 and 100: one run each).
Times are single runs at 50 and 100, so the ratios are indicative only.

## Floors and decision rule
- Floor A (refinement): the refined layout is not lexicographically worse than the import layout (same objective order as the
  project: overlaps and lines through nodes, shared lines, border hugging, crossings, bends, length).
- Floor B (against ELK): the refined layout is not lexicographically worse than the ELK layout of the same view.
- Decision rule (set before the run): a speed trade-off is acceptable only where floor A holds on every dataset and floor B holds
  on the datasets where ELK is faster. Results below.

## Results

| size | floor A (not worse than import) | floor B (not worse than ELK) | where B fails (decided by) |
|---|---|---|---|
| 50 | 17 of 17 | 12 of 17 | activity-order (hugging), er-diagram (bends), layerweave-pipeline (hugging), mixed-layouts (overlaps+line-node), network-vlan (crossings) |
| 100 | 17 of 17 | 12 of 17 | activity-order (hugging), er-diagram (bends), layerweave-pipeline (hugging), mixed-layouts (overlaps+line-node), network-vlan (crossings) |
| 150 | 17 of 17 | 12 of 17 | activity-order (hugging), er-diagram (bends), layerweave-pipeline (hugging), mixed-layouts (overlaps+line-node), network-vlan (crossings) |

## Speed against quality

At 100 nodes the refined layout is 0.3–57× the time of ELK. The ratio is large where the search runs longest (organic-200 57×,
layerweave-code 27×, organic-100 11×) and below 1 on small views (state-order 0.3×, uml-class 0.7×).

## Reading

- Floor A holds on every dataset at every size: refinement never makes the layout lexicographically worse.
- Floor B separates the same datasets at every size: layerweave is better or equal on 12, and worse on network-vlan (crossings),
  er-diagram (bends), activity-order and layerweave-pipeline (border hugging), and mixed-layouts (overlaps / lines through nodes).
- The decision rule is not met: floor B fails on five datasets, and the speed gap is large on the organic and the largest layered sets.
