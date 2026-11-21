# ADR 0021: A lean Cypher query layer over the Arqlet graph: own executor, decypher as front end, Grafeo as oracle

Date: 2026-11-21.
Status: proposed. Nothing is implemented. The evidence below comes from throw-away spikes of 2026-11-21 (kept outside the
repository), not from code in this repository.

## Context

We want to ask the Arqlet graph questions in (open)Cypher: filtering and traversals first, semantic functions (vector
similarity, full text, patterns in payloads, structure similarity) later. The graph may later also live in SQLite or
PostgreSQL (entity-relationship tables with a JSONB payload and a GIN index). Whatever we choose has to run in a browser
worker (WASM) and in a backend, in all three deployment modes, and it has to follow the rules of the core: synchronous,
pure, deterministic, identical on native and WASM, few dependencies, logical edge direction kept, cycles, self-loops and
parallel edges from the first milestone.

Two spikes (2026-11-21, own corpus, not the openCypher TCK) shaped the decision.

**Parser candidates**, 36 valid and 4 invalid queries (core language, variable-length paths, `EXISTS { }`, writes,
`CALL … YIELD`, Cypher 25 path quantifiers and label expressions):

| Crate | Valid parsed | Invalid rejected | Notes |
|---|---|---|---|
| decypher 0.2.0 (`EUPL-1.2 OR MIT OR Apache-2.0`) | 36/36 | 4/4 | 10 crates in the dependency tree; scope-resolved HIR; `wasm32` builds |
| open-cypher 0.5.0 (Apache-2.0) | 36/36 | 4/4 | parser only; 17 crates; `wasm32` builds |
| cypher-rs-zk 1.1.0 (MIT) | 32/36 | 4/4 | pins `pest ~2.7.15`; no `EXISTS { }`, no Cypher 25 syntax |
| uni-cypher 5.3.0 | 34/36 | 4/4 | 217 crates; `wasm32` build fails (`getrandom`) |
| cyrs-syntax 0.1.0 | 30/36 | 4/4 | `EXISTS { }` deferred by the crate |

**Grafeo 0.6.2** (Apache-2.0, Rust, a full graph database) as an engine: with its Cypher feature it passed 17/17 queries
with hand-computed expectations on a graph with a cycle, a self-loop and parallel edges, also on a store of our own
(`GrafeoDB::with_read_store`, ten required methods of the `GraphStore` trait, output identical over ten processes). Against
that: it brought 63 crates and a 5.3 MB `wasm32` build with `cypher,gql,ai`; the store trait is documented as experimental
through 0.6.x; on a store of our own `CALL grafeo.search.*` is unavailable (it needs the concrete built-in store) and
`text_match`/`text_score` are refused by the planner (`StoreKind::External`) although the trait has the hooks; its
floating-point behaviour across platforms is not verified.

**What decypher does not keep** (found while reading its HIR for use as an executor input, decypher 0.2.0):

1. `shortestPath(…)` and `allShortestPaths(…)` are dropped already by the parser: the AST prints back as a plain path
   pattern. Neither AST nor HIR can tell them apart from an unrestricted path. open-cypher keeps them.
2. A quantified relationship (`-[:KNOWS]->{1,3}`) is in the AST, but the HIR lowers it as a single hop
   (`RelationshipLength::Single`; `lower.rs`: "For now, lower the inner element without the quantifier").
3. A quantified parenthesized path (`(a)((x)-[:K]->(y)){2}(b)`) is printed back as three comma-separated patterns.
4. A function name that is not in the catalog (`foo_bar(1)`) is accepted without an error.

Each of these turns a query into a different one without any diagnostic. That is the failure mode this layer must not
have.

The HIR itself is otherwise a good input: scope-resolved bindings, a flat pattern with direction, label expressions and
variable length, `Match → OptionalMatch → Aggregate → Filter` per part with `WITH` as the part boundary, `Sort`, `Skip`,
`Limit`, `Union`, subqueries as nested queries. One execution detail to keep in mind: `ORDER BY` may use a variable that
the projection drops (`RETURN n.name ORDER BY n.age`), so the sort must see the input bindings as well as the aliases.

## Decision

Build a lean Cypher layer of our own, in two crates, and keep Grafeo out of the product.

1. **Crates (provisional).**
   - `crates/query`: the neutral part, with no third-party dependency. It holds a small plan IR, the values, the store
     seam (item 3) and the executor. It is synchronous, pure and deterministic and follows the rules of the core crates.
   - `crates/query-cypher`: the front end. It depends on decypher, turns a query into the plan IR, and carries the guard of
     item 4. The dependency stays here so the executor crate never changes with it.
   Both are scaffold APIs and stay marked provisional. The new operation goes through `dispatch`, so it runs in all three
   deployment modes without new graph semantics.
2. **Own plan IR between the front end and the executor.** The executor does not read decypher's HIR. A parser or HIR
   change then costs one translation, not a rewrite, and a second front end (open-cypher) can replace decypher.
3. **A store seam of our own.** A small trait (names provisional): the node by id, the edges of a node by direction and
   type, node ids, labels, edge types, property keys, and property reads. Reads return values in a fixed order (by id), never
   by hash order. The Arqlet adapter implements it over the canonical graph without copying it. Grafeo and SQL adapters can
   be added later without a change to the executor. Batch reads and pushdown hooks (property equality and range, search)
   are added when a second store needs them, not before.
4. **A front-end guard: every construct either runs correctly or is rejected.**
   - Reject by a check on the source text before decypher sees it: `shortestPath`, `allShortestPaths`, quantified paths and
     quantified path patterns. (Item 1 of the Context cannot be detected after parsing.) Until a construct is supported the
     error names it.
   - Reject function names that are not in our own function table, instead of relying on the catalog of decypher.
   - A test per known silent drop asserts the rejection, so a decypher upgrade that changes the behaviour shows up.
   - If the guard grows beyond a short list, switch the front end to open-cypher (AST keeps `shortestPath`) rather than
     patching decypher.
5. **Subset.** The first release is read-only.

   | Supported first | Rejected with a clear error | Later |
   |---|---|---|
   | `MATCH`, `OPTIONAL MATCH`, `WHERE`, `RETURN`, `WITH`, `UNWIND`, `ORDER BY`, `SKIP`, `LIMIT`, `DISTINCT`, `UNION` | `CREATE`, `MERGE`, `SET`, `REMOVE`, `DELETE`, `FOREACH`, `LOAD CSV` | `shortestPath`, `allShortestPaths` |
   | directed, reversed, undirected relationships; type alternatives; self-loops; parallel edges; relationship uniqueness per pattern | `CALL { }` subqueries and procedures other than the ones we define | quantified paths and path patterns |
   | variable length `*m..n` | | `EXISTS { }`, `COUNT { }`, `COLLECT { }` subqueries |
   | aggregation with grouping; `count`, `sum`, `min`, `max`, `avg`, `collect` | | search and similarity (item 7) |
   | expressions: three-valued logic, `CASE`, list and map literals, list comprehension, `IN`, string operators, regex | | structure similarity and graph comparison |
   | parameters | | |

6. **Semantics we fix now.** Logical edge direction is kept in every result. An undirected relationship matches a
   self-loop once. Within one pattern a relationship is used once per path. Null follows three-valued logic. Where the
   query leaves the order open, results come in a fixed order (by id), so output never depends on scheduling or hashing.
   Integers are `i64`, floats `f64`; `sin`, `cos`, `atan2`, `powf`, `hypot` go through `PortableMath`; hashes and counters
   are `u64` (`usize` is 32-bit in WASM). `ORDER BY` sees input bindings and aliases.
7. **Search and similarity are not decided here.** The plan IR gets a place for a `Search` operator, but the first release
   contains none. A brute-force similarity function over property vectors comes first, because it needs no index. Embeddings
   are inputs (property values), not computed in the engine. No standard exists for semantic search in Cypher, GQL or
   SQL/PGQ (checked on 2026-11-21); the syntax is chosen later.
8. **Gates.** A change is accepted when (a) the cases of the spike, with their hand-computed expectations, pass; (b) a
   differential run against Grafeo 0.6.2 (pinned, evaluation tooling outside the core crates, never a product dependency)
   shows no divergence on the supported subset; (c) a documented stage with an openCypher TCK subset passes; (d) every
   construct outside the subset is rejected, not silently changed. No performance statement is made without a measurement
   in `benchmarks/results/` and its machine and revision line.

## Third-party code

- decypher 0.2.0, <https://github.com/sunsided/decypher>, licensed `EUPL-1.2 OR MIT OR Apache-2.0`. We would use it under
  `MIT OR Apache-2.0`. Before adoption: record the exact revision and the notices required. No code is copied; it is a
  dependency of `crates/query-cypher` only.
- open-cypher 0.5.0 (Apache-2.0): fallback front end, same condition.
- Grafeo 0.6.2 (Apache-2.0): evaluation only (item 8b). No code is copied; its version is pinned exactly.

## Alternatives considered

- **Adopt Grafeo as the engine.** Fastest to a working Cypher (17/17 in the spike, 26 graph algorithms in its procedure
  catalog, a store seam). Rejected for the first step: its dependency tree and WASM size against our rules, a store trait that may change in
  any 0.6.x release, the search and index features unavailable on a foreign store, floating-point determinism across
  platforms not shown. It stays possible as an adapter behind item 3.
- **Fork Grafeo with a patch** (let the planner ask the store for a text index). A small patch in size, but a fork of about
  400,000 lines of three core crates with fast releases is a maintenance commitment. Deferred until an upstream proposal
  has been tried and the search cannot live outside the engine.
- **Write our own parser.** No: decypher parses all the spike's valid queries and rejects the invalid ones.
- **Translate Cypher to SQL first** (cypher-to-sql, GraphQLite). They target SQL stores, not an in-memory Arqlet graph.
  A SQL backend is a later adapter, not the starting point.

## Consequences

- We own the executor, so the semantics of items 5 and 6 are ours to get right. They are the larger part of the work, and
  the corner cases the spike exposed (parallel edges, self-loops, relationship uniqueness at `*2`) are where counts go
  wrong first. The differential run and the TCK are the guard.
- One new third-party dependency, confined to the front-end crate. The core keeps `serde`, `serde_json` and `libm`.
- A front-end upgrade can change behaviour silently; item 4 turns that into failing tests.
- The store seam keeps three paths open: a Grafeo adapter, a SQL or virtual-graph store, and a different front end.
- Semantic search and structure similarity are not covered by the first release, and no engine we found provides them
  across arbitrary stores; this ADR only reserves the place.

## Not decided here

- Syntax and scoring of search operators; hybrid ranking; embedding source.
- Structure similarity (neighbourhood fingerprints, Weisfeiler-Lehman) and comparison of two graphs.
- A SQL backend (mapping of entity tables and JSONB payload, GIN pushdown) and any virtual-graph variant of it.
- Whether to propose the planner change to Grafeo upstream, and whether to fork.
- The minimum supported Rust version (decypher 0.2.0 declares 1.94).
- The exact crate and trait names.
