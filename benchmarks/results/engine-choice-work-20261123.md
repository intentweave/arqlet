# Engine choice work limit (#149), 2026-11-23

Machine: Apple M3 (MacBook), macOS, heavily loaded during the runs (load average 8 to 21: other builds and apps), so the
absolute times are noisy; the ratio is not. Revision: `1a21a6f` (main after #154) plus this change. Build:
`cargo build --release -p arqlet-runtime --bin layout-command --locked`. Before = the same build with
`MAX_REROUTE_WORK = usize::MAX` (the engine choice as it was). Inputs: `node benchmarks/experiments/bus-mesh.mjs UNITS
BUSES STRATEGY lines` (seeded, public), one import each, 3 runs, wall time of `layout-command`.

| Input | Before (s) | After (s) | Engine choice skipped | Output |
| --- | --- | --- | --- | --- |
| 50 units, 15 buses, organic | 1.48, 0.94, 0.91 | 0.54, 0.08, 0.08 | yes (work 1.4 million) | identical (boxes and routes) |
| 100 units, 30 buses, organic | 23.56, 42.14, 20.19 | 0.36, 0.36, 0.34 | yes (work 9.7 million) | identical (boxes and routes) |
| 100 units, 30 buses, layered | 0.04, 0.04, 0.04 | 0.04, 0.04, 0.04 | not reached | identical |

Where the time went before (native `sample`, fast profile): `engine_choice::improve` routed the drawing again with the
channel router as the other engine, 19.6 s for 189 routes on a grid of 225 × 229 box edges, and found nothing better
(the output is the same). The legacy router took 93 ms for the same drawing.

Fingerprints: the 15 default views of `fingerprints-20261122-default-0.8.0.jsonl` (release bench, threads 1 and 4) are
identical; `layerweave-code` reports "input changed" (the dataset is regenerated from the source) and is identical with
and without the limit on the current dataset. The views that reach the engine choice stay below the limit:
workflow-dag 414 000, activity-order 54 080, petri-net 7 872.

Not addressed here (filed separately): a meshed bus view with containers (100 units, 480 children, 2 100 edges, organic)
takes about 90 s natively, in `repair::greedy` (overlap repair, port resolution per candidate), and leaves thousands of
routes through nodes; a `patch` that builds it costs the same as a fresh import of it.
