# elkLayout: should it take ELK's default spacings when a graph names none? (2026-11-22)

Question: `elkLayout` applies the spacings a graph names (`spacing.nodeNode`, `spacing.nodeNodeBetweenLayers`,
`spacing.edgeNode`); without them it uses Arqlet's own (24 between nodes, 64 between layers). ELK's defaults are tighter
(20, 20, 10). Taking them as the adapter's default would bring drawings closer to elk-rs in size. Does it cost quality?

## Machine, build, inputs

- Apple M3, macOS, release `layout-command` at `04cde51`; elk-rs as pinned in `benchmarks/eval` (`@openedges/elk-layout`
  0.12.1). Single runs; times not compared.
- `node benchmarks/eval/elk-spacing.mjs --limit 150 --out … --html …`: the 39 datasets of `benchmarks/datasets` (without
  the 20k, 100k and overview inputs), first view of at most 150 nodes, in ELK's JSON form by the shared adapter (uniform
  task: one direction, layered, orthogonal), laid out by elk-rs, by `elkLayout` as it is, and by `elkLayout` with ELK's
  defaults named on the root (`elk.spacing.nodeNode` 20, `elk.layered.spacing.nodeNodeBetweenLayers` 20,
  `elk.spacing.edgeNode` 10). All three read back and measured the same way (`metrics`).
- The drawings side by side: the script's `--html` page (generated, not committed).

## Result (sums over the 39 datasets)

| | elk-rs | Arqlet, own spacings (today) | Arqlet, ELK default spacings |
|---|---:|---:|---:|
| Crossings | 5 615 | **4 939** | 5 080 (+2.9 %) |
| Bends | **5 418** | 5 605 | 6 085 (+8.6 %) |
| Line–node hits | 0 | **0** | 2 |
| Sibling overlaps | 0 | 0 | 0 |
| Area / elk-rs (median, min–max) | 1 | 1.41 (0.49–2.45) | **1.29** (0.47–2.35) |

Where the tighter spacings cost: bends on workflow-dag 300 → 450, synthetic-2k 1 358 → 1 516, layerweave-code 1 104 →
1 148 (crossings 2 220 → 2 323), mixed-large 346 → 380, deployment-cluster 212 → 244; one line through a node each on
er-diagram and nested-tree-radial. The area falls by 2–13 % on the views that change.

## Decision

Not adopted. Judged in the order of the objectives (no line through a node, then crossings, then bends, before length
and area), ELK's defaults trade quality for area: two line–node hits, 2.9 % more crossings and 8.6 % more bends for
about 9 % less area at the median. The tighter layer gap leaves the routes less room to change lanes, which is where the
bends come from. The adapter keeps applying only the spacings a graph names; a host that wants ELK's compactness names
them (see `docs/integration-guide.md`). The area gap to elk-rs is a matter of placement, not of the default spacings.

## Seen in the drawings (open)

Looking at the drawings side by side (decision confirmed: ELK packs tightly and seems to move nodes and containers to
pack densely and avoid some crossings, but readability and the look suffer in most views):

- Some nodes are placed far out at the edges of the drawing. On can-bus, the nodes of the first layer that feed into
  several containers stand at the far left and right, with long edges into the middle, where elk-rs keeps them close
  to their targets.
- layerweave-code is very tall (7 978 × 11 315 against elk-rs's 7 544 × 6 104) and has three times elk-rs's crossings
  (2 220 against 732) at about the same bends.

Both are placement questions (where an area's first layer and its long edges go), not spacing ones; to be looked at
separately.
