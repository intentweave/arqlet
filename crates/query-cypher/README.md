# arqlet-query-cypher

The openCypher front end of the Arqlet query layer (ADR 0021): the parser `decypher` behind a guard over its syntax tree and HIR that rejects every construct the executor does not run, and the lowering into the plan of `arqlet-query`.

Part of [Arqlet](https://github.com/intentweave/arqlet), an incremental graph-layout
engine. The query operation is reached through [`arqlet-runtime`](https://crates.io/crates/arqlet-runtime) with its
cargo feature `query` (off by default).

**Provisional:** the query layer is experimental. This crate's API may change in any release.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
