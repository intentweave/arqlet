# Organic repair: a pure speedup, 2026-11-09

Branch `feature/single-pass-mixed`, Apple M3, release builds of `layout-command` before (`594151e`) and after; median of three
runs per view, one process per run (times include process start); load average about 3 (not idle: indicative).

## Change (output byte-identical)

- The repair's trial scan (`repair::RootDrawing::scan_from_cache`, `assemble`, `assemble_trial`) visits only the conflicting
  items: the cached scan keeps the sorted sets of conflicting node pairs and (edge, node) items, and a trial merges those it
  leaves unchanged with the items it recomputed, in the same order and summed in the same order. Before, every trial walked
  every pair and every (edge, node) item (about 165 000 at 300 nodes).
- The nodes whose ports a move can change come from a list per node instead of a scan of every edge with string comparisons.
- The port resolution (`PortContext::resolve_nodes_into`) reads the boxes in one pass in id order and walks the parents by
  index, adding the offsets in the same order as `absolute_with`, instead of a map lookup per node and per parent.

## Identity

- Every corpus view at threads 1 and 4 (64 runs, default options): byte-identical to `594151e`.
- organic-100 and organic-200 with `sparseStress`, organic-300, mixed-large with `singlePassMixed`, mixed-layouts, self-loops,
  petri-net-shapes: byte-identical.
- The repair's test mode checks every incremental scan against a full one (conflicts and depth to the bit): `cargo test -p
  layerweave-layout` passes.

## Time

| view | before | after |
|---|---:|---:|
| organic-100 | 137 ms | 88 ms |
| organic-200 | 5984 ms | **1382 ms** |
| organic-300 | 14005 ms | **2125 ms** |
| mixed-large | 33253 ms | 28607 ms |
| petri-net-shapes | 622 ms | 587 ms |

`mixed-large` gained little from this first step: its root's children are containers, which the incremental trial did not
take (a moved container moves its children), so every trial there was a full scan (`RootDrawing::compute`).

## Second step: moved containers (output byte-identical)

A trial that moves a container (`RootDrawing::scan_subtree`) now starts from the cached scan as well: the container's pairs with
its siblings are recomputed (the pairs inside it move together and keep their items), the ports of every node inside it and of
their peers are resolved again, and the (edge, node) items at the nodes inside it are recomputed for every edge. The test
`moved_containers_are_scanned_incrementally_and_match_a_full_scan` moves containers (one nested) and the test mode compares every
such scan and every committed move with a full scan, to the bit.

Same machine and method, against `594151e` (load average about 3–5); every run byte-identical:

| view | options | before | after |
|---|---|---:|---:|
| organic-100 | sparseStress | 155 ms | 74 ms |
| organic-200 | sparseStress | 14902 ms | 3316 ms |
| organic-300 | — | 14018 ms | 2146 ms |
| mixed-large | — | 33319 ms | **8386 ms** |
| mixed-large | singlePassMixed | 38443 ms | 12627 ms |
| mixed-layouts | — | 22 ms | 20 ms |
| petri-net-shapes | — | 600 ms | 550 ms |
| can-bus | singlePassMixed | 12 ms | 12 ms |

The 64 corpus runs (threads 1 and 4) are byte-identical as well.
