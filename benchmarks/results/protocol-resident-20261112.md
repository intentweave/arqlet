# Resident sessions against sending the snapshot (ADR 0019, step 3, 2026-11-12)

Raw data: [`protocol-20261112T202251Z.jsonl`](protocol-20261112T202251Z.jsonl), written by
`python3 benchmarks/protocol/measure.py --sizes 10000 100000 --reps 5 --max-load 2.5` (which also repeats the step-2
measurement on this revision).

Apple M3 (8 cores), Darwin 25.0.0, rustc 1.99.0, Node 24.21.0, revision e61e9df. The tree was marked dirty because
documentation was edited during the run; the measured code and the built WASM were those of the commit. Load average
2.48 at the start, 2.38–3.04 during the runs; medians of 5 runs after one warm-up.

Path measured (`benchmarks/protocol/measure-resident.mjs`): the request to a worker and the answer back (structured
clone, Node `worker_threads`), and the engine call in WASM (stringify, copy, run, parse). With the snapshot sent, a host
needs a second request for the scene after a toggle; a resident session's answer brings the scene, so `toggle + scene`
is one request there. Same graphs and view as step 2 (import collapsed from depth 1, limit 10 000; the toggle opens the
first collapsed container; the trace follows the first edge of the source graph).

| nodes | request | snapshot sent (ms) | resident (ms) | factor |
|---|---|---|---|---|
| 10 000 | toggle + scene | 822.6 | 515.1 | 1.6 |
| 10 000 | scene | 121.4 | 19.1 | 6.4 |
| 10 000 | trace | 108.5 | 5.4 | 20.1 |
| 100 000 | toggle + scene | 3 163.9 | 2 595.0 | 1.2 |
| 100 000 | scene | 238.1 | 59.5 | 4.0 |
| 100 000 | trace | 181.9 | 6.1 | 29.8 |

- Reads become cheap: a trace goes from 109–182 ms to 5–6 ms, a scene from 121–238 ms to 19–60 ms (what remains is
  computing the scene and moving it).
- Edits gain less, because their time is layout work (step 2); the gain is the snapshot no longer sent and received,
  and one request instead of two.
- The resident session still copies its snapshot for each edit (`Request::from_value_on` takes it by value); that copy is
  inside the resident times above.
