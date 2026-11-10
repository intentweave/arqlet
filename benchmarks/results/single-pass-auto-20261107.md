# Automatic path choice for single pass, 2026-11-07

M1 of the plan to make `singlePass` the default. Revision `14d64b2` plus these changes, Apple M3, release builds. Instruction counts
only; no time is claimed.

## What changed

`singlePassAuto` (provisional, omitted by default) uses single pass where it works and the other pipeline where it does not, with a
diagnostic `Single pass was not used: <reason>`.

- An import, or a `configure` that sets the option, turns on `abstractRouting` and `singlePass`. A failure turns them off in the
  result, so these two options always say which pipeline made the drawing. Refinement and later edits follow without further logic.
- An edit does not return to single pass. Only a new import or a `configure` that sets the option does, which keeps an edit from
  changing pipeline twice.
- Every failure of the attempt falls back, not only the known unsupported cases. A failure of the other pipeline is then the error
  the caller sees. Without the option an unsupported request stays an explicit error.
- `single_pass_cannot_run` rejects what the graph already shows before the attempt: routing overrides, tree, radial and organic
  areas, reversed or mixed fixed directions. Areas that choose their direction go through the attempt.

This is simpler than the plan, which had a separate snapshot field and a typed error.

## Checks

- `node benchmarks/eval/single-pass-auto-check.mjs`: 31 unmodified datasets at 150 nodes. 5 on single pass, 26 fallback, 0 problems. All
  import and refine. All 26 are byte-identical to the default before and after refinement, apart from the option and the report.
- `crates/runtime/tests/single_pass.rs`: every edit the default accepts works on a single-pass drawing, including `port`, `restore`
  and `refineProgressive`; an edit does not return to single pass; stale requests stay rejected; refinement is identical at threads 1
  and 4.
- `apps/playground/ui-regression.mjs`: the option, a pin that falls back, and turning it off, in split, browser-only and backend-only
  mode. `smoke.mjs`: WASM and native match, also after a fallback.
- Default fingerprints: 19 configurations of the sweep, `bench --reps 1 --threads 1,4`, against a build of `d14db45`: all identical,
  and identical for both thread counts.

## Cost of a fallback

Retired instructions per import, unmodified views at 150 nodes, default → `singlePassAuto`, (T(3) − T(1)) / 2:

| View | without the pre-check | with the pre-check |
|---|---:|---:|
| mixed-large | 530732 → 1036189 M (+95 %) | 530732 → 530714 M |
| org-chart | 226 → 263 M (+16 %) | 226 → 232 M (+2.6 %) |
| workflow-dag | 174 → 197 M (+13 %) | 173 → 173 M |
| synthetic-2k | 8353 → 8565 M (+3 %) | 8358 → 8357 M |
| tree-100 | 46 → 46 M | 46 → 46 M |

mixed-large costs 531 G instructions per import on the default path, because of its organic areas. That is a finding about the
default, outside this work.

## Deliberate moves

A drag and a pin on a single-pass drawing now work, and the node stays where it was put (test
`a_deliberate_move_and_a_pin_hold_on_a_single_pass_drawing`). But single pass has no pins, so the first step of a drag leaves it, and
the drawing stays on the other pipeline. The other nodes move about as far as on the default, or further: moved nodes / median /
maximum on the code view, default 75 / 1369 / 2502, automatic 75 / 1650 / 2365; workflow 70 / 77 / 508 and 70 / 286 / 824; org chart
103 / 1654 / 5485 and 103 / 3690 / 5666. Raw: [single-pass-edits-auto-20261107.json](single-pass-edits-auto-20261107.json). One sample
per view.

This is the fallback, not the goal: a drag loses the quality of single pass for the rest of the session. Supporting pins and
deliberate moves in single pass itself is open.
