# Border ports for regions (`regionPorts`, experimental), 2026-11-14

Apple M3, Darwin 25.0.0 arm64, rustc 1.99.0, Node 24.21.0; release `layout-command` from the branch
`single-pass-regions` (ADR 0020, step S1-2). Command: `node benchmarks/eval/region-ports.mjs [--json]` (raw rows:
`region-ports-20261114.jsonl`); single runs, so the times are indicative only. The graphs are synthetic and deterministic
(generator and seeds in the script): a root flowing left to right through layered stages and region containers of
the named strategies, with edges into every region, out of it, one back edge and one edge passing it. The existing corpus
has almost no such case (only can-bus, which has buses and is left alone by the option). Metrics: the shared `metrics`
operation with node shapes. "Border ends off side": ends of edges that cross a region's border on a side other than the
one the root's flow enters (left) or leaves (right) through, of all such ends.

| shape | variant | nodes/edges | line–node hits | crossings | bends | area (k) | border ends off side | single pass | geometry issues | ms |
|---|---|---|---|---|---|---|---|---|---|---|
| organic-only | default | 27/37 | 0 | 12 | 52 | 1265 | 0/14 | no | 0 | 17 |
| organic-only | singlePassMixed | 27/37 | 0 | 12 | 34 | 1374 | 3/14 | yes | 0 | 20 |
| organic-only | regionPorts | 27/37 | 0 | 13 | 57 | 1265 | 0/14 | no | 0 | 25 |
| organic-only | singlePassMixed + regionPorts | 27/37 | 0 | 13 | 48 | 1374 | 0/14 | yes | 0 | 32 |
| tree-only | default | 25/28 | 0 | 13 | 70 | 943 | 0/14 | no | 0 | 4 |
| tree-only | singlePassMixed | 25/28 | 0 | 19 | 58 | 1046 | 2/14 | yes | 0 | 5 |
| tree-only | regionPorts | 25/28 | 0 | 10 | 71 | 943 | 0/14 | no | 0 | 5 |
| tree-only | singlePassMixed + regionPorts | 25/28 | 0 | 10 | 63 | 1046 | 0/14 | yes | 0 | 5 |
| radial-only | default | 25/28 | 0 | 11 | 48 | 2019 | 0/14 | no | 0 | 6 |
| radial-only | singlePassMixed | 25/28 | 0 | 9 | 38 | 2190 | 3/14 | yes | 0 | 9 |
| radial-only | regionPorts | 25/28 | 0 | 11 | 50 | 2019 | 0/14 | no | 0 | 9 |
| radial-only | singlePassMixed + regionPorts | 25/28 | 0 | 11 | 42 | 2190 | 0/14 | yes | 0 | 9 |
| mixed-chain | default | 60/80 | 0 | 49 | 140 | 5814 | 2/36 | no | 0 | 82 |
| mixed-chain | singlePassMixed | 60/80 | 0 | 40 | 118 | 6068 | 11/36 | yes | 0 | 86 |
| mixed-chain | regionPorts | 60/80 | 0 | 46 | 128 | 5814 | 0/36 | no | 0 | 127 |
| mixed-chain | singlePassMixed + regionPorts | 60/80 | 0 | 46 | 112 | 6068 | 0/36 | yes | 0 | 150 |

Reading:

- Every variant with `regionPorts` keeps the border contract (no end off side); `singlePassMixed` alone has 2 to 11 off
  side, because its edges into regions are delegated and completed by the older router, which may run around the box
  and enter it from above or below.
- Against the pipeline before single pass (default): fewer bends on every graph (48 against 52, 63 against 70, 42
  against 48, 112 against 140 with `singlePassMixed`), crossings equal or fewer except organic-only (13 against 12).
- Against `singlePassMixed` alone: tree-only crosses far less (10 against 19); organic-only, radial-only and mixed-chain
  cross more (by 1, 2 and 6) and mixed-chain bends less (112 against 118). Where the ports follow the nodes inside, the
  border edges no longer cross inside the region (on mixed-chain 16 such crossings become 1), but more outside, where the
  port order no longer follows the outer ends.
- Each layout runs twice (automatic ports, then ports at the nodes' places) and keeps the better by the published key,
  so it takes about twice as long.
- Open: a port order that weighs the crossings inside and outside (a rank sum of both orders was worse on these graphs:
  61 crossings on mixed-chain), regions inside regions, and regions at an organic root (S1-3).
