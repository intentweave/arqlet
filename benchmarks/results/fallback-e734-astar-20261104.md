# Fallback search: `e734`

This is a targeted diagnostic, not a general layout speedup result. The input is the uniform-direction `layerweave-code` view (150-item bound, 182 nodes, 303 edges; input geometry fingerprint `151fa83f78e57a57`). The measured ordering candidate was `ordering/1`, with `layeredConstruction` and `hierarchicalOrdering` enabled, legacy routing, and the same candidate set in both variants.

The actual refinement candidate reported `TrackTaken` for `e734`. In the constructor, this label means the monotone search failed while earlier constructed segments were assigned. It is intentionally conservative: without another search that ignores those segments, it does not prove whether they alone blocked the path or the boxes would also have blocked it.

| Variant | Grid pops (all / `e734` / `e745`) | Delegated stage median (5 samples) |
|---|---:|---:|
| Dijkstra | 27,021 / 24,934 / 2,087 | 7.897 ms |
| A* | 2,781 / 2,299 / 482 | 6.407 ms |

The A* heuristic uses Manhattan distance as a lower bound. It drains the equal-optimal frontier and reconstructs predecessors using Dijkstra's `(distance, grid-index)` tie order. For this input, both the candidate geometry fingerprint (`c0b38fd125670029`) and final refinement fingerprint (`3b903ce37435c0bd`) match exactly in all paired runs. The search work fell by 89.7%; the delegated-stage median fell by 18.9%. This stage includes all delegated routes, not only `e734`, and is not an end-to-end refinement measurement.

Each timing is from one `--candidate-log` diagnostic execution per process; five Dijkstra/A* pairs were interleaved. The machine was shared. A load snapshot taken before the runs reported 68.8% idle CPU and load averages 6.71, 6.58, 8.23, so the timings are diagnostic only. Raw samples are in [`fallback-e734-astar-20261104.jsonl`](./fallback-e734-astar-20261104.jsonl).

**Machine/revision:** Apple M3, 8 logical CPUs, macOS arm64, Rust 1.99.0, release build; base revision `dac8190`, dirty worktree. The paired binaries differed only in the grid-search strategy.
