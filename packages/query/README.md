# @arqlet/query

Read-only openCypher queries over an [Arqlet](https://github.com/intentweave/arqlet) graph, in the browser and in Node,
as a WebAssembly module **without the layout engine**. Provisional: the API, the answer format and the supported subset
of openCypher may change in any release.

```js
import {loadQueryEngine} from '@arqlet/query';

const engine = await loadQueryEngine();
engine.load(
  {
    nodes: [
      {id: 'pkg', kind: 'package', label: 'app', parentId: null},
      {id: 'main', kind: 'fn', label: 'main', parentId: 'pkg'},
      {id: 'parse', kind: 'fn', label: 'parse', parentId: 'pkg'},
    ],
    edges: [{id: 'e1', kind: 'calls', source: 'main', target: 'parse'}],
  },
  {payload: {nodes: {parse: {loc: 120}}}},
);

const answer = engine.query(
  'MATCH (a:fn)-[:calls]->(b:fn) WHERE b.loc > $min RETURN a.label, b, b.loc',
  {params: {min: 100}},
);
// answer.rows: [['main', {node: 'parse'}, 120]]
```

Errors are answers, not exceptions: `{ok: false, error}`. A query error reads `query <class>: message`, with the class
`syntax`, `unsupported`, `runtime` or `limit`.

## The graph a query sees

- The input is an Arqlet graph (`nodes` with `id`, `kind`, `label`, `parentId`; `edges` with `id`, `kind`, `source`,
  `target`, `weight`), the same as the layout engine takes. Other formats have to be converted by the host.
- A node's label is its `kind`, a relationship's type is the edge's `kind`.
- Properties of the graph itself: `id`, `kind`, `label`, `parentId` on a node; `id`, `kind`, `weight` on an edge. Further
  properties come from the optional `payload`, by node and edge id. A payload may not use those names, nor a map as a
  value.
- Containment is not a relationship: `parentId` answers "directly in".
- An edge without a target, or with an end that is no node, is not seen; `load` counts them (`unresolvedEdges`).

## What is supported

- Node and relationship patterns: direction, type alternatives, properties, several patterns, variable-length paths.
- `MATCH`, `OPTIONAL MATCH`, `WHERE`, `WITH`, `UNWIND`, `RETURN [DISTINCT]`, `ORDER BY`, `SKIP`, `LIMIT`, parameters,
  `CASE`, `IN`, `IS NULL`, `STARTS WITH`, `ENDS WITH`, `CONTAINS`.
- The aggregates `count`, `sum`, `min`, `max`, `avg`, `collect`.

Not supported: writing, `UNION`, **any function besides those aggregates** (no `id()`, `type()`, `size()`, `toLower()`;
use `n.id`, `r.kind`), the power operator, path variables, `RETURN *`, label expressions, `shortestPath`, quantified
paths, patterns inside `WHERE` and `EXISTS { }`. A query that uses one of these is answered with an error, never with a
partial result.

## Limits and cost

- A query stops at `maxSteps` (default 20 000 000 steps of work), `maxRows` (2 000 000 rows held) or `maxHops` (64, for
  an unbounded `*`). The defaults are provisional and not yet measured against time.
- A query text is at most 65 536 bytes, and the parser refuses deep nesting (brackets, `NOT`, long operator chains).
- Calls are synchronous. Run the engine in a worker where a query may take long.
- Each engine is its own module instance with its own graph. `load` replaces the whole graph; there are no incremental
  changes. The module's memory grows and never shrinks: drop the engine to give it back.
- A host that also runs the layout engine holds the graph twice.
- Requests and answers are JSON: integers beyond 2^53 lose precision in JavaScript, and a whole number (`2.0`) arrives as
  an integer.
- After a call that stops the module (out of memory, stack exhausted), the engine answers errors only; create a new one.
- Load time, query time and memory are not measured. The module's size is recorded in
  [`benchmarks/results`](https://github.com/intentweave/arqlet/tree/main/benchmarks/results).

## Licenses

`MIT OR Apache-2.0`, at your option. The module contains third-party Rust crates, among them the parser
[decypher](https://github.com/sunsided/decypher); their notices are in `THIRD_PARTY_NOTICES.md`.
