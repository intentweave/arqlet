# Straight edges around nodes (`straightDetours`, experimental), 2026-11-14

Apple M3, Darwin 25.0.0 arm64, rustc 1.99.0, Node 24.21.0; release `layout-command` from the branch
`single-pass-regions` (ADR 0020, step S1-1b). Command: `node benchmarks/eval/straight-detours.mjs [--json]` (raw rows:
`straight-detours-20261114.jsonl`); single runs. Metrics: the shared `metrics` operation, node shapes included (a circle
is its disc).

| dataset | variant | line–node hits | crossings | bends | area (k) | straight-through-node | ms |
|---|---|---|---|---|---|---|---|
| mixed-large | default | 44 | 721 | 0 | 50888 | 51 | 7745 |
| mixed-large | straightDetours | 0 | 811 | 68 | 50888 | 0 | 7723 |
| mixed-large | singlePassMixed | 67 | 750 | 0 | 27185 | 75 | 12473 |
| mixed-large | singlePassMixed + straightDetours | 4 | 954 | 120 | 27185 | 4 | 12454 |
| mixed-layouts | default | 0 | 5 | 0 | 1330 | 0 | 20 |
| mixed-layouts | straightDetours | 0 | 5 | 0 | 1330 | 0 | 20 |
| mixed-layouts | singlePassMixed | 0 | 5 | 0 | 1330 | 0 | 20 |
| mixed-layouts | singlePassMixed + straightDetours | 0 | 5 | 0 | 1330 | 0 | 21 |
| self-loops | default | 0 | 0 | 30 | 255 | 0 | 5 |
| self-loops | straightDetours | 0 | 0 | 30 | 255 | 0 | 5 |
| self-loops | singlePassMixed | 0 | 0 | 30 | 255 | 0 | 5 |
| self-loops | singlePassMixed + straightDetours | 0 | 0 | 30 | 255 | 0 | 5 |
| can-bus | default | 0 | 0 | 32 | 1315 | 0 | 8 |
| can-bus | straightDetours | 0 | 0 | 32 | 1315 | 0 | 7 |
| can-bus | singlePassMixed | 0 | 0 | 27 | 1305 | 0 | 12 |
| can-bus | singlePassMixed + straightDetours | 0 | 0 | 27 | 1305 | 0 | 12 |
| nested-tree-radial | default | 0 | 0 | 112 | 42157 | 0 | 9 |
| nested-tree-radial | straightDetours | 0 | 0 | 112 | 42157 | 0 | 9 |
| nested-tree-radial | singlePassMixed | 0 | 0 | 112 | 42157 | 0 | 9 |
| nested-tree-radial | singlePassMixed + straightDetours | 0 | 0 | 112 | 42157 | 0 | 9 |
| petri-net-shapes | default | 0 | 27 | 0 | 1614 | 0 | 540 |
| petri-net-shapes | straightDetours | 0 | 27 | 0 | 1614 | 0 | 538 |
| petri-net-shapes | singlePassMixed | 0 | 27 | 0 | 1614 | 0 | 541 |
| petri-net-shapes | singlePassMixed + straightDetours | 0 | 27 | 0 | 1614 | 0 | 545 |

Reading:

- mixed-large (an organic root holding layered containers): its straight edges join nodes nested in different
  containers and ran through their siblings, which the root's placement cannot move. With the option each such edge
  takes its orthogonal route pulled taut around every node it does not lie in: 44 lines through nodes
  become 0, for 68 more bends and 90 more crossings
  (lines through nodes come before both in the objective order); the placement and the area do not change, and edges
  with a free line stay one segment. The geometry findings follow the drawn routes.
- The other mixed examples have no such edge and do not change.
- Under `singlePassMixed` some edges reach the straight-edge stage without an orthogonal route (only their straight
  segment), so they cannot take a detour and stay reported; `singlePassMixed` stays off (ADR 0020, S1-2).
