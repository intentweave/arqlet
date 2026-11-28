# Engine package: the second WASM module with the operation `query` (#210): size

Machine: Apple M3 (macOS), rustc 1.99.0 (b940084d7 2026-09-28). Revision: cb7a666 (version 0.10.0)
plus the change that adds `arqlet-engine-query.wasm` to `packages/engine-client`.
Measured 2026-11-28 on the files `tools/npm-prepack.mjs` builds: `arqlet-wasm` in the `wasm-size` profile,
`wasm32-unknown-unknown`, `--locked`, without and with `--features query`. gzip is `gzip -9`.

| Module | Raw bytes | gzip bytes |
|---|---|---|
| `arqlet.wasm` (default, unchanged) | 7 726 587 | 1 908 232 |
| `arqlet-engine-query.wasm` (`--features query`) | 8 572 478 | 2 177 835 |
| difference | +845 891 (+10.9 %) | +269 603 (+14.1 %) |

A host that does not ask for the second module downloads nothing more than before. A host that does downloads this module
instead of the default one (it contains the whole engine), not both. For comparison, the query-only module of
`@arqlet/query` was 1 082 369 B raw, 344 603 B gzip (`query-npm-20261125.md`).

`npm pack --dry-run` of `@arqlet/engine-client` lists both modules and both notice files (`THIRD_PARTY_NOTICES.md` 71.7 kB,
`THIRD_PARTY_NOTICES-query.md` 105.2 kB): package size 4.2 MB, unpacked 16.8 MB, 24 files.

Not measured: load time, query time and memory in a browser.
