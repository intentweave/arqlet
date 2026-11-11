# Isolated children as a grid (`isolatedGrid`, 2026-11-11)

Apple M3, macOS 26.0, Rust 1.99.0, `--release`, branch `isolated-grid-session` from `main` fd7e1df. Geometry only, no
timing. Scenes of `import` with default options against `options: {isolatedGrid: true}`.

**Problem.** A layered area whose children have no edges among them (a file of unrelated functions or types) put them
all into rank 0: one column, up to 30 times as long as wide. In the cluster overview a structured cluster is scaled by its
diagonal into its disc, so such a cluster was drawn tiny.

**Change (opt-in).** Children without a link to another child get ranks of their own after the connected children, a
grid of about √(k·pitch) columns (pitch: gap of two neighbours in a rank over that of two ranks), at least as many rows
as the widest connected rank. They form one ordering block of their own, so they never share a level with connected
children: the connected part is placed exactly as without them (test
`isolated_children_form_a_grid_after_the_connected_part`).

## Real clusters (Intentweave export of 2026-11-11, 8 874 nodes)

`node benchmarks/eval/isolated-grid.mjs exports/intentweave-20261111/graph.json 14`: the 14 largest clusters of at most
300 nodes with at least 15 children, laid out as the overview lays out a structured cluster (`structuredExcerpt`).

| nodes | cluster | off: size | area M | diagonal | bends | crossings | through nodes | grid: size | area M | diagonal | bends | crossings | through nodes |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 235 | `packages/core/src/interfaces.ts` | 640×18282 | 11.70 | 18293 | 6 | 3 | 0 | 3564×3084 | 10.99 | 4713 | 6 | 3 | 0 |
| 219 | `packages/index/src/claims/types.ts` | 886×17046 | 15.10 | 17069 | 16 | 8 | 0 | 3274×3546 | 11.61 | 4826 | 16 | 8 | 0 |
| 208 | `packages/index/src/export/prescriptiveReport.ts` | 640×16242 | 10.39 | 16255 | 2 | 2 | 0 | 3224×4854 | 15.65 | 5827 | 2 | 2 | 0 |
| 181 | `packages/core/src/reports/types.ts` | 640×14118 | 9.04 | 14132 | 14 | 8 | 0 | 3028×2850 | 8.63 | 4158 | 14 | 8 | 0 |
| 158 | `packages/index/src/claims/portableState.ts` | 606×12390 | 7.51 | 12405 | 0 | 0 | 0 | 2702×4158 | 11.23 | 4959 | 0 | 0 | 0 |
| 154 | `packages/index/src/claims/candidates.ts` | 606×12066 | 7.31 | 12081 | 6 | 0 | 0 | 2946×2934 | 8.64 | 4158 | 6 | 0 | 0 |
| 150 | `packages/core/src/types/index.ts` | 640×11784 | 7.54 | 11801 | 12 | 7 | 0 | 2444×3162 | 7.73 | 3996 | 12 | 7 | 0 |
| 146 | `packages/core/src/weave/types.ts` | 886×11430 | 10.13 | 11464 | 8 | 5 | 0 | 3030×2082 | 6.31 | 3676 | 8 | 5 | 0 |
| 146 | `packages/cypher-lite/src/types.ts` | 640×11322 | 7.25 | 11340 | 4 | 3 | 0 | 2100×2220 | 4.66 | 3056 | 4 | 3 | 0 |
| 128 | `packages/cli/src/commands/claims.ts` | 924×10086 | 9.32 | 10128 | 54 | 2 | 0 | 2388×3846 | 9.18 | 4527 | 54 | 2 | 0 |
| 126 | `packages/analyzer/src/stages/ax.ts` | 640×9900 | 6.34 | 9921 | 10 | 3 | 0 | 1956×4014 | 7.85 | 4465 | 10 | 3 | 0 |
| 126 | `packages/core/src/types/stages.ts` | 886×9900 | 8.77 | 9940 | 8 | 4 | 0 | 2446×2388 | 5.84 | 3418 | 8 | 4 | 0 |
| 108 | `packages/ast-extractor/src/types.ts` | 640×8496 | 5.44 | 8520 | 4 | 3 | 0 | 2248×2394 | 5.38 | 3284 | 4 | 3 | 0 |
| 108 | `packages/core/src/types/tcg.ts` | 640×8472 | 5.42 | 8496 | 0 | 1 | 0 | 1808×2082 | 3.76 | 2757 | 0 | 1 | 0 |

- Bends, crossings and lines through nodes are the same in all 14 clusters.
- The diagonal shrinks 2.2–3.9×, so the overview draws these clusters 2.2–3.9× larger.
- The area is smaller in 9 of 14 and larger in 5 (up to 1.5×, `prescriptiveReport.ts`, `portableState.ts`): a column is
  as wide as its widest label, and the grid adds rank gaps. The shape, not the area, was the problem here.

## Synthetic containers

One container with `n` children, of which the first `l` form a random tree plus `l/3` extra edges (`random.seed(l)`), the
rest unconnected. "alone": only the `l` connected children.

| n:l | alone (area M, bends) | off (area M, bends) | grid (area M, bends) |
|---|---|---|---|
| 40:12 | 0.75, 18 | 4.44, 20 | 2.35, 18 |
| 60:30 | 3.47, 54 | 9.82, 56 | 4.20, 54 |
| 120:20 | 1.39, 44 | 16.23, 40 | 5.16, 44 |
| 200:60 | 7.93, 130 | 44.46, 134 | 11.34, 130 |
| 300:120 | 16.78, 262 | 63.46, 258 | 24.54, 262 |
| 40:0 | – | 1.02 (312×3276), 0 | 0.98 (1044×936), 0 |

No crossings and no lines through nodes in any case. With the grid, bends equal those of the connected part alone;
"off" differs from that by −4 to +4, because there the unconnected children share rank 0 with connected ones and shift
their alignment. A variant that spread the unconnected children over the connected ranks gave smaller areas (10.7 and
19.8 M for the last two) but bends that differed from "alone" by up to +8; it was dropped.

**Status.** Off by default; the overview's structured clusters (`apps/playground/overview.js`) turn it on. Default
output unchanged.
