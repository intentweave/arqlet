# Overview continued by node id: how far nodes move (#209)

Machine: Apple M3, 16 GB, macOS 26.0. Revision 035bd32 (branch `overview-server-result`). Positions only; no timing.
The layout is deterministic, so another machine gives the same numbers.

Graphs: `benchmarks/overview/generate.py`, seed 1: hierarchical with 10 000 nodes (SHA-256 `2fe8e1c6…`), and `--flat`
from 6 000 (`464599b7…`, 5 612 symbols at the top level, so every container is a community the overview added). The
script removes a share of the leaves, adds as many nodes (each beside one it calls) and rewires that share of the edges
(seeded, seed 7). A movement is the distance between a surviving node's old and new centre in the shared frame, in
percent of the earlier root radius. No run had overlapping discs.

    node benchmarks/overview/continuation-drift.mjs target/fast/layout-command \
      benchmarks/datasets/overview-10000.json benchmarks/datasets/overview-flat-6000.json

| Graph, share changed | Request | Median | 90th percentile | Largest | Root radius (× earlier) |
|---|---|---|---|---|---|
| hierarchical, 0.1 % | `start.nodes` | 1.2 | 1.6 | 2.1 | 1.02 |
| | `start.nodes`, `settle` | 6.0 | 12.2 | 18.1 | 0.99 |
| | no `start` | 4.3 | 11.6 | 32.2 | 1.00 |
| hierarchical, 1 % | `start.nodes` | 2.5 | 3.5 | 4.8 | 1.04 |
| | `start.nodes`, `settle` | 5.2 | 11.8 | 18.4 | 0.99 |
| | no `start` | 12.3 | 29.0 | 65.8 | 0.99 |
| hierarchical, 5 % | `start.nodes` | 7.5 | 10.8 | 19.7 | 1.19 |
| | `start.nodes`, `settle` | 6.4 | 13.1 | 20.7 | 1.00 |
| | no `start` | 19.1 | 44.0 | 76.4 | 0.99 |
| flat, 0.1 % | `start.nodes` | 0.0 | 0.0 | 0.9 | 1.00 |
| | `start.nodes`, `settle` | 11.8 | 32.8 | 61.4 | 1.01 |
| | no `start` | 72.3 | 144.9 | 175.7 | 0.97 |
| flat, 1 % | `start.nodes` | 1.3 | 1.7 | 5.2 | 1.04 |
| | `start.nodes`, `settle` | 11.8 | 28.7 | 117.2 | 1.00 |
| | no `start` | 88.0 | 143.4 | 186.2 | 1.00 |
| flat, 5 % | `start.nodes` | 2.6 | 8.6 | 16.1 | 1.19 |
| | `start.nodes`, `settle` | 11.8 | 31.0 | 56.6 | 1.01 |
| | no `start` | 77.7 | 136.9 | 185.6 | 1.02 |

- Without `settle` gaps are not closed, so the root disc grows with the share changed; one change is measured here, not
  a long series of them.
- `settle` runs forces in every changed container, the top level among them. It moves more than without it, except the median at
  5 % on the hierarchical graph; at 0.1 % there its median and 90th percentile exceed those of no `start`.
- The continuation keeps the earlier clusters; the layout without `start` clusters anew.
