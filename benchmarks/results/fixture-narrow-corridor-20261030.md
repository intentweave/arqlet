# Fixture: narrow corridor and the reserved width, 2026-10-30

Test: `a_narrow_corridor_needs_the_reserved_width_and_no_more` (crates/layout/src/constructive.rs). Machine: Apple M3, revision
0bcfd56 plus uncommitted changes. Deterministic (no timing).

Setup: source box at x 0 (width 40), target box at 40 + gap, LR flow, one edge, required clearance 6 and track spacing 6
(margin 12). Widening moves the target by the extra width, as `corridor_gaps` does.

Result:
- gap 20: the edge has no free track (NoFreeTrack).
- the smallest widening that carries it is between 4 and 4.5 units, i.e. a gap of 24 + ε, which is two margins (2 × 12).
- every wider corridor carries it as well (checked at +6 and +30).

What this does not show: the widening is applied by moving the boxes in the test, not by the layout. The placement reads
`corridor_gaps` from an earlier snapshot only for partial edits, and it computes new widths from finished routes, not from the
constructor's demand. Connecting the two is the next step. The other fixtures of the approved set (fan-out, long edge past an
intermediate node, local resize) are not yet built.
