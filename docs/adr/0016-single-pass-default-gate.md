# ADR 0016: The gate for making single pass the default

Date: 2026-11-07.
Status: accepted on 2026-11-08. Single pass with the automatic choice is the default from engine 0.2 on.

## Context

`singlePass` is an opt-in that rejects 26 of the 31 unmodified datasets. Its results so far are on a simplified task with a uniform
direction. CLAUDE.md accepts algorithm changes by quality gates, not fingerprints, and the choice of a default strategy is an open
decision. [The first measurement on the real task](../../benchmarks/results/single-pass-raw-20261106.md) is the starting point.

## Decision

Single pass may become the default where all of these hold, measured on the unmodified corpus against the refined default.

Hard, without tolerance:

- valid geometry: no geometry issue on a view where the default has none;
- complete edges in logical direction; cycles, self-loops and parallel edges work;
- stale results are rejected; output is identical for any thread count;
- every operation of `dispatch` works in all three deployment modes, falling back rather than failing.

Quality:

- per view, lexicographic in the order of `DEFAULT_OBJECTIVES`, over seed means: no view worse than the default without a documented
  reason in this ADR;
- line–node hits: no view worse than the default. CLAUDE.md asks for 0 hits, which the default itself misses on 4 views; this stays
  the target. Decided 2026-11-08: "not worse" suffices for the switch.

Deliberate actions:

- what the user placed or configured holds: a moved or pinned node, a direction, a size, a port. This is a gate.
- the movement of the other nodes is reported, not gated: the more a drawing is optimized, the less stable it may be on an edit.
  Measured so far: single pass moves other nodes further than the default on shape, measure and toggle.

A dataset for which the mode does not apply stays byte-identical to the default, apart from option and diagnostic fields.

elk-rs remains the reference for the single-pass target on the uniform task; on mixed views it is no reference, because the adapter
sets one direction at the root.

## Consequences

The switch itself raises `engine_version`, so that old snapshots are rejected explicitly rather than changing meaning: defaults are
omitted from serialized snapshots, and `layoutVersion` is the stale counter of a drawing, not a format version.

## Outcome (2026-11-08)

Measured over five seeds on the 16 unmodified datasets the automatic choice takes, single pass is better than the refined default
on all 16 ([gate](../../benchmarks/results/single-pass-gate-20261108.md)); no view has line–node hits in either pipeline. The 15
other datasets (organic, tree and straight areas, a view budget) fall back and stay byte-identical to the pipeline before, apart
from the option and diagnostic fields. Timing: [at the switch](../../benchmarks/results/single-pass-timing-20261108.md).

The switch: `singlePassAuto`, `singlePass` and `abstractRouting` are on by default and omitted; a fallback writes the two flags as
`false`. `ENGINE_VERSION` is `0.2.0-prototype`, so snapshots of 0.1 are rejected. The reference fingerprints of the new default
are [fingerprints-20261108-single-pass-default.jsonl](../../benchmarks/results/fingerprints-20261108-single-pass-default.jsonl);
`singlePassAuto: false` keeps the pipeline before, and its fingerprints stay those of
`fingerprints-20261021T103351Z.jsonl`.

Known consequences: a pin and a multi-node drag are not single-pass inputs, so such an edit continues on the pipeline before
(the container's inside is then laid out by it); the refinement plans no candidates on a single-pass drawing.
