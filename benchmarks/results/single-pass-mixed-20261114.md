# `singlePassMixed` with shape-aware metrics, 2026-11-14

Apple M3, Darwin 25.0.0 arm64, rustc 1.99.0, Node 24.21.0; release `layout-command` from the branch
`single-pass-regions` (ADR 0020, step S1-1). Command: `node benchmarks/eval/single-pass-mixed.mjs`; single runs.

Change against `single-pass-mixed-20261109.md`: the `metrics` drawing now carries node shapes, and a circle is measured as
its disc (as the straight router already treats it). Before, an edge that ends correctly on a circle's outline ran inside
the circle's box and counted as a line through a node. Rectangles are measured as before; the fingerprint corpus has no
circles, so the published metrics of the default views do not change.

| dataset | variant | single pass | line–node hits | crossings | bends | hugging | area (k) | geometry issues | ms |
|---|---|---|---|---|---|---|---|---|---|
| can-bus | before | no | 0 | 0 | 32 | 1 | 1315 | 0 | 15 |
| can-bus | mixed | yes | 0 | 0 | 27 | 1 | 1305 | 0 | 21 |
| mixed-large | before | no | 44 | 726 | 0 | 20 | 50888 | 61 | 9812 |
| mixed-large | mixed | yes | 67 | 756 | 0 | 24 | 27185 | 88 | 18243 |
| mixed-layouts | before | no | 0 | 5 | 0 | 1 | 1330 | 0 | 20 |
| mixed-layouts | mixed | yes | 0 | 5 | 0 | 1 | 1330 | 0 | 21 |
| self-loops | before | no | 0 | 0 | 30 | 0 | 255 | 0 | 6 |
| self-loops | mixed | yes | 0 | 0 | 30 | 0 | 255 | 0 | 6 |

Reading:

- mixed-layouts and self-loops have no line through a node in either pipeline (10 and 9 before, all at circles).
- mixed-large: every remaining hit is a straight one-segment edge of the organic root (or of a `none` area) that runs
  through another node; the geometry check already reports them as `straight-through-node`. `singlePassMixed` places the
  root more densely (half the area) and has more of them, so it stays off. The defect is the routing inside the organic
  region, not single pass: ADR 0020 step S1-1b.
- A circle that a route passes near is now checked for hugging instead of counting as a hit, so hugging can rise by
  the same edges (mixed-layouts 0 to 1).
