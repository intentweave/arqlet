# Stage timing of import and refinement, 2026-10-30

Bench, threads 1, reps 1, limit 100, release. Apple M3, 8 logical CPUs, macOS 25.0.0, rustc 1.99.0. Revision 0bcfd56 plus uncommitted changes (the experimental layered construction is off by default). Load average about 5 (not idle). Raw output: stage-timing-20261030.jsonl.

- petri-net: import 5.4 ms; refinement 150 ms; routing 130 ms, scoring 6 ms, corridor gaps 0.9 ms, placement 1.8 ms, port resolution 0.8 ms, repair 0.2 ms
- layerweave-code: import 135.2 ms; refinement 7370 ms; routing 6921 ms, scoring 157 ms, corridor gaps 12.5 ms, placement 7.8 ms, port resolution 5.5 ms, repair 2.9 ms
