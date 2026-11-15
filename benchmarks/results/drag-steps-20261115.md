# Drag steps on a nested organic drawing (2026-11-15)

Machine: Apple M3, macOS (Darwin 25.0.0), not idle; single runs, times vary. Build: `cargo build --release -p
arqlet-runtime --bins` at 7e27e27 (before) and the same plus the change in `crates/layout/src/repair.rs`
(after), built and measured one after the other. Gesture on mixed-large (limit 150): begin at the first root node's place,
three updates of 40/20 units each, finish with release; one `layout-command` process per step (about 10 ms of it is the
process start). The issues are the geometry issues the step reports.

| forceDirected | | begin | update 1 | update 2 | update 3 | finish | sum |
|---|---|---|---|---|---|---|---|
| off | before | 608 ms | 753 ms (157) | 661 ms (157) | 853 ms (154) | 2026 ms (3) | 4.9 s |
| off | after | 542 ms | 75 ms (163) | 74 ms (165) | 154 ms (2) | 3833 ms (1) | 4.7 s |
| on | before | 1486 ms | 52 ms (1) | 50 ms (1) | 53 ms (1) | 2161 ms (2) | 3.8 s |
| on | after | 961 ms | 43 ms (1) | 42 ms (1) | 41 ms (1) | 1883 ms (2) | 3.0 s |

Cause: a step of a drag repairs with the local effort (a small scan budget), but the lines into containers
(`nestedEdgesByDetour`, `Lines::Bounded`) always took their own budget of 8000 scans, also in a drag step. That budget is now
capped by the effort. The drag steps are about 8 times faster with as many issues left open as before; the finish
(moderate effort) does the work the steps leave, so it takes longer without force-directed behaviour.

Tried and dropped: a drag step that clears only the lines between children of organic areas (as fast, but a pin set after
a session of drops then moved a node of a circle container by 20 units: `circle_containers` test); skipping the layout of a
begin that grabs a node where it stands (the same test failed).

Import and refinement are unchanged (full effort): default fingerprints identical on all 15 views; the import outputs of
seven organic datasets byte-identical; 852 tests.
