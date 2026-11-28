# octilinearRefine: cost before and after the work budget (2026-11-28)

Indicative, not a benchmark: one run each on a loaded machine (load average 7.5 on 8 cores, another timing baseline
running at the same time).

- Machine: Apple M3, 8 cores, 16 GB; macOS (Darwin 25.0.0).
- Build: `cargo test -p arqlet-layout --profile fast --lib -- --ignored octilinear_refinement_timing --nocapture`
  (rustc 1.99.0, `fast` profile: release optimisation without LTO), `CARGO_BUILD_JOBS=2`.
- Revision: branch `octilinear-refine`. Before: `c7327d7` (refinement without budget). After: the commit that adds this
  file (lexicographic cost, line passes only where a line has two links at a node, branches of at most 32 nodes, work
  budget of 40 000 000 units per group).
- Input: `tree::tests::octilinear_refinement_timing`, `octilinear_arrange` alone (no routing, no runtime), boxes of
  100×40, every link on one line (the edge kind, as without explicit buses): a star (one hub), a path, and a random graph
  (a spanning tree plus n/2 links from a fixed linear congruential sequence). Groups above 400 nodes are not refined.

| Shape | Nodes | Star placement | Refined before | Refined after |
|---|---|---|---|---|
| star | 200 | 0.001 s | 5.547 s | 0.103 s |
| star | 400 | 0.001 s | 65.884 s | 0.133 s |
| path | 400 | 0.075 s | 6.906 s | 0.216 s |
| random | 400 | 0.002 s | 7.657 s | 0.238 s |
| random | 1000 | 0.003 s | 0.003 s (not refined) | 0.003 s (not refined) |

The times include the star placement. Quality on the made-up city of the example *Metro map*, placed as an octilinear
star (`tree::tests::the_octilinear_refinement_schematises_a_metro_map`), is the same before and after: 1 skewed link,
3 crossings, 1 station on a link and 8 bends become 0, 0, 0 and 3, for 2 lattice steps more. On a graph that exhausts the
budget the search stops part way, at the same move for the same input, so the result is improved but may not be a local
optimum.
