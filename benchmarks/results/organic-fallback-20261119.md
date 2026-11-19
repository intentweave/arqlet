# Organic areas above 300 children (2026-11-19)

An organic area above the dense bound (`organic::MAX_NODES`, 300 children) is placed by the scalable stress when no
stress was chosen, instead of being refused; above `SCALABLE_MAX_NODES` (2 000) it is refused with that bound. This
measures one organic area at the top level: a ring with a chord from every fifth node.

Machine: Apple M3 (8 cores, 16 GB), macOS (Darwin 25.0.0), **heavily loaded** (load average 54 to 62 during the runs):
the times are indications. Build: `cargo build --release -p arqlet-runtime --bin snapshot`, branch `bus-trunk` on
`7bec528` plus the bound message of this change (the placement code is the same). Command:
`python3 benchmarks/scaling/organic_fallback.py --sizes 200 300 301 500 1000 1500 2000 2001 --reps 3`; raw rows in
`organic-fallback-20261119.jsonl`. Each run is one `snapshot` process (JSON read and write included); peak memory is
the process's maximum resident set (`/usr/bin/time -l`).

| nodes | edges | placement | median | runs | peak memory |
|---|---|---|---|---|---|
| 200 | 240 | dense | 0.32 s | 0.87, 0.32, 0.31 | 9 MiB |
| 300 | 360 | dense | 0.47 s | 0.49, 0.47, 0.47 | 11 MiB |
| 301 | 362 | scalable (fallback) | 0.46 s | 0.46, 0.39, 0.50 | 12 MiB |
| 500 | 600 | scalable (fallback) | 0.77 s | 0.71, 0.77, 0.90 | 16 MiB |
| 1000 | 1200 | scalable (fallback) | 1.50 s | 1.71, 1.50, 1.50 | 30 MiB |
| 1500 | 1800 | scalable (fallback) | 2.51 s | 2.42, 2.62, 2.51 | 48 MiB |
| 2000 | 2400 | scalable (fallback) | 3.37 s | 3.36, 3.64, 3.37 | 68 MiB |
| 2001 | 2402 | refused | | | "bounded to 2000 children per area" |

Before this change every row above 300 was refused. The time grows about linearly in this range; at the bound the
fallback is no slower than the dense stress at 300. Quality is not measured here: the scalable stress is the one of
`scalableStress` (`organic-scalable-20261109.md`), and sparse rings are an easy shape. Not measured: dense graphs,
areas with containers, sessions.
