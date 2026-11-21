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

1. `shortestPath(…)` and `allShortestPaths(…)` are gone from the AST: it prints back as a plain path pattern. Neither AST
   nor HIR can tell them apart from an unrestricted path. open-cypher keeps them. Upstream issue #133.
2. A quantified relationship (`-[:KNOWS]->{1,3}`) is in the AST, but the HIR lowers it as a single hop
   (`RelationshipLength::Single`; `lower.rs`: "For now, lower the inner element without the quantifier"). Upstream issue
   #134.
3. A quantified parenthesized path (`(a)((x)-[:K]->(y)){2}(b)`) becomes three disconnected patterns (also #134).
4. `WHERE` inside a node or relationship pattern (`MATCH (n WHERE n.age > 3)`) is dropped: the AST prints back without it
   and the HIR has no predicate. open-cypher keeps it. Upstream issue #132.
5. A function name that is not in the catalog (`foo_bar(1)`) is accepted without an error.

Issues #132 to #134 were filed by decypher's maintainer on 2026-11-13 and were open on 2026-11-21. Each of these turns a
query into a different one without any diagnostic. That is the failure mode this layer must not have.

The information is not lost in the parser. decypher's lossless syntax tree (`decypher::parse_cst`, rowan) still holds all of
it: `shortestPath` as an `IDENT` token, `WHERE_CLAUSE` nodes inside the patterns and a `RELATIONSHIP_QUANTIFIER` node. The
loss happens when the AST is built and when the HIR is lowered, so a guard can find each case reliably by walking the tree,
and the defects can be fixed upstream without touching the parser.

**Choice of the parser** (decypher 0.2.0 against open-cypher 0.5.0, assessed 2026-11-21). decypher is a GitHub fork of
`a-poor/open-cypher` and a complete re-implementation (hand-written rowan parser instead of pest) by a different maintainer;
open-cypher was itself rewritten in 2026 (Logos and LALRPOP, syntax only). They no longer share a design.

| | decypher 0.2.0 | open-cypher 0.5.0 |
|---|---|---|
| Architecture | lossless CST, typed AST, scope-resolved HIR, semantic checks and function catalog, `ToCypher`, optional `serde` | lossless token stream, typed and spanned AST, diagnostics; syntax only |
| Size | 10 crates; `wasm32` library about 650 KB (size-optimized, no `wasm-opt`) | 17 crates; about 1.85 MB under the same settings |
| Deep nesting | rejected above 128 levels with an error | parsed to 10,000 levels, stack overflow at 100,000 (1 MiB stack, native) |
| Silent drops found | four kinds above, three of them tracked upstream | none found in the constructs we probed |
| Conformance evidence | TCK syntax gate in its tests | TCK projection of 4,131 queries, all 377 grammar productions traced, fuzzing, mutation testing |
| Activity, last three months | 42 commits; release 2026-11-13 | 32 commits; release 2026-10-16 |
| crates.io downloads (total / recent) | 12,005 / 4,172 | 7,365 / 4,065 |

Both have one maintainer. In a sample of 40 closed decypher issues, all were filed by the maintainer, with a median of 0
days to close. Of 40 pull requests, 36 were by the maintainer and one external pull request was merged: the evidence on
how outside contributions are handled is thin. All numbers are from public repository and registry pages on 2026-11-21
and from our spikes; none is a performance measurement.

The HIR itself is otherwise a good input: scope-resolved bindings, a flat pattern with direction, label expressions and
variable length, `Match → OptionalMatch → Aggregate → Filter` per part with `WITH` as the part boundary, `Sort`, `Skip`,
`Limit`, `Union`, subqueries as nested queries. One execution detail to keep in mind: `ORDER BY` may use a variable that
the projection drops (`RETURN n.name ORDER BY n.age`), so the sort must see the input bindings as well as the aliases.

## Decision

Build a lean Cypher layer of our own, in two crates plus a thin adapter crate (named in M0), and keep Grafeo out of the
product.

1. **Crates (provisional).**
   - `crates/query`: the neutral part, with no third-party dependency. It holds a small plan IR, the values, the store
     seam (item 3) and the executor. It is synchronous, pure and deterministic and follows the rules of the core crates.
   - `crates/query-cypher`: the front end. It depends on decypher, turns a query into the plan IR, and carries the guard of
     item 4. The dependency stays here so the executor crate never changes with it.
   Both are scaffold APIs and stay marked provisional. The new operation goes through `dispatch`, so it runs in all three
   deployment modes without new graph semantics.
2. **Own plan IR between the front end and the executor.** The executor does not read decypher's HIR; the front end lowers
   decypher's HIR (which already resolves scopes and bindings) into our IR. A parser or HIR change then costs one
   translation, not a rewrite, and another front end can replace decypher later. We keep exactly one front end in the
   build: decypher is the default, and there is no second adapter and no build option for one (the effort of keeping two
   current is not justified; see the switch criteria in item 9).
3. **A store seam of our own.** A small trait (names provisional): the node by id, the edges of a node by direction and
   type, node ids, labels, edge types, property keys, and property reads. Reads return values in a fixed order (by id), never
   by hash order. The Arqlet adapter implements it over the canonical graph without copying it. Every read names the
   revision it reads at (the idea of Grafeo's `ReadView`), so a result carries its revision and a stale one is rejected, as
   the project rules require. Grafeo and SQL adapters can be added later without a change to the executor. Batch reads and
   pushdown hooks (property equality and range, search) are added when a second store needs them, not before. How the
   adapter reads the graph in place is the open question of the last section.
4. **A front-end guard: every construct either runs correctly or is rejected.**
   - Walk decypher's syntax tree (CST) before lowering and reject, with an error that names the construct: `shortestPath`
     and `allShortestPaths` (the `IDENT` token in a path assignment), `WHERE_CLAUSE` nodes inside node and relationship
     patterns, and `RELATIONSHIP_QUANTIFIER` and quantified path-pattern nodes. The tree keeps all of them (Context), so
     the guard does not scan source text.
   - Reject function names that are not in our own function table, instead of relying on the catalog of decypher.
   - Limit the size and bracket depth of the query before parsing, and the depth of our own recursion in lowering and
     execution. decypher already rejects expressions nested above 128 levels, but the limits of our own code are ours, and
     a stack overflow in WASM aborts the worker.
   - A test per known silent drop asserts the rejection. A decypher upgrade that fixes a drop makes the test fail, and the
     construct can then move from "later" to "supported" with its own semantic tests.
   - The guard lists the constructs; it does not guess. A drop that the tree does not show cannot be guarded; finding one is
     a switch criterion (item 9).
5. **Subset.** The first release is read-only.

   | Supported first | Rejected with a clear error | Later |
   |---|---|---|
   | `MATCH`, `OPTIONAL MATCH`, `WHERE`, `RETURN`, `WITH`, `UNWIND`, `ORDER BY`, `SKIP`, `LIMIT`, `DISTINCT`, `UNION` | `CREATE`, `MERGE`, `SET`, `REMOVE`, `DELETE`, `FOREACH`, `LOAD CSV` | `shortestPath`, `allShortestPaths` |
   | directed, reversed, undirected relationships; type alternatives; self-loops; parallel edges; relationship uniqueness per pattern | `CALL { }` subqueries and procedures other than the ones we define | quantified paths and path patterns |
   | variable length `*m..n` | | `EXISTS { }`, `COUNT { }`, `COLLECT { }` subqueries |
   | aggregation with grouping; `count`, `sum`, `min`, `max`, `avg`, `collect` | | search and similarity (item 7) |
   | expressions: three-valued logic, `CASE`, list and map literals, list comprehension, `IN`, string operators, regex | | structure similarity and graph comparison |
   | parameters | | `WHERE` inside node and relationship patterns |

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
9. **When to change the front end.** decypher stays the default until one of these happens; then we choose between a fork
   of decypher (the license allows it) and an open-cypher front end (which needs our own name and scope resolution):
   1. A further silent drop turns up that the syntax tree does not show, so the guard of item 4 cannot catch it.
   2. When we need `shortestPath`, quantified paths or `WHERE` inside patterns (the "later" column of item 5), the
      matching upstream issue (#132 to #134) is still open, and a pull request we offered has had no answer for about six
      weeks. Before offering one, agree on the shape of the fix in the issue (the quantifier needs a representation in the
      HIR), instead of sending a large change.
   3. The maintainer is inactive for 90 days, or the license or release policy changes to our disadvantage.
   The first release does not need any of the three constructs, so none of the open issues blocks it. The criteria are a
   starting proposal and can be adjusted in review.

## Third-party code

- decypher 0.2.0, <https://github.com/sunsided/decypher>, licensed `EUPL-1.2 OR MIT OR Apache-2.0`. We would use it under
  `MIT OR Apache-2.0`. Before adoption: record the exact revision and the notices required. No code is copied; it is a
  dependency of `crates/query-cypher` only.
- open-cypher 0.5.0 (Apache-2.0), <https://github.com/a-poor/open-cypher>: not a dependency. It is the fallback front end
  of item 9; the same conditions apply if it is ever adopted.
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
- **open-cypher as the default front end.** Its AST keeps what decypher drops, and it has stronger conformance tooling.
  Not chosen: it parses syntax only, so name and scope resolution, aggregate detection and the function catalog would be
  our code; it has no nesting limit, and its WASM size is about three times decypher's. decypher's defects are known,
  tracked upstream and detectable in its syntax tree. This can be revisited under item 9.
- **Both parsers as build options.** Rejected: keeping two adapters, two test matrices and two sets of notices current is
  more effort than the safety it adds. The own plan IR keeps a later switch cheap.
- **Translate Cypher to SQL first** (cypher-to-sql, GraphQLite). They target SQL stores, not an in-memory Arqlet graph.
  A SQL backend is a later adapter, not the starting point.

## Consequences

- We own the executor, so the semantics of items 5 and 6 are ours to get right. They are the larger part of the work, and
  the corner cases the spike exposed (parallel edges, self-loops, relationship uniqueness at `*2`) are where counts go
  wrong first. The differential run and the TCK are the guard.
- One new third-party dependency, confined to the front-end crate. The core keeps `serde`, `serde_json` and `libm`.
- A front-end upgrade can change behaviour silently; item 4 turns that into failing tests. The guard is only as good as
  the syntax tree it walks, which is why item 9 lists an unguardable drop as a reason to change.
- decypher is a young project with one maintainer; so is open-cypher. The plan IR and the switch criteria are the
  protection, not a promise of upstream support.
- The store seam keeps three paths open: a Grafeo adapter, a SQL or virtual-graph store, and a different front end.
- Semantic search and structure similarity are not covered by the first release, and no engine we found provides them
  across arbitrary stores; this ADR only reserves the place.

## Plan

Each stage ends with a check that can fail. M0 is this document and the preparation around it; no code before M1.

| Stage | Content | Exit check |
|---|---|---|
| M0 | The decisions below; spike corpus and the 17 cases with their hand-computed expectations as fixtures | this ADR accepted |
| M1 | Crates created (`publish = false`), parse, syntax-tree guard, limits, lowering of `MATCH … WHERE … RETURN` into the plan IR | all 40 corpus queries are accepted or rejected as the subset table says; one test per known drop |
| M2 | Executor on a test store: nodes, labels, properties, expressions with three-valued logic, `ORDER BY`, `SKIP`, `LIMIT`, `DISTINCT` | the spike cases for nodes and expressions pass; fixed result order |
| M3 | Relationships: directions, types, self-loops, parallel edges, relationship uniqueness, several patterns | the spike cases for edges and counts pass |
| M4 | Variable length, `OPTIONAL MATCH`, `WITH`, aggregation | the remaining supported spike cases pass |
| M5 | Arqlet adapter, operation through `dispatch`, WASM feature, napi path, results in the platform parity run, size of the WASM engine before and after | gates (a) and (d) of item 8; identical digests on all platforms; the size difference recorded |
| M6 | Differential run against Grafeo 0.6.2 (pinned, tooling only), a TCK subset | gates (b) and (c) of item 8 |
| After | `shortestPath`, quantified paths, `WHERE` inside patterns, `EXISTS { }` subqueries, the search operator, a SQL adapter | each its own decision |

The shared-graph question (last section) must be decided before M5. A measurement of throughput, if we want to state one,
belongs in `benchmarks/` with machine, build and seed, as the rules require.

## M0: decisions fixed here

1. **Properties.** The canonical graph carries none: a node is `id`, `kind`, `label`, `parentId`, `shape`; an edge is
   `id`, `kind`, `source`, `target` (may be missing), `weight`. A query sees two sources:
   - **Virtual properties**, read-only, taken from the fields the model has today: `id`, `kind`, `label`, `parentId`
     (nodes), `weight` (edges). The `kind` of a node is its single label and the `kind` of an edge is its relationship
     type. Containment is not a relationship in the first release; `parentId` answers it as a property. Edges without a
     target are excluded before selection, as the project rule says.
   - **A payload table** supplied by the host: for a node or edge id, a map of properties with JSON values (null, boolean,
     integer as `i64`, float as `f64`, string, list, map; an integer stays an integer). It travels with the query or is held
     by the session under its own revision. It is never part of the canonical graph, layout does not read it, and the
     canonical graph's serialized form does not change. A payload key that equals a virtual property is an error at import,
     not a silent override; an id that names no node or edge is an error, not skipped.
2. **Rust version.** The workspace `rust-version` is raised from 1.88 to 1.94, decypher's floor. Stable is 1.100.0
   (2026-11-09); the CI uses unpinned stable. The edition stays 2021 here: decypher's edition 2024 does not force ours, and
   an edition change is its own decision. The raise itself is a separate change (manifest, documentation, CI check); this
   ADR does not change `Cargo.toml`.
3. **Crates.** `arqlet-query` (plan IR, values, store seam, executor), `arqlet-query-cypher` (front end, depends on
   decypher), `arqlet-query-model` (adapter to `arqlet-model`). The first two have no `arqlet-*` dependency, so they could
   leave the repository later without a rewrite. The facade `arqlet` takes the feature that enables them; it is not on by
   default. They start with `publish = false` and join the release when accepted.
4. **Third-party records.** Before M1: the exact decypher version, crates.io checksum and upstream revision are recorded;
   `NOTICE` and the license handling of `tools/npm-prepack.mjs` (today only the two license files) are extended for the
   transitive notices; `CLAUDE.md` gets the one exception to "the core uses only `serde`, `serde_json` and `libm`".
5. **Fixtures.** The 40-query corpus of the spike (accept or reject) and the 17 executable cases with expected values,
   computed by hand on a graph with a cycle, a self-loop and parallel edges, move into the repository as test data in M0.
   The spike code itself does not.

## The shared in-memory graph (research pass 1 and a measurement, not decided)

The query layer and the layout engine should read the same graph in place: no copy of the graph, no comparison of two
copies, no merge of two versions. This section records what exists and a proposal; the choice is made before M5.

**What Arqlet has today.** `model::Graph` is two vectors (`nodes`, `edges`) with string ids plus constraint maps; the
runtime session owns it as part of the snapshot. `view::project` builds a new `Graph` for the visible nodes and copies an
edge only when the view keeps it; callers that project one graph many times keep a `GraphIndex` (id to index, parent,
container) and call `project_indexed`. Edits go through patches that carry a graph revision and are rejected when stale;
browser and backend share semantics, not memory (`docs/architecture.md`). ADR 0019 step 4 already plans interned dense
`u32` indices and columns for large graphs.

**What exists outside** (crates.io, 2026-11-21; descriptions from their own pages and our survey, none built here):

| Candidate | State | Fit |
|---|---|---|
| petgraph 0.8.3 (MIT OR Apache-2.0) | last release 2025-09-30; `Graph`, `StableGraph`, `Csr`, `GraphMap`; generic weights; algorithms | no snapshots, diff or merge. As storage it would be a second representation next to `model::Graph`, with a copy at the boundary unless layout moves to it too. Its visit traits can, as far as we recall, be implemented on own types; unverified |
| imbl 7.0.2 (MPL-2.0+), im 15.1.0 (last release 2022-04-29), rpds 1.2.1 (MIT) | persistent collections with structural sharing; cloning copies nothing | cheap snapshots, but reads are slower than flat vectors (a source we found says so for array-based containers); a sorted map gives a deterministic order, hash-based ones depend on the hasher; MPL-2.0 needs a check against our `MIT OR Apache-2.0` |
| salsa 0.28.5, differential-dataflow 0.26.0 (MIT) | incremental computation (memoized queries by revision; collections maintained under insertions and deletions) | execution models, not graph stores; Arqlet has its own incremental scheduler, so adopting either is an architecture change |
| loro 1.18.0 (MIT), automerge 0.12.0 (MIT) | CRDT documents for merging concurrent edits | merge is their purpose, with their own document model; far more than we need until several writers exist. `grafeo-loro` on crates.io combines Loro with Grafeo as a precedent |
| Grafeo (row-group store, reads at an epoch, `GraphStore` trait), oxigraph 0.5.13, cozo 0.7.6 (last release 2023-12-11), indradb 5.0.0 (2025-08-16) | embedded stores that own their storage | layout would read through their API or from a copy; the epoch read is the useful idea |
| Arrow, GraphAr 0.3.0 (Apache-2.0, 788 downloads) | columnar formats | GraphAr is a storage format with a young Rust crate; Arrow IPC is already named in ADR 0019 for the backend-to-browser frame |
| rkyv 0.8.18, arc-swap 1.9.5 | zero-copy deserialization; swapping an `Arc` snapshot while readers keep the old one | building blocks, not graphs |

We found no library that offers a property graph, in-place sharing with a second engine, cheap diff and merge, a
`wasm32` build and deterministic iteration together.

**Proposal** (to be tested, then decided):

1. A read-only view trait that `Graph` with its `GraphIndex` implements today, reading in place as `project_indexed`
   already does, and that the columnar form of ADR 0019 implements later. The query layer borrows during one request;
   edits run between requests inside the session, so the synchronous core needs no locking.
2. The payload table (item 1 of M0) lives beside the graph, keyed by interned index, with its own revision, behind an
   `Arc` so a session and a request share it without a copy. Typed columns per property key replace a JSON value per node
   when a measurement shows the need.
3. Every read names a revision (item 3 of the Decision), so compare is a revision check: equal revision means equal data by
   construction; immutable parts can be checked with `Arc::ptr_eq`. There is no structural diff.
4. Merge is not built. Stale patches are rejected by revision, as today, until a use with several concurrent writers
   exists; a CRDT is then the candidate, and the persistent collections only if a measurement shows that snapshots (undo
   history, hand-over in split mode) are too expensive to copy.
5. No external graph library for now.

**Measured (2026-11-21)**, `benchmarks/results/copy-cost-20261121.md`: Apple M3, release build, a shared machine at load
about 3, synthetic graphs of 2,286 to 228,614 nodes (seeds in the file), median of 15 runs. Reading the graph in place
takes 0.01 to 0.8 ms. Against that, a full `Graph::clone()` takes 0.6 to 65 ms, `GraphIndex::of` 1.5 to 238 ms (2.4 to 3.7
times a clone, and growing faster than the graph, because it clones every id into a `BTreeMap`), `arqlet_model::validate`
2.2 to 411 ms, and a projection that keeps the whole graph 2.8 to 591 ms. A projection of a small view is cheap (0.25 to 50
ms); about 90 % of the public `project` call with the top level collapsed is validation and indexing. Against the layout
time of an import of a bounded view (`bench`, limit 100), validation, index and projection are about 5 % at 2k, 25 % at
20k and 37 % at 100k graph nodes (two separate measurements, so an order of magnitude, not a breakdown).

What this means for the proposal:

- The copy that matters is not the small projection but the index and the validation, and the import path already builds
  the index once and keeps it with the view structure (`Arc<GraphIndex>`, `crates/runtime/src/lib.rs`). The query adapter
  reuses that index; it must not build its own.
- A query that reads the whole graph must read it in place. Going through `project` costs 36 to 591 ms on the graphs of
  20k to 200k nodes, where the in-place walk costs 0.06 to 0.8 ms.
- Persistent collections are not needed for projections: the projection of a view is already cheap compared with the
  index. They stay a candidate for snapshots only (item 4 of the proposal).
- The index and the validation grow faster than the graph. Interned dense ids (ADR 0019, step 4) should cut both; this is a
  hypothesis that no measurement here tests.
- Still open: the size of a realistic payload table, which none of these graphs has.

## Not decided here

- Syntax and scoring of search operators; hybrid ranking; embedding source.
- Structure similarity (neighbourhood fingerprints, Weisfeiler-Lehman) and comparison of two graphs.
- A SQL backend (mapping of entity tables and JSONB payload, GIN pushdown) and any virtual-graph variant of it.
- Whether to propose the planner change to Grafeo upstream, and whether to fork.
- Whether and when to contribute fixes for decypher #132 to #134 (item 9 sets the conditions).
- The names of the traits and types (the crate names are fixed in M0).
- The shared in-memory graph and the payload representation beyond the first table (last section).
