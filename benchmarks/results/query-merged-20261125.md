# The query module as of the merge of the functions and expressions: size

Machine: Apple M3 (macOS), rustc 1.99.0. Revision: the merge of 9e629d1 (`main`) into 0fe4bd0 (functions, `UNION`, maps,
list expressions and the fixes from their reviews). Measured 2026-11-25.
`cargo build -p arqlet-query-wasm --target wasm32-unknown-unknown --profile wasm-size --locked`; raw bytes of the
`.wasm` file; gzip is `gzip -9`.

| Module | Raw | gzip |
|---|---|---|
| `arqlet-query-wasm`, the module of `@arqlet/query` | 1 158 280 B | 374 118 B |

The steps before it: [the package](query-npm-20261125.md), [functions](query-functions-20261125.md),
[maps and list expressions](query-expressions-20261125.md).
