# Single pass against elk-rs, 2026-11-07

Revision `b2835a0` plus the crossing-aware order across axis borders, Apple M3, release builds. Single pass: mean of five shuffle
seeds (from an uncommitted copy that reads the seed from the environment); elk-rs (`@openedges/elk-layout`, pinned in
`benchmarks/eval/package-lock.json`) is deterministic, one run, layered, orthogonal routing. Both drawings are measured by the same
`metrics` operation. Per view, lexicographic in the order of `DEFAULT_OBJECTIVES` (sibling overlaps, line–node hits, shared line
pairs, crossings, hugging pairs, bends, length). No time was measured here; the last timing is in
[the levers report](single-pass-levers-20261106.md) (frozen code view 59 + 13 ms against 44 ms for elk-rs).

Raw: [single-pass-elk-20261107.json](single-pass-elk-20261107.json). Commands: `node benchmarks/eval/single-pass.mjs --corpus --arms
singlePass` (uniform), the same without `--corpus` (frozen views), and `--corpus --raw --arms default,singlePass` on the 16 datasets
an explicit single pass lays out.

## 1. Uniform task (31 normalized views, 150-node cut)

The task elk-rs solves: one direction, orthogonal routing everywhere. Single pass better on 21, worse on 10.

| Sum over the 31 views | single pass | elk-rs |
|---|---:|---:|
| crossings | 4167 | 6584 |
| bends | 6124 | 5620 |
| line–node hits | 95 | 0 |
| shared line pairs | 6 | 0 |

The 10 worse:

- by line–node hits (mixed-large 46, petri-net-shapes 35, mixed-layouts 7, self-loops 7): circles and organic shapes in the normalized
  views; the default has the same hits.
- by crossings: deployment-cluster 95 / 71, org-chart 32 / 23, layerweave-pipeline 6.8 / 5, fixed-ports 1 / 0, and organic-200,
  organic-300 (a few crossings against none).

On the large views single pass has far fewer crossings: synthetic-2k 1749 / 3660, layerweave-code at 204 nodes 1380 / 1716, petri-net
32 / 58. elk-rs has fewer bends on most views and many hugging pairs (which rank above bends, so trees and organic views count as
better for single pass by hugging alone).

## 2. Frozen views

| View | single pass (crossings / bends) | elk-rs |
|---|---|---|
| layerweave-code, 188 nodes | 3784 / 1634 | **3057** / 1638 |
| workflow-dag | **232** / 452 | 278 / **434** |
| org-chart | 32 / 186 | **23 / 148** |

## 3. Real task (datasets as they are)

The adapter now gives elk-rs the containers' own directions (`containerDirections`), and lays mixed ones out with
`SEPARATE_CHILDREN`, which is how ELK takes different directions per container.

- **elk-rs fails on all 11 datasets with mixed `lr`/`tb` containers**: `UnsupportedGraphException: The source of edge … could not be
  found in the current layout scope. An edge cannot cross into a different layout algorithm.` With separate children, ELK does
  not route an edge between containers of different layout scopes. With `INCLUDE_CHILDREN` it takes one direction for all, which is
  the uniform task of section 1.
- On the 5 datasets with one axis single pass is better on 3 (layerweave-code 1380 / 1716 crossings, petri-net, state-tcp) and worse on
  2 (er-diagram by one shared line pair, fixed-ports by one crossing).
- On the 11 mixed datasets single pass draws every view, against the refined default (seed 0): see
  [legs](single-pass-legs-20261107.md); crossings remain above the default's (workflow-dag 1526 / 581, synthetic-2k 5031 / 3868).

## Reading

On the uniform task single pass is ahead of elk-rs on crossings and behind on bends and hits; on two of the three frozen views elk-rs is
better. On the real task with mixed directions elk-rs gives no drawing at all, so the reference there can only be the default.
