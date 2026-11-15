# The repair at the end of a drag on a nested organic drawing (2026-11-15)

Machine: Apple M3, macOS (Darwin 25.0.0), not idle; single runs. Build: `cargo build --release -p arqlet-runtime --bins` at
faf3cce (before) and the same plus the change in `crates/layout/src/repair.rs` and `crates/layout/src/lib.rs`
(after), one after the other. Gesture on mixed-large (limit 150), as in `drag-steps-20261115.md`: begin at the first root
node (the circle container `mesh`), three updates of 40/20 units, finish with release; one `layout-command` process per step.

What a drop did (counted with temporary output): the probe for the lines into containers (moderate effort) used up its
8000 scans and was counted against the moderate budget, so the phase for the lines between children made 2 scans and left
its conflicts; two nodes stood before the root's wall (a drag step had pushed them there), which the repair treated as
conflicts of each node; and the rule that repairs fully after a moderate drop that leaves conflicts also fired on the lines
into containers that `nestedEdgesByDetour` leaves to the detours on purpose, and the full repair spent 88 000 scans pushing
the nodes from the wall one by one. Now:

1. An organic root none of whose nodes is pinned moves as a whole off the wall before the repair (a fresh layout never
   stands before it).
2. Below the full effort the phase for the lines between children counts its scans on its own, not after the probe's.
3. The full repair after a drop runs only for a conflict the detours do not draw around (an overlap, a node outside its
   area), not for straight edges through nodes when `straightDetours` is on.
4. An exhausted probe is kept and taken as the second phase when the first phase moved nothing (deterministic, the same
   result; here the containers' fit moved boxes, so it does not apply).

| forceDirected | | begin | updates | finish | sum | after the drop |
|---|---|---|---|---|---|---|
| off | before | 608 ms | 753/661/853 ms | 2026 ms | 4.9 s | 960 crossings, 226 bends, 33 055k, 1 issue |
| off | after | 267 ms | 69/68/144 ms | 737 ms | 1.3 s | 944 crossings, 438 bends, 28 501k, 0 issues |
| on | before | 1486 ms | 52/50/53 ms | 2161 ms | 3.8 s | 1135 crossings, 140 bends, 39 213k, 2 issues |
| on | after | 879 ms | 42/41/40 ms | 1399 ms | 2.4 s | 916 crossings, 121 bends, 35 088k, 1 issue |

Without force-directed behaviour the drop leaves the lines into containers to the detours (as `nestedEdgesByDetour` does at
an import), so it has more bends than the full repair that used to run; the refinement the playground runs in the
background after a drop takes it to 980 crossings, 141 bends, 21 057k, 0 issues (3.75 s, background).

Unchanged: every import (byte-identical on seven organic datasets; default fingerprints identical on all 15 views); 852 tests,
among them the stability tests of drags and pins (`interactive_continuation`, `circle_containers`, `drag_region`).
