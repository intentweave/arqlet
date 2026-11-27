# Overview work budget: units per second (2026-11-27, #211)

Machine: Apple M3, 8 cores, macOS 26.0 (darwin 25.0.0), Node v24.21.0. Revision `a157053` (branch `overview-budget`),
clean tracked tree; native `layout-command` release build (`lto = "thin"`), WASM release build
(`wasm32-unknown-unknown`). **The machine was under load** (another agent's measurements): 1-minute load average 4.4 to
6.0, recorded per row below. Graphs: `benchmarks/overview/generate.py --seed 1` (code-index-like; communities split
files with more than 120 symbols). Command:

    node benchmarks/overview/work-rate.mjs --sizes 10000 50000 100000 --reps 3 --threads 1 4 \
      --out benchmarks/results/overview-work-budget-20261127.jsonl

Per size and engine: the whole overview (`workBudget` 2^53 − 1, so `work.used` is all of it), half that work as the
budget, and `workBudget: 0` (nothing budgeted: every container packed; the part of the time the budget does not
bound). Native: one `layout-command` process per request, JSON read and written included. WASM: the engine in the Node
process (one thread). One warm-up, then 3 runs; median (min–max). "units/s (budgeted part)" is
`used / (whole − budget 0)`; "units/s (whole)" is `used / whole`. Raw rows: `overview-work-budget-20261127.jsonl`.

| nodes | engine | load before/after | work used (of which clustering) | whole | half the work as budget | budget 0 | units/s (budgeted part) | units/s (whole) |
|---|---|---|---|---|---|---|---|---|
| 10000 | native, 1 thread | 5.3/5.2 | 43524917 (84684) | 756 ms (752 ms–761 ms) | 403 ms (401 ms–405 ms) (used 21734743, 442 packed) | 52 ms (52 ms–52 ms) (537 packed) | 61.8 M | 57.6 M |
| 10000 | native, 4 threads | 5.2/5.2 | 43524917 (84684) | 264 ms (263 ms–267 ms) | 158 ms (157 ms–161 ms) (used 21734743, 442 packed) | 53 ms (52 ms–53 ms) (537 packed) | 206.4 M | 165.1 M |
| 10000 | WASM | 5.2/5.1 | 43524917 (84684) | 791 ms (790 ms–792 ms) | 420 ms (420 ms–423 ms) (used 21734743, 442 packed) | 64 ms (63 ms–65 ms) (537 packed) | 59.8 M | 55.0 M |
| 50000 | native, 1 thread | 5.1/4.4 | 217031002 (448952) | 3749 ms (3740 ms–3762 ms) | 1998 ms (1992 ms–2003 ms) (used 108477642, 2218 packed) | 264 ms (263 ms–270 ms) (2635 packed) | 62.3 M | 57.9 M |
| 50000 | native, 4 threads | 4.4/4.5 | 217031002 (448952) | 1363 ms (1345 ms–1433 ms) | 816 ms (809 ms–873 ms) (used 108477642, 2218 packed) | 257 ms (256 ms–268 ms) (2635 packed) | 196.2 M | 159.3 M |
| 50000 | WASM | 4.5/5.1 | 217031002 (448952) | 3991 ms (3981 ms–4060 ms) | 2165 ms (2135 ms–2172 ms) (used 108477642, 2218 packed) | 346 ms (345 ms–350 ms) (2635 packed) | 59.6 M | 54.4 M |
| 100000 | native, 1 thread | 5.1/6.0 | 438591664 (868521) | 7975 ms (7821 ms–8136 ms) | 4218 ms (4083 ms–4489 ms) (used 219290548, 4043 packed) | 535 ms (535 ms–538 ms) (4981 packed) | 58.9 M | 55.0 M |
| 100000 | native, 4 threads | 6.0/5.6 | 438591664 (868521) | 3319 ms (3011 ms–3363 ms) | 1774 ms (1751 ms–1777 ms) (used 219290548, 4043 packed) | 526 ms (525 ms–541 ms) (4981 packed) | 157.1 M | 132.1 M |
| 100000 | WASM | 5.6/5.2 | 438591664 (868521) | 8116 ms (8101 ms–8135 ms) | 4367 ms (4335 ms–4485 ms) (used 219290548, 4043 packed) | 724 ms (722 ms–736 ms) (4981 packed) | 59.3 M | 54.0 M |

Observations (this machine, these graphs only):

- The work grows linearly with the graph here: about 4.35 M units per 1 000 nodes (43.5 M at 10 000, 438.6 M at
  100 000); clustering is 0.2 % of it.
- One thread spends 59 to 62 M units per second of the budgeted part, natively and in WASM alike (WASM is 5 to 7 %
  slower in total, most of it in the unbudgeted part: 64 against 52 ms at 10 000 nodes, 724 against 535 ms at 100 000).
- The time follows the budget: with half the work as the budget, the time is within 1 to 2 % of
  `budget 0 + (whole − budget 0) / 2` for one thread.
- With 4 threads the rate is 157 to 206 M units per second: the units are the same, the time is shared. A host
  calibrates with the thread count it uses.
- The unbudgeted part (budget 0) is 52 to 64 ms at 10 000 nodes and 526 to 724 ms at 100 000 nodes.

The answers are identical for both thread counts and native and WASM (`tools/platform-parity.mjs`, runtime tests
`overview_budget::`). No claim beyond this machine and load; a host measures its own rate.
