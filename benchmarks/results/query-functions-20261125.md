# The query module with the function table and UNION: size

Machine: Apple M3 (macOS), rustc 1.99.0. Revision: fc40597 plus the change that adds the scalar functions and `UNION`.
Measured 2026-11-25. `cargo build -p arqlet-query-wasm --target wasm32-unknown-unknown --profile wasm-size --locked`;
raw bytes of the `.wasm` file; gzip is `gzip -9`.

| Module | Raw | gzip |
|---|---|---|
| before ([query-npm](query-npm-20261125.md)) | 1 082 369 B | 344 603 B |
| with the function table and `UNION` | 1 105 873 B | 352 857 B |
| difference | +23 504 B (+2.2 %) | +8 254 B (+2.4 %) |

## Not measured

The engine with the cargo feature `query`; the time of a query, with or without functions; memory.
