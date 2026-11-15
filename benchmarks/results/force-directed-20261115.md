# Dragging with and without force-directed behaviour (2026-11-15)

Machine: Apple M3, macOS (Darwin 25.0.0), not idle; single runs. Build: `cargo build --release -p arqlet-runtime --bins` at 622152b plus the playground change (settling after a force-directed drop runs until it converges). Each dataset at limit 150; the root node with the most edges is dragged: begin, five updates of 60/30 units, finish with release; with `forceDirected` the drop is followed by `settle` calls until convergence (at most 60; the count in brackets), as the playground does. One `layout-command` process per step (about 10 ms of each is the process start). "Others moved": distance of every other root node from its place before the drag (mean / max / how many more than 20 units).

| dataset | forceDirected | worst drag step | drop | settle | total | crossings | bends | area (k) | issues | others moved: mean / max / >20 |
|---|---|---|---|---|---|---|---|---|---|---|
| petri-net-shapes | off | 23 ms | 42 ms | 0 ms | 160 ms | 27 | 0 | 1001 | 0 | 20 / 142 / 5/23 |
| petri-net-shapes | on | 9 ms | 39 ms | 181 (7) ms | 310 ms | 24 | 0 | 924 | 0 | 164 / 284 / 23/23 |
| mixed-layouts | off | 4 ms | 4 ms | 0 ms | 25 ms | 0 | 0 | 1256 | 0 | 0 / 0 / 0/5 |
| mixed-layouts | on | 4 ms | 10 ms | 78 (10) ms | 112 ms | 0 | 0 | 941 | 0 | 287 / 333 / 5/5 |
| self-loops | off | 5 ms | 3 ms | 0 ms | 28 ms | 0 | 30 | 361 | 0 | 17 / 51 / 2/5 |
| self-loops | on | 4 ms | 4 ms | 154 (21) ms | 182 ms | 0 | 30 | 305 | 0 | 168 / 327 / 5/5 |
| organic-25 | off | 11 ms | 5 ms | 0 ms | 39 ms | 0 | 0 | 1487 | 0 | 15 / 121 / 4/24 |
| organic-25 | on | 8 ms | 11 ms | 486 (30) ms | 539 ms | 0 | 0 | 825 | 0 | 375 / 778 / 24/24 |
| organic-50 | off | 17 ms | 17 ms | 0 ms | 91 ms | 6 | 0 | 2290 | 0 | 7 / 114 / 5/49 |
| organic-50 | on | 16 ms | 21 ms | 672 (19) ms | 781 ms | 2 | 0 | 2413 | 0 | 143 / 480 / 49/49 |
| organic-100 | off | 27 ms | 19 ms | 0 ms | 146 ms | 1 | 0 | 10903 | 0 | 6 / 205 / 5/99 |
| organic-100 | on | 33 ms | 39 ms | 3685 (42) ms | 3911 ms | 1 | 0 | 9453 | 0 | 891 / 1200 / 99/99 |
| organic-200 | off | 25 ms | 25 ms | 0 ms | 173 ms | 2 | 0 | 11125 | 0 | 0 / 0 / 0/149 |
| organic-200 | on | 49 ms | 53 ms | 5693 (39) ms | 6037 ms | 6 | 0 | 11989 | 0 | 741 / 1350 / 149/149 |
| mixed-large | off | 87 ms | 1075 ms | 0 ms | 1769 ms | 933 | 118 | 25299 | 4 | 61 / 294 / 5/18 |
| mixed-large | on | 38 ms | 3777 ms | 6561 (60) ms | 11022 ms | 658 | 0 | 11542 | 192 | 4665 / 5408 / 18/18 |

Reading: with force-directed behaviour every other node moves (means of 140 to 900 units) and the settling costs up to 6 s of frames on 100 to 200 nodes; the drawing usually takes less area (8 to 45 %) and sometimes fewer crossings, but organic-200 got 4 crossings more. On mixed-large (organic root over nested containers, where the forces are left out and the continuation relaxes with the stress) it does not converge within 60 calls and leaves 192 issues. Without it the other nodes stay (means of 0 to 61 units), a drag is fast and valid (mixed-large keeps 4 issues after this drag).
