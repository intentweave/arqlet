# arqlet-query

The neutral part of the Arqlet query layer (ADR 0021): values, the store seam (`GraphStore`), the plan IR and the executor with its limits. No third-party dependency.

Part of [Arqlet](https://github.com/intentweave/arqlet), an incremental graph-layout
engine. The query operation is reached through [`arqlet-runtime`](https://crates.io/crates/arqlet-runtime) with its
cargo feature `query` (off by default).

**Provisional:** the query layer is experimental. This crate's API may change in any release.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
