# Cluster overview on real exports (2026-11-11)

Two Intentweave exports, made with `tools/intentweave-export` (`npm run export -- --db … --out … --claims`) from local
indexes; the exports are not in the repository (`exports/` is ignored). The times below are single observations on a
machine under load from builds, not measurements.

| export | nodes | kinds | resolved edges (unresolved left out) | clusters | levels | overlaps | native, 4 threads | browser (WASM worker, incl. transfer) |
|---|---|---|---|---|---|---|---|---|
| Intentweave | 8 874 | 175 directories, 786 files, 7 913 symbols | 382 (17 167) | 1 356 | 10 | 0 | 0.29 s | 0.73 s |
| Backstage | 31 758 | 2 123 directories, 9 615 files, 20 020 symbols | 10 168 (29 043) | 8 436 | 14 | 0 | 1.19 s | 2.41 s |

## Findings, and what changed because of them

- **A single root.** Both exports have one root directory (`.`), and below it one or two directories hold most of
  the code (`packages`, or `packages` and `plugins`). Colouring by the top level made everything one or two colours.
  The colours now come from the clusters left after opening, again and again, the largest cluster that holds more than
  a quarter of the graph (only clusters whose children include clusters): one colour per package or plugin.
- **Mostly unlinked containers.** Real indexes resolve few edges inside a file of type declarations (Intentweave's
  `types.ts`: about 180 interfaces without links between them). Community detection gathered them in one `unlinked`
  cluster, a level without meaning. A container whose children are mostly without links is no longer split (test).
- **Symbols are containers too** (an interface holds its fields); the overview and the drill-down handle that.
- **A double-click did nothing** on the page: a parameter named `zoomInto` hid the function of that name (fixed).
- **Open:** a file without internal edges laid out structured is one long row of boxes (the layered layout puts
  unlinked nodes in one rank), which is tiny in its disc. The box pipeline needs a grid packing of unlinked nodes.
