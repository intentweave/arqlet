> **Superseded.** The composition ignored authored container directions; the composed numbers and the claim that the composition wins on org-chart and workflow-dag are wrong. See `structured-compare-correction-20261024.md`.

# Joint layout, raw composition and winner, five diagrams with containers (plus one flat diagram)

Machine: Apple M3, 8 logical CPUs, 16 GiB, macOS 26.0. Revision: a08f7d0 plus the uncommitted extension of `hierarchy::HierarchyResult` (new fields `composed_candidate`, `joint_candidate`; no change to any result) and the example `crates/runtime/examples/structured_compare.rs`. Build: rustc 1.95.0, `cargo run --release --locked -p layerweave-runtime --example structured_compare`.
Options: default `LayoutOptions` for every candidate. Threads: 1. One run per diagram; times are indicative only.
Drawings: `benchmarks/results/structured-compare/<name>-{joint,composed,winner}.svg` (17 files, not viewed as images; no SVG rasterizer is installed here).

- **Joint**: layout of the whole graph, then refinement.
- **Composed (raw)**: the hierarchical composition before the comparison, whether or not it wins.
- **Winner**: what `structuredRefine` returns (the better key of the two).

The composed key, when lower, is the winner. Keys are compared lexicographically; the third entry is shared line pairs in the report, the fourth is routed crossings in the bpmn-pools and workflow-dag rows.

## Results

| Diagram | Structure | Winner | Crossings joint / composed | Shared line pairs (report) joint / composed | Edge length joint / composed | Drawing extent joint / composed (px) |
|---|---|---|---|---|---|---|
| state-tcp | flat | joint (no composition) | 5 / – | 0 / – | 5883 / – | – |
| bpmn-pools | 5 containers | joint | 3 / 6 | 0 / 0 | 9865 / 10183 | 1924×974 / 2760×374 |
| network-vlan | 5 containers, 21 cross-container edges | joint | 66 / 64 | 0 / 10 | 26960 / 42659 | 1556×1864 / 3380×1092 |
| nested | 2 nested containers | composed | 0 / 0 | 0 / 0 | 1072 / 1000 | – |
| org-chart | 33 containers, 82 cross-container edges | composed | 92 / 73 | 6 / 1 | 67612 / 57900 | – |
| workflow-dag | 8 containers, 125 cross-container edges | composed | 624 / 499 | 33 / 14 | 207855 / 53339 | 13740×200 / 2130×878 |

Keys (joint / composed):
- bpmn-pools: [0,0,0,3,10,11,45,986500] / [0,0,0,6,15,15,50,1018300] → joint wins.
- network-vlan: [0,0,0,83,50,81,126,2696000] / [0,0,10,70,31,68,122,4265946] → joint wins; the composition has more crossings-related entries better (70 < 83) but loses at the shared-pairs entry (10 > 0), which the key ranks first.
- nested: [0,0,0,0,2,0,11,107200] / [0,0,0,0,2,0,11,100000] → composed wins.
- org-chart: [0,0,6,108,57,135,290,6761186] / [0,0,1,80,42,98,204,5789950] → composed wins.
- workflow-dag: [0,0,33,701,739,0,500,20785480] / [0,0,14,554,167,494,416,5333900] → composed wins.

## Independent checks (recomputed from the geometry, not from the report)

Checked for every candidate: all routes present (every edge has one), every route starts and ends on the border of its nodes (tolerance 1), every child lies inside its parent, no edge-node hits. All pass in every candidate of every diagram. The routing counts are complete, and the boundary continuity holds at the endpoints.

Recomputed edge lengths equal the reported ones in every candidate.

Shared line pairs, recounted from segments (tolerance 1), differ from the report: org-chart 7 (report 6) joint, 1 composed; workflow-dag 46 (report 33) joint, 15 (report 14) composed. The definitions differ and are not yet reconciled. The report's values are what the keys use.

Shared stretches lying on a container frame (within 2 units): 0 in every composed drawing, 1 in the org-chart joint drawing. So the shared lines in the network-vlan composition (10 pairs, 14 stretches) are not on container borders.

## Reading

1. **The network-vlan loss is not a boundary effect.** Its shared stretches run through the drawing, not along container frames. The composition has fewer crossings (64 against 66) but more edge length (+58 %) and longer routes (longest 3087 against 1798). The key ranks shared pairs before crossings, so the 10 shared pairs decide the loss. The cause is in the routing of the composed drawing, not in the border handshake.
2. **The bpmn-pools loss is crossings.** The composition has 6 crossings against 3 and no shared lines. Where these crossings lie is not yet checked.
3. **The workflow-dag length reduction is largely a layout effect.** The joint drawing is a single strip 13740 px wide; the composed one is 2130 × 878. The longest edge falls from 6114 to 1407. The measured lengths are correct, but a compact drawing shortens every edge. Edge length alone does not separate better routing from a smaller drawing.
4. **The suspected next focus is not confirmed.** The proposal "shared track planning at container boundaries" is not supported by these numbers for network-vlan, and bpmn-pools shows no shared lines at all. The evidence points to routing inside the composed drawing and to the crossings in bpmn-pools. Node-level measures are zero everywhere, so placement cannot be ruled out, but nothing here shows it is the cause.

## Timing

Indicative, one run each: joint 207–213 ms and composed-plus-joint 257–384 ms on workflow-dag and org-chart; the structured entry computes the joint candidate as well, so its time includes it. Not a budget-matched comparison.

## Limits

- Five diagrams with containers and one flat diagram; one option set; one run each; no repetitions.
- Drawings are checked numerically; I have not viewed the SVG files.
- The "where do the bpmn-pools crossings lie" and "why does the vlan composition route longer" questions are open; they are the next checks, before choosing a feature.
