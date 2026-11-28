# Louvain against Leiden (#237, 2026-11-28)

Apple M3 (8 cores), macOS 26.0, rustc 1.99.0, `--release` (lto thin), branch `leiden` from `origin/main` `2cfefcd`
(working tree with the change, not yet committed). **The machine was shared and heavily loaded**: load average
`28 / 54 / 57` before and `45 / 47 / 53` after the run (1, 5, 15 minutes). Modularity, communities, levels and work
units are deterministic and do not depend on the load; **the times do**, and are not measurements of speed. Raw rows:
[`communities-20261128T040936Z.jsonl`](communities-20261128T040936Z.jsonl) (first line: machine). Reproduce with
`python3 benchmarks/overview/communities.py --reps 5` (it builds and runs the ignored test
`crates/layout/tests/community_eval.rs` and times the `overview` operation through `layout-command`).

Datasets: the overview's generated code index (`benchmarks/overview/generate.py`, seed 1) at 10 000, 50 000 and
100 000 nodes, and its flat variant (`flatten`: the symbols only, all at the top level) at 2 000, 10 000 and 50 000
nodes (1 866, 9 412 and 46 911 symbols); the reference views of `benchmarks/generate.py` (`layerweave-code`,
`synthetic-2k`, `synthetic-20k`, `organic-300`, `mixed-large`). Threshold 120 (the overview's default) and 40 (the
structured view's measurements). Containers as `with_communities` splits them (more children than the threshold, at
least half linked); `synthetic-2k`, `synthetic-20k` and `mixed-large` have none at either threshold.

## The algorithms alone (per container that is split)

Modularity of the final partition at resolution 1 (mean over containers); communities of at least two linked members;
*disconnected*: communities whose members no path of links inside the community joins, at the final level and at any
level of the hierarchy; work in the units of the work budget (ADR 0018, addendum of 2026-11-27).

| dataset | thr. | containers | algorithm | modularity | disconnected (final / any level) | communities | ≥ 8 nodes | largest | levels (max) | work |
|---|---|---|---|---|---|---|---|---|---|---|
| overview-10000 | 120 | 13 | Louvain | 0.7185 | 0 / 0 | 188 | 146 | 40 | 4 | 84 684 |
| | | | Leiden | 0.7169 | 0 / 0 | 188 | 149 | 36 | 4 | 78 710 |
| overview-50000 | 120 | 53 | Louvain | 0.7003 | 0 / 0 | 803 | 653 | 47 | 4 | 448 952 |
| | | | Leiden | 0.7002 | 0 / 0 | 796 | 650 | 45 | 5 | 411 973 |
| overview-100000 | 120 | 107 | Louvain | 0.7189 | 0 / 0 | 1 694 | 1 354 | 47 | 4 | 868 521 |
| | | | Leiden | 0.7183 | 0 / 0 | 1 715 | 1 367 | 41 | 5 | 804 771 |
| overview-flat-2000 | 120 | 1 | Louvain | 0.7881 | 0 / 0 | 25 | 22 | 166 | 4 | 207 811 |
| | | | Leiden | 0.7811 | 0 / 0 | 26 | 23 | 181 | 5 | 85 991 |
| overview-flat-10000 | 120 | 1 | Louvain | 0.8190 | 0 / 0 | 63 | 53 | 762 | 5 | 2 264 353 |
| | | | Leiden | 0.8138 | 0 / 0 | 65 | 55 | 818 | 7 | 484 698 |
| overview-flat-50000 | 120 | 1 | Louvain | 0.8410 | 0 / 0 | 166 | 123 | 938 | 5 | 17 599 472 |
| | | | Leiden | 0.8368 | 0 / 0 | 164 | 121 | 1 033 | 7 | 2 522 872 |
| organic-300 | 120, 40 | 1 | Louvain | 0.8834 | 0 / 0 | 17 | 17 | 25 | 3 | 9 770 |
| | | | Leiden | 0.8870 | 0 / 0 | 18 | 18 | 26 | 4 | 9 022 |
| layerweave-code | 40 | 6 | Louvain | 0.4344 | 0 / 0 | 35 | 21 | 18 | 2 | 11 446 |
| | | | Leiden | 0.4366 | 0 / 0 | 38 | 21 | 17 | 3 | 11 545 |

## The whole clustering, and the overview end to end

Clusters the overview adds (communities of at least 8 nodes that are not the whole container, every level),
clustering work, and the median time of the clustering alone (5 runs) and of the whole `overview` request through
`layout-command` (5 runs after a warm-up, threads 1, JSON in and out included). Times under the load above.

| dataset | algorithm | clusters added | clustering work | clustering ms | overview s (median, min–max) |
|---|---|---|---|---|---|
| overview-10000 | Louvain | 227 | 84 684 | 2.7 | 0.79 (0.77–0.92) |
| | Leiden | 249 | 78 710 | 2.9 | 0.77 (0.72–0.85) |
| overview-50000 | Louvain | 1 088 | 448 952 | 14.2 | 3.66 (3.61–3.85) |
| | Leiden | 1 238 | 411 973 | 15.0 | 3.85 (3.80–4.12) |
| overview-100000 | Louvain | 2 252 | 868 521 | 29.0 | 9.67 (8.15–11.61) |
| | Leiden | 2 534 | 804 771 | 30.9 | 13.20 (10.19–14.35) |
| overview-flat-2000 | Louvain | 128 | 207 811 | 1.9 | 0.31 (0.30–0.32) |
| | Leiden | 210 | 85 991 | 1.7 | 0.26 (0.25–0.27) |
| overview-flat-10000 | Louvain | 707 | 2 264 353 | 16.1 | 1.57 (1.15–1.62) |
| | Leiden | 1 126 | 484 698 | 10.3 | 0.91 (0.86–1.37) |
| overview-flat-50000 | Louvain | 3 944 | 17 599 472 | 139.8 | 7.73 (6.28–8.04) |
| | Leiden | 5 765 | 2 522 872 | 59.3 | 5.70 (5.46–5.95) |
| layerweave-code (thr. 40) | Louvain | 32 | 11 446 | 0.5 | – |
| | Leiden | 31 | 11 545 | 0.5 | – |
| organic-300 | Louvain | 24 | 9 770 | 0.1 | – |
| | Leiden | 26 | 9 022 | 0.1 | – |

## Findings

1. **Connectivity:** on none of these graphs did Louvain leave a community in parts, at any level (0 of 2 991
   communities). Leiden's guarantee holds (0 as well, and tested), but on this data it changes nothing. Louvain's flaw
   does occur: a deterministic search over 20 000 small generated graphs with planted groups found one case
   (`leiden_keeps_connected_what_louvain_leaves_in_parts` in `crates/layout/src/overview/leiden.rs`), so it is rare
   here, not absent.
2. **Modularity:** about equal. Leiden (greedy refinement) is within 0.1 % of Louvain on the hierarchical code index,
   0.5–0.9 % lower on the flat graphs, 0.4–0.5 % higher on `organic-300` and `layerweave-code`.
3. **Work:** about equal on the hierarchical graphs (Leiden 7–10 % less); on the flat graphs far less (2.4× at 2 000,
   4.7× at 10 000, 7.0× at 50 000 nodes): Louvain's sweeps run over every node while the queue of the moving phase only
   revisits nodes whose neighbourhood changed. The clustering time follows (2.4× faster at 50 000 flat, under load).
   The overview as a whole is dominated by the arrangement; its times are too noisy under this load to compare beyond
   that.
4. **Shape:** Leiden makes deeper hierarchies (up to 7 levels instead of 5 on flat graphs) and more clusters (+10 % on
   the code index, +46 to +64 % on the flat graphs), because each refined partition becomes a level. The largest
   community is similar.

## Recommendation

**Keep Louvain as the default** (not changed here). Leiden brings no measured gain in what it is for — connected
communities — on these graphs, its modularity is equal or slightly lower, and it changes the shape of the overview
(more, deeper clusters), which is a question of readability, not of this measurement. Leiden is the better choice for
large flat graphs by work (up to 7× less clustering work) and for hosts that need the guarantee. Before a default
switch: measure on real exports (not in the repository), and decide whether the deeper hierarchy reads better; a
randomised refinement with a seed would be the next lever on modularity.
