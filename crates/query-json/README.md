# arqlet-query-json

The JSON request and answer of the Arqlet query operation (ADR 0021), shared by the engine (`arqlet-runtime`, feature `query`) and the query-only WASM module.

Part of [Arqlet](https://github.com/intentweave/arqlet), an incremental graph-layout
engine. The query operation is reached through [`arqlet-runtime`](https://crates.io/crates/arqlet-runtime) with its
cargo feature `query` (off by default).

**Provisional:** the query layer is experimental. This crate's API may change in any release.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
