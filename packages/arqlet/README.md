# arqlet

**Interactive graphs, continuously arranged.** Arqlet lays out nested graphs with
ports, cycles, self-loops and parallel edges, routes their edges and keeps the layout
up to date as the graph changes. This package is the single entry point for the
engine (WebAssembly, runs in browsers and Node) and its viewers.

```sh
npm install arqlet
```

```js
import {loadEngine, createElkLayout} from 'arqlet';

const run = await loadEngine();          // the engine shipped with @arqlet/engine-client
const elk = createElkLayout(run);        // an elk.layout(graph)-style call
const laidOut = await elk.layout({
  id: 'root',
  children: [{id: 'a', width: 40, height: 30}, {id: 'b', width: 40, height: 30}],
  edges: [{id: 'e1', sources: ['a'], targets: ['b']}, {id: 'e2', sources: ['b'], targets: ['a']}],
});
// laidOut.children have x/y; laidOut.edges have sections. elk.diagnostics lists ELK
// options without an Arqlet counterpart.
```

Every engine operation is a JSON request: `run({op: 'scene', snapshot})`, for
example. `renderSvg(svgElement, scene)` draws a scene; `toReactFlow(scene)` maps it to
React Flow nodes and edges. Types for snapshots and scenes come from
`@arqlet/contracts`.

Arqlet reads graphs in the JSON form [ELK](https://eclipse.dev/elk/) uses. It is not a
1:1 ELK, contains no ELK code and is not affiliated with the Eclipse Foundation.

> **Status: prototype.** The API, the JSON protocol and the snapshot format are
> provisional and may change in any release.

Part of [Arqlet](https://github.com/intentweave/arqlet), an incremental graph-layout
engine in Rust. See the repository for documentation and the current status.

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
