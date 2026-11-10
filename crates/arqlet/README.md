# arqlet

**Interactive graphs, continuously arranged.**

Arqlet is an incremental graph-layout engine in Rust. It lays out nested graphs with
ports, cycles, self-loops and parallel edges, routes their edges and keeps the layout
up to date as the graph changes. The same engine runs natively and in the browser
via WebAssembly (npm package [`@arqlet/engine-client`](https://www.npmjs.com/package/@arqlet/engine-client)).

> **Status: prototype.** Every API, the JSON protocol and the snapshot format are
> provisional and may change in any release. Large graphs (100K+ nodes) are a goal,
> not a measured result.

## Example

Every operation is a JSON request to one dispatcher. This one lays out a graph in
the JSON form [ELK](https://eclipse.dev/elk/) uses:

```rust
let request = r#"{"op": "elkLayout", "graph": {
  "id": "root",
  "children": [{"id": "a", "width": 40, "height": 30}, {"id": "b", "width": 40, "height": 30}],
  "edges": [{"id": "e1", "sources": ["a"], "targets": ["b"]},
            {"id": "e2", "sources": ["b"], "targets": ["a"]}]
}}"#;
let response = arqlet::dispatch(request);
// {"ok": true, "graph": {...with x, y and edge sections...}, "diagnostics": [...]}
```

Arqlet is not affiliated with the Eclipse Foundation; it contains no ELK code.

See the [repository](https://github.com/intentweave/arqlet) for the documentation,
the browser demo and the current status.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
