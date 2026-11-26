# The query module with maps and list expressions: size

Machine: Apple M3 (macOS), rustc 1.99.0. Revision: 96f59f3 plus the change that adds maps, list comprehension, indexing,
slicing and `+` on lists. Measured 2026-11-25.
`cargo build -p arqlet-query-wasm --target wasm32-unknown-unknown --profile wasm-size --locked`; raw bytes of the
`.wasm` file; gzip is `gzip -9`.

| Module | Raw | gzip |
|---|---|---|
| before ([query-functions](query-functions-20261125.md)) | 1 105 873 B | 352 857 B |
| with maps and list expressions | 1 156 240 B | 373 051 B |
| difference | +50 367 B (+4.6 %) | +20 194 B (+5.7 %) |

## Not measured

The engine with the cargo feature `query`; the time and the memory of a query.
