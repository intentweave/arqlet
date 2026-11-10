# Layout engine comparison, 2026-11-04

## Setup

The comparison uses nine bounded views (`layerweave-code`, `deployment-cluster`, `workflow-dag`,
`org-chart`, `cloud-architecture`, `network-vlan`, `petri-net`, `uml-class`, and
`layerweave-pipeline`), a 150-node selection limit, uniform direction, and three measured runs per
engine and view. Times below are sums of the per-view medians, not one end-to-end request.

Machine: Apple M3, 8 logical CPUs, macOS; Node 24.21.0; Layerweave release binary; source revision
`e966e93` with a dirty worktree. The recorded load averages were about 4–5 during both runs, so
these measurements are indicative rather than a controlled speedup claim. Engine versions are
elkjs 0.12.0, elk-rs 0.12.1, Cytoscape 3.34.4, and d3-force 3.0.1.

The hierarchical comparison uses the original bounded views. Layerweave's import/refine times
include a `layout-command` process start and JSON transfer for each call; elkjs and elk-rs time only
their in-process `layout()` calls after graph conversion. The boundaries are therefore not equal.
Raw rows: [hierarchical comparison](./current-elk-compare-20261104.jsonl).

## Hierarchical layouts

The timings below came from the public-process comparison and have asymmetric boundaries. For a
matched in-process core comparison on identical bounded graph inputs, see
[inprocess-core-elk-20261104.md](./inprocess-core-elk-20261104.md); use that report for engine-core
time conclusions.

| Engine/output | Time (ms) | Crossings | Edge-node hits | Bends | Area |
|---|---:|---:|---:|---:|---:|
| Layerweave hierarchical, initial | 333.38 | 3544 | 0 | 3449 | 147.4 M |
| Layerweave hierarchical, after refine | 2538.98 | 3537 | 0 | 3421 | 151.0 M |
| elkjs layered | 574.05 | 3314 | 2 | 2724 | 90.0 M |
| elk-rs layered | 95.26 | 2957 | 1 | 2742 | 113.4 M |

On this timing boundary, Layerweave initial is about 3.5× elk-rs, while it takes less elapsed time
than elkjs. Refinement adds about 2.2 s over the nine views and changes the aggregate metrics only
slightly; its value remains input-dependent. The timings do not establish an algorithm-only ratio.
The three engines also produce different geometries.

The existing Dagre comparison is not used for these conclusions: compound mode fails on
`layerweave-code` and `uml-class`, and flat mode omits 38 edges in total.

## Flat force-layout comparison

Force-directed algorithms do not implement hierarchical containment or layered ranking. With
`--flat-force`, the input graph's parent links are removed before bounded selection, all nodes use
180×54 dimensions, and every engine receives the resulting same flat view. This is a separate
algorithm-class comparison, not a substitute for the hierarchical results. Cytoscape's built-in
CoSE and breadthfirst layouts are included; no external Cytoscape layout plugin is used.

| Engine/output | Time (ms) | Crossings | Edge-node hits | Overlaps | Bends | Area |
|---|---:|---:|---:|---:|---:|---:|
| Layerweave hierarchical, initial (flat input) | 234.92 | 3471 | 0 | 0 | 2753 | 50.3 M |
| elkjs layered (flat input) | 275.25 | 2736 | 0 | 0 | 1554 | 48.5 M |
| elk-rs layered (flat input) | 41.06 | 2736 | 0 | 0 | 1554 | 48.5 M |
| Cytoscape CoSE | 549.16 | 3113 | 3020 | 233 | 18 | 37.5 M |
| Cytoscape breadthfirst | 24.38 | 2997 | 2675 | 0 | 18 | 104.1 M |
| d3-force | 362.65 | 2890 | 2156 | 0 | 18 | 247.8 M |

Cytoscape CoSE used 1,000 iterations and deterministic initial positions. d3-force used seed 42 and
300 ticks. All force-layout adapters return straight center-to-center edges (with explicit loop
geometry for self-loops); edge routing is not part of these libraries' layout result. The high
edge-node hit counts make their lower crossing counts an incomplete quality comparison. Breadthfirst
is fast here, but it is a tree-oriented layout rather than a force-directed one.

Raw rows: [flat force comparison](./force-layout-compare-20261104.jsonl). Reproduce with:

```sh
npm ci --prefix benchmarks/eval
cargo build --release --locked -p layerweave-runtime --bins
node benchmarks/eval/compare.mjs --uniform-direction --repeat 3 \
  --out benchmarks/results/current-elk-compare-20261104.jsonl \
  layerweave-code deployment-cluster workflow-dag org-chart cloud-architecture network-vlan petri-net uml-class layerweave-pipeline
node benchmarks/eval/compare.mjs --flat-force --uniform-direction --repeat 3 \
  --out benchmarks/results/force-layout-compare-20261104.jsonl \
  layerweave-code deployment-cluster workflow-dag org-chart cloud-architecture network-vlan petri-net uml-class layerweave-pipeline
```
