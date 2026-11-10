# arqlet-elk

Provisional adapter for graphs in the JSON form ELK uses (children, ports, edges with sources and targets, layoutOptions): translated into an Arqlet graph and options, and the layout written back. No ELK code or dependency.

Part of [Arqlet](https://github.com/intentweave/arqlet), an incremental graph-layout
engine. Most users want the [`arqlet`](https://crates.io/crates/arqlet) crate, which
re-exports this one together with the rest of the engine.

**Provisional:** Arqlet is a prototype. This crate's API may change in any release.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
