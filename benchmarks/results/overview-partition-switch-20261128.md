# Overview: switching between two partitions from the old places (#239)

Machine: Apple M3, 16 GB, macOS 26.0, shared and loaded (load averages 10.7 / 27.0 / 37.0 at the start, 8.5 / 25.2 /
36.1 at the end). Revision: branch `partition-switch` on `e1b38a2` (the code of this branch's first commit). Build:
`cargo build --release -p arqlet-runtime --bin layout-command --features query --locked`, rustc 1.99.0, Node 24.21.0.
The movement numbers are deterministic (any machine gives the same); the times are wall clock of one process per
request, including reading the graph and writing the answer, and only indicative on this machine.

Graphs: `benchmarks/overview/generate.py --seed 1`, 3 000 nodes (SHA-256 `a7f32ccd…`) and 10 000 nodes (`2fe8e1c6…`),
flattened to their symbols (2 816 and 9 450 of them at the top level, the calls between them); partition A groups them
by top-level directory and file (`[directory, file]`, 3 and 5 directories), `communities: 0`. The switches B
(`benchmarks/overview/partition-switch.mjs`):

| B | What it is |
|---|---|
| shared | A with every tenth file moved to the next directory (most paths kept) |
| renamed | A's groups under other names: the same members, no path shared |
| regrouped | the files dealt into as many groups as A has directories by a hash: no path shared, cutting across A |
| regrouped (Louvain 40) | the same with `communities: 40`, so Louvain divides the large groups |
| merge | A with the two largest directories merged into one group |
| split | A with the largest directory split in two (files by parity) |

A movement is the distance between a symbol's centre in A and in B, in percent of A's root radius; "anew" is
`continuation.anew` (groups count). Valid: no overlapping discs in a container, every disc inside its parent's (checked
on every answer, also A's).

    node benchmarks/overview/partition-switch.mjs target/release/layout-command \
      benchmarks/datasets/overview-3000.json benchmarks/datasets/overview-10000.json

| Graph | B | Request | Anew | Median | 90th percentile | Largest | Root radius (× A) | Valid | ms |
|---|---|---|---|---|---|---|---|---|---|
| 3 000 | shared | no `start` | – | 65.4 | 91.7 | 153.9 | 0.989 | yes | 306 |
| 3 000 | shared | `start.nodes` | 308 | 6 | 36.8 | 174.1 | 1.232 | yes | 59 |
| 3 000 | shared | `start.nodes`, `carry` | 308 | 1.9 | 25.7 | 128.5 | 1.073 | yes | 64 |
| 3 000 | renamed | no `start` | – | 0 | 0 | 0 | 1 | yes | 327 |
| 3 000 | renamed | `start.nodes` | 2951 | 0 | 0 | 0 | 1 | yes | 302 |
| 3 000 | renamed | `start.nodes`, `carry` | 2951 | 7.8 | 11.2 | 24.3 | 1.073 | yes | 284 |
| 3 000 | regrouped | no `start` | – | 60.5 | 103 | 144.5 | 0.614 | yes | 569 |
| 3 000 | regrouped | `start.nodes` | 2816 | 60.5 | 103 | 144.5 | 0.614 | yes | 571 |
| 3 000 | regrouped | `start.nodes`, `carry` | 2816 | 43.1 | 72.3 | 115.1 | 0.678 | yes | 572 |
| 3 000 | regrouped (Louvain 40) | no `start` | – | 95.8 | 157.4 | 208.4 | 1.454 | yes | 272 |
| 3 000 | regrouped (Louvain 40) | `start.nodes` | 2816 | 95.8 | 157.4 | 208.4 | 1.454 | yes | 271 |
| 3 000 | regrouped (Louvain 40) | `start.nodes`, `carry` | 2816 | 88.6 | 124.7 | 196.8 | 1.688 | yes | 274 |
| 3 000 | merge | no `start` | – | 68.4 | 101.6 | 164 | 0.83 | yes | 300 |
| 3 000 | merge | `start.nodes` | 2913 | 83.9 | 127 | 148.5 | 1.273 | yes | 312 |
| 3 000 | merge | `start.nodes`, `carry` | 2913 | 20.6 | 37.9 | 80.3 | 0.86 | yes | 285 |
| 3 000 | split | no `start` | – | 27.2 | 93.6 | 122.3 | 0.99 | yes | 298 |
| 3 000 | split | `start.nodes` | 2291 | 96.8 | 163.7 | 181 | 1.852 | yes | 251 |
| 3 000 | split | `start.nodes`, `carry` | 2291 | 30.2 | 51 | 108.9 | 0.996 | yes | 247 |
| 10 000 | shared | no `start` | – | 27.8 | 64.8 | 182.6 | 1.014 | yes | 1025 |
| 10 000 | shared | `start.nodes` | 1021 | 19.5 | 59.2 | 187.5 | 1.316 | yes | 182 |
| 10 000 | shared | `start.nodes`, `carry` | 1021 | 7.2 | 30 | 164.2 | 1.043 | yes | 227 |
| 10 000 | renamed | no `start` | – | 0 | 0 | 0 | 1 | yes | 1089 |
| 10 000 | renamed | `start.nodes` | 9897 | 0 | 0 | 0 | 1 | yes | 1060 |
| 10 000 | renamed | `start.nodes`, `carry` | 9897 | 5.3 | 9.5 | 19.1 | 1.048 | yes | 967 |
| 10 000 | regrouped | no `start` | – | 69.3 | 112.6 | 159.7 | 0.689 | yes | 2246 |
| 10 000 | regrouped | `start.nodes` | 9450 | 69.3 | 112.6 | 159.7 | 0.689 | yes | 2365 |
| 10 000 | regrouped | `start.nodes`, `carry` | 9450 | 49.3 | 88.5 | 137.9 | 0.768 | yes | 2058 |
| 10 000 | regrouped (Louvain 40) | no `start` | – | 116.2 | 175.3 | 234.9 | 1.622 | yes | 921 |
| 10 000 | regrouped (Louvain 40) | `start.nodes` | 9450 | 116.2 | 175.3 | 234.9 | 1.622 | yes | 943 |
| 10 000 | regrouped (Louvain 40) | `start.nodes`, `carry` | 9450 | 113.9 | 177.9 | 255.5 | 2.241 | yes | 924 |
| 10 000 | merge | no `start` | – | 65.3 | 127.7 | 170.4 | 1.016 | yes | 1047 |
| 10 000 | merge | `start.nodes` | 5522 | 57.5 | 125 | 173.8 | 1.51 | yes | 610 |
| 10 000 | merge | `start.nodes`, `carry` | 5522 | 18.1 | 43.6 | 92.3 | 1.092 | yes | 570 |
| 10 000 | split | no `start` | – | 12.4 | 50.8 | 103.7 | 0.975 | yes | 986 |
| 10 000 | split | `start.nodes` | 3180 | 23.8 | 109.5 | 174.7 | 1.214 | yes | 388 |
| 10 000 | split | `start.nodes`, `carry` | 3180 | 26.2 | 37 | 54.6 | 1.066 | yes | 398 |

Raw lines: `overview-partition-switch-20261128.jsonl` (with the engine's own `movement` report of each carried answer,
whose shares divide by the answer's root radius instead of A's).

## Reading

- **Carrying moves less than today's continuation** (`start.nodes` alone) in the median, the 90th percentile and the
  largest move in shared, regrouped and merge on both graphs, and in split and regrouped with Louvain at 3 000 nodes,
  and keeps the root disc closer to A's (merge 1.09 against 1.51, split 1.07 against 1.21 at 10 000 nodes). Where no
  path is shared, today's continuation is the fresh layout. The exceptions follow.
- **The exception: Louvain inside new groups** (regrouped, `communities: 40`). The communities made now start at the
  centre of members that come from all over the drawing, so they start on top of each other; at 10 000 nodes the
  largest move is larger than today's (255.5 against 234.9), the median about the same (113.9 against 116.2), and the
  root disc grows to 2.24 × A's (fresh: 1.62 ×). At 3 000 nodes carrying is still ahead there, with a larger root disc.
- **Split:** the largest moves fall to a third of today's (54.6 against 174.7 at 10 000 nodes), but the median rises a
  little (26.2 against 23.8; a fresh layout: 12.4).
- **Renamed** (the same members under other names): a fresh layout draws A again exactly (the layout does not depend
  on ids), so today's continuation moves nothing either; carrying settles every container from its old places and moves
  the nodes a little (median 5.3, largest 19.1 at 10 000 nodes). A host that knows its partitions have the same members
  should keep A's answer.
- Every answer was valid.

## Temperature

The containers with carried nodes settle at the temperature of `settle` (0.3). A full temperature (1, as a fresh
layout) was measured once on the 3 000-node graph before (same revision otherwise; median / 90th percentile / largest,
root radius):

| B | 0.3 (chosen) | 1 |
|---|---|---|
| shared | 1.9 / 25.7 / 128.5, 1.073 | 1.5 / 37.9 / 141.5, 1.093 |
| renamed | 7.8 / 11.2 / 24.3, 1.073 | 11.5 / 18.4 / 36.4, 1.096 |
| regrouped | 43.1 / 72.3 / 115.1, 0.678 | 43.3 / 73.3 / 115.5, 0.606 |
| regrouped (Louvain 40) | 88.6 / 124.7 / 196.8, 1.688 | 75.8 / 120.9 / 198.9, 1.444 |
| merge | 20.6 / 37.9 / 80.3, 0.860 | 27.1 / 47.0 / 106.0, 0.894 |
| split | 30.2 / 51.0 / 108.9, 0.996 | 34.2 / 51.8 / 126.5, 0.986 |

0.3 moves less in five of six; 1 is better only where Louvain divides the new groups.
