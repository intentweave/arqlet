# A patch into a bus view is laid out as an import (#228, ADR 0029 C, 2026-11-27)

Before/after for the change of `patch_snapshot` (`crates/runtime/src/patch.rs`): a view with buses is laid out from
scratch, as an import of the patched graph.

- **Machine:** Apple M3 (8 cores, 16 GB), Darwin 25.0.0, rustc 1.99.0, Node 24.21.0. **Heavily loaded**: other agents
  were building and testing Rust; 1-minute load 14 to 25 during the bus-view runs, 11 to 19 during the fingerprint run.
  The times below are only good for comparisons within a row (the requests ran in turn), and even there roughly.
- **Builds:** main `f59ec9f` and the branch (main plus this change), both `cargo build --release --locked -p
  arqlet-runtime --bin layout-command --bin bench`.
- **Driver:** `benchmarks/experiments/bus-patch.mjs` of #227 (the #212 measurement; overview imported, then one patch
  with the children and their edges, against a fresh import), one run each, `--metrics 1`. Raw rows:
  `patch-as-import-20261127.jsonl` (labels `main` and `branch`).

```sh
C=60/12/organic/lines/60/250,100/30/organic/lines/240/1000,280/107/organic/lines/480/2100,100/30/layered/lines/240/1000,280/107/layered/lines/480/2100,100/30/organic/bar/240/1000,100/0/layered/lines/60/250,100/0/organic/lines/60/250
node benchmarks/experiments/bus-patch.mjs --binary MAIN --label main --metrics 1 --reps 1 --cases $C --out benchmarks/results/patch-as-import-20261127.jsonl
node benchmarks/experiments/bus-patch.mjs --binary BRANCH --label branch --metrics 1 --reps 1 --cases $C --out benchmarks/results/patch-as-import-20261127.jsonl
```

## Bus views and views without buses

| view | patch on main: ms, line–node hits, shared line pairs | patch on the branch | fresh import | patch = import (branch) | patch unchanged |
|---|---|---|---|---|---|
| 60/12 organic lines | 7 197, 24, 2 125 | 4 965, 24, 2 125 | 5 276, 24 | yes | yes |
| 100/30 organic lines | 63 180, 109, 16 046 | 34 614, 109, 16 046 | 32 355, 109 | yes | yes |
| 280/107 organic lines | 8 989, 1 408, 36 492 | 7 777, 1 408, 36 492 | 6 425, 1 408 | yes | yes |
| 100/30 layered lines | 582, **1 043**, 4 951 | 583, **0**, 380 | 561, 0 | yes | no (fixed) |
| 280/107 layered lines | 2 182, **2 091**, 15 744 | 2 299, **0**, 923 | 2 405, 0 | yes | no (fixed) |
| 100/30 organic bar | 7 419, **1 374**, 23 942 | **42 463**, **36**, 15 002 | 41 691, 36 | yes | no (fixed) |
| 100 units, no bus, layered, 60/250 | 553, 0, 18 | 731, 0, 18 | 55, 0 | no | yes |
| 100 units, no bus, organic, 60/250 | 6 112, 74, 390 | 6 479, 74, 390 | 662, 0 | no | yes |

- The patch of every bus view now has the geometry of the fresh import (same hash of boxes, routes and bus lines); the
  fresh imports are the same on main and the branch.
- Organic views with bus lines draw exactly what they drew on main (same geometry hash); their time differences are load.
- **Time cost:** a patch into an organic view with bus bars now takes an import's time. On the dense 100/30 view that is
  the import's second routing pass: 42 s here under load 17, about 16.5 s at load 4 (#212 measurement), against 7.4 s
  (2.0 s at load 4) before, when the patch's first routing left over 500 conflicts and skipped the pass, at the price of
  1 374 routes through nodes. Layered patches take about the same time as before.
- Views without buses are unchanged (same geometry as main). They are not covered by #228, and the measurement shows the
  same kind of defect there: the organic patch has 74 line–node hits where an import has 0, and the layered patch takes
  ten times an import's time. Recorded as a finding for the owner, not changed.

## Default fingerprints

`target/release/bench --graph benchmarks/datasets/NAME.json --limit 100 --reps 1 --threads 1,4 --name NAME` for the 15
views of `fingerprints-20261126-default-0.10.0.jsonl`, compared with `benchmarks/fingerprint_compare.py`:
**identical 15** (raw: `patch-as-import-20261127-fingerprints.jsonl`). Imports and refinements do not go through
`patch_snapshot`.
