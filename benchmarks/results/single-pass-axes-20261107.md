# Single pass on mixed axes, first slice (M2a), 2026-11-07

Revision `f77821f` plus these changes, Apple M3, release builds. No time is claimed.

## What changed

- The channel growth (`growth::widen_planned`) grows every area along its own flow instead of the root's. Children finish first; a
  child area whose flow runs across its parent's has grown across the parent's flow, and the elements after it in its rank move on
  by that growth. An area grows along and across its flow by what its content grew. With one axis everywhere nothing grows across
  a flow, so the output does not change: single pass on the 31 normalized views and the 3 frozen views is byte-identical to
  `f77821f` (34 of 34).
- `growth::check_capability` accepts areas of different axes; reversed directions are still rejected.
- An explicit `singlePass` lays out the 11 datasets with mixed `lr`/`tb` containers that it rejected before.
- The automatic choice (`singlePassAuto`) still keeps mixed axes on the other pipeline, because of the quality below.

## Quality on the 11 mixed datasets

`node benchmarks/eval/single-pass.mjs --corpus --raw --arms default,singlePass` on the 11 datasets, five shuffle seeds from an
uncommitted copy, against the refined default. Lexicographic in the order of `DEFAULT_OBJECTIVES`. Raw:
[single-pass-axes-20261107.json](single-pass-axes-20261107.json).

3 better (cloud-architecture, org-chart, state-order), 8 worse (5 by shared line pairs, 3 by crossings). No line–node hits in
either arm; network-vlan has 0.4 routing issues on average against none.

The cause is the edges between areas of different axes. Delegation reasons from an instrumented copy (built with
`benchmark-timings`), on the engine's seed:

| Dataset | constructed | delegated | of which `MixedFlow` | `MissingBorder` | self-loop |
|---|---:|---:|---:|---:|---:|
| org-chart | 0 | 82 | 77 | 5 | 0 |
| workflow-dag | 0 | 125 | 125 | 0 | 0 |
| synthetic-2k | 85 | 233 | 165 | 56 | 12 |
| network-vlan | 28 | 19 | 19 | 0 | 0 |

Delegated edges are drawn as seed routes, which share lines and cross more. On org-chart and workflow-dag every edge between two
containers crosses an axis border.

## Next

M2b: construct an edge across an axis border. A child area whose flow runs across its parent's has to be left on the sides that
the parent's flow uses (for a `tb` child in an `lr` parent: right and left, not bottom), else the route lands in the parent's rank
below the child. That is a new kind of border section, from a channel of the child to its side, and the border nodes of such a
child move to those sides. `MissingBorder` is a separate case.
