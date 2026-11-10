# Straight-edge repair on mixed diagrams, 2026-10-25

Machine: Apple M3, macOS 26.0. Build: debug test profile (`cargo test`), `--locked`.
Revision: base `8efd358` (main); change on branch `layout/shape-contours` (uncommitted at the time of the run).
Command: `cargo test -p layerweave-layout --locked straight_conflicts_on_mixed_diagrams -- --ignored --nocapture`
Input: `crates/layout/src/repair.rs` (`diagram`), seeds 1 to 20, 14 nodes, 18 straight edges, every third node a circle, the
rest rectangles, organic root, `defaultRouting: straight`, default options. Edges come from a fixed LCG; one edge per node pair.

Columns: `through` = drawn straight-edge conflicts, i.e. `straight edge … passes through node …` from
`apply_straight_edges` on the final geometry (the authority for straight edges); `overlap` and `clearance` = node pairs
whose real outlines collide or come closer than 12 (a pair counts once: collision or clearance); `unresolved` = geometry
issues the repair reports (its check is a superset: it also judges the endpoints and the clearance).

Caveat on the earlier figure: a first count (148 for the same seeds) mixed in `Edge … passes through node …` messages from
`labels::decorate`, which runs before the straight edges are applied and therefore describes the orthogonal routes. That
count is not used here.

## Before: repair disabled (`repair_root` returns no issues; warm start and pin handling are present but inactive here)

```
seed  1 through  0 overlap 0 clearance 0 unresolved 0
seed  2 through  0 overlap 0 clearance 0 unresolved 0
seed  3 through  0 overlap 0 clearance 0 unresolved 0
seed  4 through  2 overlap 0 clearance 0 unresolved 0
seed  5 through  2 overlap 0 clearance 0 unresolved 0
seed  6 through  1 overlap 0 clearance 0 unresolved 0
seed  7 through  0 overlap 0 clearance 0 unresolved 0
seed  8 through  0 overlap 0 clearance 0 unresolved 0
seed  9 through  3 overlap 0 clearance 0 unresolved 0
seed 10 through  1 overlap 0 clearance 0 unresolved 0
seed 11 through  3 overlap 0 clearance 0 unresolved 0
seed 12 through  1 overlap 0 clearance 0 unresolved 0
seed 13 through  4 overlap 0 clearance 0 unresolved 0
seed 14 through  1 overlap 0 clearance 0 unresolved 0
seed 15 through  2 overlap 0 clearance 0 unresolved 0
seed 16 through  0 overlap 0 clearance 0 unresolved 0
seed 17 through  1 overlap 0 clearance 0 unresolved 0
seed 18 through  2 overlap 0 clearance 0 unresolved 0
seed 19 through  0 overlap 0 clearance 0 unresolved 0
seed 20 through  1 overlap 0 clearance 0 unresolved 0
total through 24 overlap 0 clearance 0 unresolved 0
```

## After: repair on the drawn geometry

```
seed  1 through  0 overlap 0 clearance 0 unresolved 0
seed  2 through  0 overlap 0 clearance 0 unresolved 0
seed  3 through  0 overlap 0 clearance 0 unresolved 0
seed  4 through  0 overlap 0 clearance 0 unresolved 0
seed  5 through  1 overlap 0 clearance 0 unresolved 1
seed  6 through  0 overlap 0 clearance 0 unresolved 0
seed  7 through  0 overlap 0 clearance 0 unresolved 0
seed  8 through  0 overlap 0 clearance 0 unresolved 1
seed  9 through  1 overlap 0 clearance 0 unresolved 1
seed 10 through  0 overlap 0 clearance 0 unresolved 1
seed 11 through  1 overlap 0 clearance 0 unresolved 1
seed 12 through  1 overlap 0 clearance 0 unresolved 1
seed 13 through  0 overlap 0 clearance 0 unresolved 0
seed 14 through  0 overlap 0 clearance 0 unresolved 0
seed 15 through  0 overlap 0 clearance 0 unresolved 0
seed 16 through  0 overlap 0 clearance 0 unresolved 0
seed 17 through  1 overlap 0 clearance 0 unresolved 1
seed 18 through  2 overlap 0 clearance 0 unresolved 3
seed 19 through  0 overlap 0 clearance 0 unresolved 0
seed 20 through  0 overlap 0 clearance 0 unresolved 0
total through 7 overlap 0 clearance 0 unresolved 10
```

## Reading

- Drawn straight-edge conflicts: 24 before, 7 after. Every remaining one is also a reported geometry issue.
- Overlaps and clearance violations: 0 before and after (the stress separation already keeps boxes apart on these inputs).
- `unresolved` (10) is larger than `through` (7) because the repair also judges endpoints and the clearance band; no
  conflict is silent.
- Not measured here: performance (the search evaluates the whole drawing per candidate; no timing is claimed),
  unchanged-input drift of the warm start, nested organic areas, and the default-configuration fingerprints (see the
  specification, section 8).

## Second step: depth objective, jumps, warm start without stress (same inputs and machine)

```
total through 0 overlap 0 clearance 0 unresolved 1
```

Drawn straight-edge conflicts 24 -> 0; one geometry issue remains and is reported. The search now ranks a move by conflict
count, then total conflict depth, then displacement, and tries steps up to eight base units. A warm start keeps the previous
positions and runs no stress iterations; the measurement of unrelated-node displacement over move, resize, shape change and a
second move on 8 diagrams (`interactive_continuation`, debug build) gave a median of 0 for the warm start against 39 to 136
units for a layout from scratch. Timings of that test are debug-build figures and are not a performance claim.

## Default-output check (fingerprints), 2026-10-25

Base `8efd358` against branch `layout/shape-contours` (commit d1e353c), release `bench` binaries built with `--locked` from each
revision, one repetition (only the output fingerprints matter), refinement threads 1 and 4, the datasets of `benchmarks/run.sh`
except `synthetic-20k`. Apple M3, macOS 26.0. Compared with `python3 benchmarks/fingerprint_compare.py`:

```
summary: identical 19
```

All 19 configurations are identical in `refineFingerprint` and `geometryFingerprint`, and each run is identical across thread
counts. Raw runs: `fingerprint-check-shape-contours-base-20261025.jsonl` and `...-branch-20261025.jsonl`. Scope: these
datasets use the default routing (orthogonal) and strategies; the repair and the warm start act only on an organic root with
straight candidates or on a session recomputation, which the sweep does not exercise. It shows that the defaults are unchanged,
not that the new paths are right (the tests cover those).

### Repeated after the diagnostics order change

The branch run was repeated with the change that derives the labels and route diagnostics after the straight segments (and the
oblique rectangle hit test): `summary: identical 19` against the same base runs. The stored branch file is the repeated run.

### Repeated after the typed geometry update

The branch run was repeated after `LayoutOptions` gained `geometryRevisions` (omitted when empty) and `measure` took
`measurements`: `summary: identical 19`. The `layerweave-code` dataset follows the repository state, so it changed between the
first base run and this one; the base was run again for those three configurations on the current dataset file and is what the
stored base file holds for them. The other 16 base rows are from the first run.
