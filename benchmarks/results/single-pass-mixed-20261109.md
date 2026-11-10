# Single pass with organic, tree and radial areas (`singlePassMixed`), 2026-11-09

Branch `feature/single-pass-mixed` on the bus work (`1df554f`), Apple M3, release build of `layout-command`; command `node benchmarks/eval/single-pass-mixed.mjs` (from `benchmarks/eval`). One run each
(deterministic); times are indicative (machine not idle).

## What the option does

Without it, a graph with an area of no fixed flow (organic, tree, radial) or with straight routing leaves single pass and is
laid out by the pipeline before it (reported as `Single pass was not used`). With `singlePassMixed` such a graph stays in single
pass: the areas without a flow keep their own placement (stress, tree, radial, and the geometry repair as before), their edges go
to the router that completes single pass, straight routing is applied by the same last step as before, and the channel growth of
single pass handles areas without a plan: they keep their children where their placement put them, and siblings that overlap
move apart (the later one in reading order, along the axis it overlaps least, with the placement's gap).

Pure organic, tree or radial graphs have no layered area and are laid out by the organic pipeline either way; only mixed graphs
are affected. Of the corpus, `can-bus`, `mixed-large`, `mixed-layouts` and `self-loops` are.

## Result (150-node cut, same `metrics` operation)

| view | arm | overlaps | line–node hits | shared | crossings | hugging | bends | length | area (k) | time |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| can-bus | before single pass | 0 | 0 | 0 | 0 | 1 | 32 | 9520 | 1315 | ~10 ms |
| can-bus | `singlePassMixed` | 0 | 0 | 0 | 0 | 1 | **27** | **9059** | 1305 | 24 ms |
| mixed-layouts | before | 0 | 10 | 0 | 5 | 0 | 0 | 3713 | 1330 | 23 ms |
| mixed-layouts | `singlePassMixed` | 0 | 10 | 0 | 5 | 0 | 0 | 3713 | 1330 | 23 ms |
| self-loops | before | 0 | 9 | 0 | 0 | 0 | 30 | 2319 | 255 | 5 ms |
| self-loops | `singlePassMixed` | 0 | 9 | 0 | 0 | 0 | 30 | 2331 | 255 | 5 ms |
| mixed-large | before | 0 | **90** | 0 | **726** | 19 | 0 | 242119 | 50888 | 32 s |
| mixed-large | `singlePassMixed` | 0 | 117 | 0 | 756 | 19 | 0 | **204067** | **27185** | 38 s |

By the gate's order `can-bus` is better (bends), `mixed-layouts` and `self-loops` are equal, and `mixed-large` is worse (27 more
line–node hits, most of them straight edges of its organic root through nodes: its root is arranged more compactly, at half
the area). Before the overlap step ran on every area without a plan, `mixed-large` had 5 node overlaps.

## Decision

`singlePassMixed` stays off by default. Open: why the single-pass placement of an organic root gives straight edges more hits
than the earlier pipeline on `mixed-large` (its repair runs on the same geometry), and the time of the organic repair there
(both pipelines take over 30 s on this view).
