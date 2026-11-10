# Arqlet

![Arqlet — Interactive graphs, continuously arranged. A hierarchy, a flow and a graph connected in one diagram.](docs/assets/brand/arqlet-cover.svg)

**Interactive graphs, continuously arranged.**

Combine hierarchies, flows, and graphs in a single diagram.

Arqlet is an incremental graph-layout engine in Rust. It lays out nested graphs
with ports, routes their edges and keeps the layout up to date as the graph
changes. The same engine runs natively and in the browser via WebAssembly.

> **Status: prototype (engine `0.2.0-prototype`).** Arqlet works end to end and is
> tested, but every API, the JSON protocol and the snapshot format are
> provisional and may change without notice. Nothing is published on crates.io or
> npm yet; the packages are prepared ([releasing](docs/releasing.md)). Large graphs (100K+ nodes) are a goal, not a measured result.

Arqlet was developed under the working name Layerweave. Measurement records in
`benchmarks/results` from before 2026-11-09 and some dataset names (such as
`layerweave-code`) keep that name.

## What works today

- **Graphs:** nested containers, ports, cycles, self-loops and parallel edges.
  Logical edge direction is preserved in every output.
- **Layout:** hierarchical (layered) layout with a single pass by default, with a
  direction per container; tree, radial and organic (stress) areas inside one
  diagram.
- **Routing:** orthogonal and polyline routes, port sides and order, and bundled
  edges.
- **Incremental edits:** you can add a node, collapse or expand a container,
  change a direction or repair routing locally. Unaffected inner layouts are
  reused, and an optional background refinement improves the result.
- **Stale results:** every result carries a revision and is rejected if the
  graph has moved on, never applied silently.
- **Deterministic output:** identical for any thread count.
- **Three deployment modes over one dispatcher:**
  - split: the backend precomputes and the browser continues;
  - browser-only;
  - backend-only.
- **Inputs:**
  - Arqlet snapshots;
  - [Intentweave](https://github.com/intentweave) graph exports;
  - graphs in the JSON form [ELK](https://eclipse.dev/elk/) uses (`children`,
    `ports`, `edges`, `layoutOptions`). Options without a counterpart are
    reported as diagnostics.
- **Output:** a renderer-independent scene, an SVG viewer and a React Flow
  adapter.

Not yet: full crossing minimization, guaranteed obstacle-free routes in every
case, a native Node.js adapter, and stable published packages. See
[docs/specification.md](docs/specification.md) section 8 for the detailed status.

## Quick start: browser demo

Requires Rust with the `wasm32-unknown-unknown` target, Node 24+ and Python 3.

```sh
rustup target add wasm32-unknown-unknown
sh apps/playground/build.sh examples/fixtures/small.json
python3 apps/playground/server.py      # http://127.0.0.1:8765
```

`build.sh` builds the WASM engine and runs the demo's tests. Pass any other
`graph.json`, for example an Intentweave export. See the
[playground README](apps/playground/README.md).

## Using the engine

Every operation is a JSON request to one dispatcher, the same in Rust, in WASM and
behind the loopback backend. The packages are prepared for crates.io (`arqlet`) and
npm (`arqlet`, `@arqlet/*`) but not published yet
([releasing](docs/releasing.md)); until then, depend on the Git repository:

```toml
[dependencies]
arqlet = { git = "https://github.com/intentweave/arqlet" }
```

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

The `layout-command` binary does the same on stdin:

```sh
cargo run --release -p arqlet-runtime --bin layout-command < request.json
```

In JavaScript, the `arqlet` package (`packages/arqlet`) is the entry point. It ships the
WASM engine, which runs in browsers and in Node:

```js
import {loadEngine, createElkLayout} from 'arqlet';

const run = await loadEngine();
const laidOut = await createElkLayout(run).layout(graph);   // an elk.layout(graph)-style call
```

`createEngineWorker()` runs the engine in a worker; `renderSvg` and `toReactFlow` draw
or map a scene.

## Repository structure

| Path | Contents |
|---|---|
| `crates/model` | canonical graph, identities, layout options |
| `crates/view` | visible and aggregated projection |
| `crates/layout` | layering, ordering, placement, ports, constraints |
| `crates/routing` | edge routes and bundles |
| `crates/scene` | renderer-independent geometry and metrics |
| `crates/topology` | planarity embedding and straight-line grid drawings |
| `crates/runtime` | dispatcher, snapshots, patches, incremental scheduling; `snapshot` and `layout-command` binaries |
| `crates/elk` | adapter for graphs in ELK's JSON form |
| `crates/reference` | curated reference diagrams (canonical JSON, validation, SVG) |
| `crates/wasm` | browser ABI (`lw_alloc`, `lw_free`, `lw_run` with JSON payloads) |
| `crates/contracts` | generates the TypeScript contract types (tooling) |
| `crates/node` | placeholder for a native Node.js adapter |
| `crates/arqlet` | facade crate: re-exports the engine crates and `dispatch` |
| `packages/arqlet` | npm entry point: re-exports engine client and viewer |
| `packages/engine-client` | the WASM engine with loader, worker protocol, presets, ELK-style API |
| `packages/viewer` | SVG rendering, navigation, React Flow adapter |
| `packages/containment-tree` | interactive containment tree of a scene |
| `packages/contracts` | generated TypeScript types for snapshot, scene and config sets |
| `apps/playground` | demo UI, comparison view and loopback backend |
| `examples/react-flow` | React Flow example |
| `tools/intentweave-export` | standalone exporter for Intentweave graphs (Node 24+) |
| `benchmarks/` | datasets, measurement scripts, recorded results, external-engine evaluation |

The core crates are synchronous, pure and deterministic. They have no browser,
Node, file system or network dependencies; only `serde` and `serde_json`.

## Documentation

- [Specification and implementation status](docs/specification.md)
- [Architecture](docs/architecture.md) and [decisions (ADRs)](docs/adr/)
- [Roadmap](docs/roadmap.md)
- [Configuration](docs/configuration.md) and [config sets](docs/config-sets.md)
- [Scene and viewers](docs/scene-viewers.md)
- [Comparison with other engines](docs/comparison.md) (dagre, elkjs, elk-rs)
- [Benchmarks](benchmarks/README.md)
- [Releasing to crates.io and npm](docs/releasing.md)

Performance figures are only stated with a results file in `benchmarks/results/`
that records machine, build and revision.

## Development

```sh
cargo fmt --all -- --check
cargo check --workspace --locked
cargo test --workspace --locked
sh apps/playground/build.sh examples/fixtures/small.json   # WASM, demo and browser tests
```

For faster local test runs use `cargo test --workspace --profile fast`. The
Intentweave exporter has its own tests: `cd tools/intentweave-export && npm test`.

## Contributing

Contributions are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md): every commit needs a
Developer Certificate of Origin sign-off (`git commit -s`).

## Third-party code

No implementation code from other layout engines has been copied. External
engines (elkjs, elk-rs, dagre) are used only for evaluation in `benchmarks/eval`
and keep their own licenses. ELK and Eclipse are trademarks of the Eclipse
Foundation; Arqlet is not affiliated with it.

## License

Copyright 2026 Benjamin Becker.

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
