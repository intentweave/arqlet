# Louvain against Leiden (#237, 2026-11-28)

Apple M3 (8 cores, arm64), macOS 26.0, rustc 1.99.0, `--release` (lto thin), branch `leiden` at commit `ef16297`
(clean tree; the results file is the only untracked change). The Python driver happened to run translated (Rosetta),
which the machine line says; the engine and the measured test are native arm64 builds. **The machine was shared and
loaded**: load average `11.5 / 31.5 / 39.3` before and `13.6 / 21.6 / 32.0` after the run (1, 5, 15 minutes). Modularity, communities, levels and work
units are deterministic and do not depend on the load; **the times do**, and are not measurements of speed. Raw rows:
[`communities-20261128T101504Z.jsonl`](communities-20261128T101504Z.jsonl) (first line: machine). Reproduce with
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
| layerweave-code | 40 | 6 | Louvain | 0.4366 | 0 / 0 | 35 | 22 | 18 | 2 | 11 539 |
| | | | Leiden | 0.4367 | 0 / 0 | 37 | 23 | 17 | 3 | 11 608 |

## The whole clustering, and the overview end to end

Clusters the overview adds (communities of at least 8 nodes that are not the whole container, every level),
clustering work, and the median time of the clustering alone (5 runs) and of the whole `overview` request through
`layout-command` (5 runs after a warm-up, threads 1, JSON in and out included). Times under the load above.

| dataset | algorithm | clusters added | clustering work | clustering ms | overview s (median, min–max) |
|---|---|---|---|---|---|
| overview-10000 | Louvain | 227 | 84 684 | 3.8 | 0.96 (0.94–0.98) |
| | Leiden | 249 | 78 710 | 4.0 | 0.96 (0.95–0.97) |
| overview-50000 | Louvain | 1 088 | 448 952 | 19.5 | 4.37 (4.29–4.87) |
| | Leiden | 1 238 | 411 973 | 20.3 | 4.22 (4.17–4.38) |
| overview-100000 | Louvain | 2 252 | 868 521 | 39.6 | 9.24 (9.01–9.85) |
| | Leiden | 2 534 | 804 771 | 41.7 | 10.01 (9.51–10.27) |
| overview-flat-2000 | Louvain | 128 | 207 811 | 2.5 | 0.28 (0.23–0.29) |
| | Leiden | 210 | 85 991 | 2.1 | 0.19 (0.19–0.20) |
| overview-flat-10000 | Louvain | 707 | 2 264 353 | 20.3 | 1.14 (1.05–1.16) |
| | Leiden | 1 126 | 484 698 | 12.8 | 0.88 (0.84–0.92) |
| overview-flat-50000 | Louvain | 3 944 | 17 599 472 | 171.6 | 6.43 (6.37–7.06) |
| | Leiden | 5 765 | 2 522 872 | 72.5 | 5.57 (5.46–5.90) |
| layerweave-code (thr. 40) | Louvain | 33 | 11 539 | 0.5 | – |
| | Leiden | 33 | 11 608 | 0.6 | – |
| organic-300 | Louvain | 24 | 9 770 | 0.2 | – |
| | Leiden | 26 | 9 022 | 0.1 | – |

## Findings

1. **Connectivity:** on none of these graphs did Louvain leave a community in parts, at any level (0 of 2 991
   communities). Leiden's guarantee holds (0 as well, and tested), but on this data it changes nothing. Louvain's flaw
   does occur: a deterministic search over 20 000 small generated graphs with planted groups found one case
   (`leiden_keeps_connected_what_louvain_leaves_in_parts` in `crates/layout/src/overview/leiden.rs`), so it is rare
   here, not absent.
2. **Modularity:** about equal. Leiden (greedy refinement) is within 0.1 % of Louvain on the hierarchical code index,
   0.5–0.9 % lower on the flat graphs, 0.4 % higher on `organic-300`, equal on `layerweave-code`.
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

## Note on the inputs

`layerweave-code` is generated from the repository's own source, so it changes with every commit; its rows belong to
the graph generated at `ef16297` (an earlier run on the uncommitted tree gave slightly different values for it). All
other inputs are generated deterministically (seed 1) and gave identical modularity, counts and work in both runs.
