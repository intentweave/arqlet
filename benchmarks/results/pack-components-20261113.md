# Packing components (`packComponents`, experimental), synthetic shapes (2026-11-13)

Apple M3, Darwin 25.0.0 arm64, rustc 1.99.0, Node 24.21.0; release `layout-command` built from 819d34d plus the
`packComponents` change (branch `component-packing`); elk-rs (`@openedges/elk-layout`) as pinned in
`benchmarks/eval/package-lock.json`. Command: `node benchmarks/eval/pack-components.mjs` (raw rows:
`pack-components-20261113.jsonl`). Single runs; the times are not a performance result.

Shapes (deterministic generators in the script), standing for the packing problems an external evaluation reported:
`mixed` (40 small trees of 2–31 nodes and 30 lone nodes, flat), `lone` (120 nodes without edges), `leaves` (a
container with a 25-node tree and 80 leaves, as `reveal` opens), `wide` (a container with 60 small trees and edges to
one node outside, as opening a large container shows). Leaves are 180 × 54. Metrics: the shared `metrics` operation.

| shape | variant | width × height | aspect | area (M) | crossings | bends | line–node hits | overlaps | ms |
|---|---|---|---|---|---|---|---|---|---|
| mixed | default | 1400 × 13507.5 | 0.1 | 18.91 | 0 | 312 | 0 | 0 | 39 |
| mixed | componentPacking | 1400 × 13507.5 | 0.1 | 18.91 | 0 | 312 | 0 | 0 | 21 |
| mixed | isolatedGrid | 1644 × 11206.5 | 0.15 | 18.42 | 0 | 310 | 0 | 0 | 21 |
| mixed | packComponents | 4328 × 4104 | 1.05 | 17.76 | 0 | 304 | 0 | 0 | 21 |
| mixed | packComponents 16:9 | 5060 × 3442.5 | 1.47 | 17.42 | 0 | 298 | 0 | 0 | 22 |
| mixed | elk-rs layered | 1300 × 11838 | 0.11 | 15.39 | 0 | 272 | 0 | 0 | 28 |
| mixed | elk-rs, components separated | 4830 × 2671 | 1.81 | 12.9 | 0 | 272 | 0 | 0 | 10 |
| lone | default | 180 × 9336 | 0.02 | 1.68 | 0 | 0 | 0 | 0 | 4 |
| lone | componentPacking | 180 × 9336 | 0.02 | 1.68 | 0 | 0 | 0 | 0 | 4 |
| lone | isolatedGrid | 1400 × 1536 | 0.91 | 2.15 | 0 | 0 | 0 | 0 | 4 |
| lone | packComponents | 1400 × 1536 | 0.91 | 2.15 | 0 | 0 | 0 | 0 | 4 |
| lone | packComponents 16:9 | 1888 × 1146 | 1.65 | 2.16 | 0 | 0 | 0 | 0 | 4 |
| lone | elk-rs layered | 180 × 8860 | 0.02 | 1.59 | 0 | 0 | 0 | 0 | 2 |
| lone | elk-rs, components separated | 1580 × 1090 | 1.45 | 1.72 | 0 | 0 | 0 | 0 | 3 |
| leaves | default | 1448 × 6408 | 0.23 | 9.28 | 0 | 28 | 0 | 0 | 5 |
| leaves | componentPacking | 1448 × 6408 | 0.23 | 9.28 | 0 | 28 | 0 | 0 | 5 |
| leaves | isolatedGrid | 2668 × 1296 | 2.06 | 3.46 | 0 | 26 | 0 | 0 | 4 |
| leaves | packComponents | 2424 × 1452 | 1.67 | 3.52 | 0 | 26 | 0 | 0 | 5 |
| leaves | packComponents 16:9 | 2912 × 1062 | 2.74 | 3.09 | 0 | 28 | 0 | 0 | 5 |
| leaves | elk-rs layered | 1254 × 5998 | 0.21 | 7.52 | 0 | 26 | 0 | 0 | 2 |
| wide | default | 1694 × 10066.5 | 0.17 | 17.05 | 0 | 212 | 0 | 0 | 17 |
| wide | componentPacking | 1694 × 10134 | 0.17 | 17.17 | 0 | 214 | 0 | 0 | 18 |
| wide | isolatedGrid | 1938 × 9937.5 | 0.2 | 19.26 | 0 | 210 | 0 | 0 | 17 |
| wide | packComponents | 4030 × 4251 | 0.95 | 17.13 | 0 | 204 | 0 | 0 | 18 |
| wide | packComponents 16:9 | 5320 × 2886 | 1.84 | 15.35 | 0 | 198 | 0 | 0 | 18 |
| wide | elk-rs layered | 1499 × 9479 | 0.16 | 14.21 | 0 | 186 | 0 | 0 | 8 |

Reading:

- The default stacks components and lone nodes across the flow in one rank range: aspect 0.02–0.23 (very tall).
  `componentPacking` (blocks per level) does not change that; `isolatedGrid` helps only where lone nodes dominate.
- `packComponents` packs in rank space: aspect 0.91–1.67 for the target 1 (1.47–2.74 for 16:9), with area equal or
  smaller than the default, 0 crossings, 0 lines through nodes, 0 overlaps, and equal or fewer bends on all four
  shapes; the same time. The first attempt (moving finished components afterwards) broke the shared ranks the
  placement and routing rely on (up to 386 crossings, 4–5× the area) and was dropped.
- elk-rs through the shared adapter (`INCLUDE_CHILDREN`) does not pack either. Called with
  `separateConnectedComponents` on the flat shapes, it packs: `mixed` 4830 × 2671 (1.81), 12.9 M, 272 bends, against
  4328 × 4104 (1.05), 17.8 M, 304 bends here; `lone` 1.72 M against 2.15 M. The area gap is the general one (unpacked:
  15.4 M against 18.9 M); packing does not close it.
- A test graph of 20 trees and 15 lone nodes (`pack_components.rs`) gets 204 bends against 198 in the default (+3 %)
  at the same crossings and area: packing changes neighbours, and bends can rise a little. The option stays opt-in.
- Checked locally on larger graphs of an external evaluation (data and numbers not published): on large nested views
  with many edges across containers the aspect improved only a little and crossings rose clearly; behind the ELK
  adapter's `separateConnectedComponents` it made a mixed graph larger; for the structured clusters of the overview it
  was about even with `isolatedGrid`. So it helps forests and leaf-heavy containers, not large connected views, and
  neither the adapter nor `structuredExcerpt` uses it.
