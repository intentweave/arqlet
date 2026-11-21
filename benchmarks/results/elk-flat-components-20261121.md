# elkLayout on flat graphs with many small components, 2026-11-21

Question: why do flat, sparse graphs laid out through `elkLayout` take several times elk-rs's area, and does packing the
components as ELK does close the gap without costing quality?

## Machine, build, inputs

- Apple M3, macOS 26.0, rustc 1.99.0; Arqlet as a release `layout-command` (and the release WASM module for the area
  table), elk-rs as pinned in `benchmarks/eval` (`@openedges/elk-layout`).
- Revision b2d1360 ("before") against the same revision with the change in `crates/elk` ("after").
- Area table: flat graphs in ELK's JSON form, `n` nodes of random size (60–160 x 30–60), `n x ratio` random edges, layered
  to the right, generated with a fixed linear congruential seed (7). Quality table: `benchmarks/eval/elk-compat.mjs
  --limit 150` on the 37 datasets of `benchmarks/datasets` and on three flat datasets of 150 nodes with 0, 15 and 75
  random edges (seed 5, written for this check, not committed).

## Area against elk-rs (Arqlet's bounding area / elk-rs's)

| n | edges | before | after |
|---|---|---|---|
| 100 | 0 | 1.19 | 1.19 |
| 100 | 9 | 1.45 | 1.37 |
| 100 | 48 | 2.88 (1076 x 4165) | 1.47 (1970 x 1163) |
| 300 | 0 | 1.29 | 1.29 |
| 300 | 30 | 1.39 | 1.40 |
| 300 | 149 | 2.62 (1131 x 12560) | 1.38 (3312 x 2257) |
| 1000 | 0 | 1.28 | 1.28 |
| 1000 | 100 | 1.47 (2586 x 6046) | 1.44 (5319 x 2883) |
| 1000 | 500 | 4.48 (2046 x 43604) | 1.90 (6215 x 6084) |

Before, many small components stood one below the other (aspect down to 1:21). After, they are packed as boxes in rows
towards ELK's `aspectRatio` (default 1.6), as elk-rs does by default (`separateConnectedComponents` true).

## Quality

- The 37 datasets of `benchmarks/datasets`: crossings 3 796, bends 6 649, lines through nodes 0, shared lines 3,
  overlaps 0, before and after: no view changed a quality number (their views are connected, so the rule does not apply).
- The flat datasets: 0 and 15 edges unchanged; 75 edges crossings 15 → 3 (elk-rs 10), bends 72 → 68 (elk-rs 54), lines
  through nodes 0 → 0.
- Evaluation graphs kept outside the repository (not published): with packing whenever two components had edges, a
  graph with one dominant component became larger, because that component sets the width and packing the rest only
  added height. With the rule below all of them give byte-identical output before and after.

## The rule (`crates/elk`)

`packComponents` with `aspect` = the graph's `elk.aspectRatio` (default 1.6) when `separateConnectedComponents` is not
`false`, the top level has at least two components that have edges, and the largest of them holds at most half of the
linked nodes. A request's own `options` decide over it.

## What remains

Flat graphs without edges and with few edges stay about 1.2 to 1.4 times elk-rs's area. Most of it is spacing: with
ELK's default spacings (20 between nodes and between layers, which the adapter applies only when the graph names them)
the ratio is 1.16 to 1.28 on the same graphs. Taking ELK's defaults when a graph names none would change every drawing
through `elkLayout` and is a decision of its own.
