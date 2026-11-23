# `organicGap` (#156, provisional, off), 2026-11-23

Machine: Apple M3 (MacBook), macOS, heavily loaded during the runs (load average 8 to 25), so the times are noisy; take
the route counts and areas as the result. Revision: PR #158 (route repair through narrow gaps) plus the option. Build:
`cargo build --profile fast -p arqlet-runtime --bin layout-command`. Inputs: `node benchmarks/experiments/bus-mesh.mjs
UNITS BUSES organic lines CHILDREN EDGES`, with `options.organicGap` set where it is not 24. "Routes": routes of the final
drawing that cross a box other than their ends and the ends' ancestors. Area: the bounding box of the top-level boxes.

| Input | Gap 24 (default): routes, time, area | Gap 36 | Gap 48 |
| --- | --- | --- | --- |
| 100 units, no buses, 60 children, 250 edges | 0, 1.96 s, 2232 × 3957 | 0, 0.32 s, 2413 × 4470 | 0, 0.37 s, 2566 × 4496 |
| 100 units, 30 buses, 60 / 250 | 21 of 409, 0.98 s, 2440 × 3648 | 14, 0.37 s, 6115 × 4028 | 3, 0.25 s, 6694 × 3840 |
| 100 units, 30 buses, 120 / 500 | 24 of 659, 7.9 s, 4903 × 3081 | 0, 0.69 s, 4468 × 3568 | 6, 7.32 s, 4734 × 3846 |
| 100 units, 30 buses, 240 / 1 000 | 18 of 1 159, 31.5 s, 7029 × 2362 | 11, 14.64 s, 7669 × 2655 | 8, 0.54 s, 8365 × 2955 |

Reading: a wider gap mostly leaves fewer routes through boxes and fewer forced overlaps, so the route repair and the
overlap resolution often finish much faster; the area grows, once to 2.5 times the width. The organic placement reacts
strongly to the box sizes, so the effect is not monotone in the gap. Not accepted as a default: it needs the quality
gates (crossings and bends against the default and elk-rs, line–node hits, area) on the curated views first.

Default output: the option is omitted when unset and the arrangement is unchanged; the 15 default fingerprint views were
checked identical.
