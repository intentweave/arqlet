# ELK-format comparison, 2026-11-08

Branch `feature/elk-adapter` on `939674f`, Apple M3, release builds. The same graph in ELK's JSON form, built by the shared adapter
(`benchmarks/eval/elk-adapter.mjs`: layered, one direction, orthogonal routing, `INCLUDE_CHILDREN`, every leaf 180 × 54), laid out
by elk-rs (`@openedges/elk-layout`, pinned in `benchmarks/eval/package-lock.json`) and by Layerweave's new `elkLayout` operation
(`crates/elk`, through `layout-command`), and both results read back by the same normalization and measured by the same `metrics`
operation. 31 corpus views of the uniform task, 150-node cut, one run each (both deterministic).

Raw: [elk-format-20261108.json](elk-format-20261108.json). Command: `node benchmarks/eval/elk-compat.mjs --out …` (from
`benchmarks/eval`).

## Result

Every view: Layerweave's output is accepted by the same reader as elk-rs's (no failure; every edge one section, in the frame its
`container` names).

| Sum | elk-rs | Layerweave |
|---|---:|---:|
| crossings, 30 views (without layerweave-code) | 4868 | **2712** |
| bends, 30 views | **4208** | 4385 |
| line–node hits, shared line pairs, overlaps | 0 | 0 |
| layerweave-code (203 nodes, 294 edges): crossings / bends | 1193 / 1492 | **1111 / 1259** |

layerweave-code is listed apart: a playground build regenerated it during this run (its graph grows with the source); the other 30
views gave identical numbers in two runs. Layerweave draws fewer crossings on most layered views (synthetic-2k 1791 / 3660,
workflow-dag 230 / 278, petri-net 32 / 58) and more on deployment-cluster (96 / 71) and org-chart (29 / 23); it draws more bends on
the tree and organic datasets, which this adapter lays out as layered graphs for both engines.

The times in [elk-format-20261108.json](elk-format-20261108.json) are not comparable: elk-rs runs in-process, Layerweave as one
process per call with JSON transport.

## Time, both in one process

`--wasm` runs Layerweave as the WASM module of `apps/playground/build.sh` in the same Node process as elk-rs (also WASM,
`@openedges/elk-layout` 0.12.1), on the branch after `78f9803` (spacings, port constraints, label placement; no layout change).
Apple M3, 8 cores, Node 24.21; **not idle** (load average 3–7 during the run), so the times are indicative, one run each.
Raw: [elk-format-20261108-wasm.json](elk-format-20261108-wasm.json). The drawings are the same as above (30 views: crossings
4868 / 2712, bends 4208 / 4385).

| 30 views | elk-rs | Layerweave |
|---|---:|---:|
| sum of times | 143 ms | 453 ms |
| ratio Layerweave / elk-rs, per view | | median 2.3, from 1.3 to 11.6 |

The largest gaps: synthetic-2k (231 nodes, 318 edges) 36 / 163 ms, activity-order (31 nodes) 4.4 / 51 ms. Layerweave in this
setting includes the translation, the JSON of the snapshot and the write-back.

## Ports across the flow (finding; followed up in [port sides](elk-port-sides-20261108.md))

ELK users often set ports on all four sides. Single pass constructs an edge only when its ports lie on the flow sides; any other
edge is drawn by the fallback router. On 40 random graphs (6–19 nodes, ports on random sides, `FIXED_SIDE`, half `RIGHT`, half
`DOWN`; `node benchmarks/eval/elk-side-ports.mjs BINARY [BINARY]`) 38 of 687 edges were constructed. A trial built such edges from a stub out of the port side when that side faces the
other end: 85 constructed, but shared line pairs 25 → 28, crossings 241 → 248 and bends 2296 → 2334, so worse by the gate order; it was
not kept. The cause is earlier: order and placement do not know port sides, so most ports face away from their other end. Taking
port sides into the order (as ELK's port-side handling does) is the lever; it is open.

## What the adapter reads and what it reports

See the module documentation of `crates/elk`. On these inputs it reports only the hyperedge split where it applies; `elk.hierarchyHandling`
is read as Layerweave's own behaviour (edges cross every container border). Since `78f9803` the spacings, port constraints (`FREE`,
`FIXED_SIDE`/`FIXED_ORDER` with `port.index`, `FIXED_POS`/`FIXED_RATIO`) and `nodeLabels.placement` are read. Options without a
counterpart (node placement strategies, ...) are listed in the response's `diagnostics`.
