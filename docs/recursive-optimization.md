# Recursive, score-driven layout optimization

Date: 2026-10-22. Status: proposed architecture and implementation specification.
Implementation inventory: main at `40728d9`; capabilities below are requirements unless explicitly identified as existing.

This document defines the target orchestration of [architecture.md](architecture.md) and the context for the operators in [algorithm-improvements.md](algorithm-improvements.md). It complements [configuration.md](configuration.md), [config-sets.md](config-sets.md), and [curated-references.md](curated-references.md). Status, scope and sequencing are tracked in [specification.md](specification.md) §8, [roadmap.md](roadmap.md) and [project-status.md](project-status.md). It does not claim that the strategy catalogue is implemented.

Architectural decisions are recorded in [ADR 0007](adr/0007-recursive-strategy-orchestration.md). The [reactive session in ADR 0006](adr/0006-reactive-layout-session.md) drives this same regional plan through deltas, dependency invalidation and solver continuation. [ADR 0008](adr/0008-engine-client-viewer-boundaries.md) defines the independent engine/client/viewer boundary. This document remains the detailed procedure and strategy catalogue; its original implementation inventory is historical.

## 1. Purpose and governing decisions

Arqlet shall construct and progressively improve mixed, nested diagrams using multiple compatible strategy paths. A common evaluator compares complete candidates; a coordinator allocates bounded work across regions, strategies and refinements. The same Rust core supports browser WASM and native execution without requiring a renderer.

The problem contains discrete decisions (decomposition, embedding, rank/order, port assignment, route corridors) and continuous or grid-constrained decisions (positions, dimensions, spacing, label placement). A strategy may optimize a subset exactly without optimizing the complete diagram globally. No global optimality or unrestricted 100K-node interactive performance is promised.

Required separation:

1. Requirements: semantics, constraints, preferred topology and inheritance.
2. Quality model: metrics, normalization, utility curves and weights.
3. Search policy: available strategies, budgets, parallelism, target score and stopping.
4. Presentation: colors, visibility and rendering choices.

A preset may reference all four but must not conflate them. A speed tier changes search effort, not silently the quality definition. A required LR direction differs from a preferred LR direction and from an LR initialization. A 10-unit minimum margin differs from a preferred margin. Fixed requirements must never be relaxed without an explicit change of request.

## 2. Terminology and identities

- **Canonical graph:** semantic identities and relationships, unchanged by layout decomposition.
- **View projection:** visible graph after collapse, filtering and aggregation; retains mappings to canonical identities.
- **Region:** optimization scope; not necessarily a visible container. Analysis regions may overlap, but a composition plan must define exclusive geometry ownership and boundary interfaces.
- **Boundary contract:** identities, allowed attachment geometry, ordering and size constraints shared by a region and its environment.
- **Strategy:** a structural construction path; **operator:** a bounded modification of an existing candidate.
- **Candidate:** partial search state or complete scene, explicitly distinguished.
- **Incumbent:** best publishable complete candidate for a fixed evaluation context.

Evaluation identity includes graph and view revision, measurement revision, resolved constraints, quality-model version and frozen normalization context. Execution identity additionally includes strategy versions, seeds, search policy and boundary-contract versions. Presentation-only changes need not invalidate geometry, but hiding required content cannot improve its quality score.

## 3. End-to-end procedure

| Step | Required procedures | Output / feedback |
| --- | --- | --- |
| P01 Resolve request | Validate graph, dimensions and references; resolve inherited constraints and local overrides; classify required vs preferred values | Immutable request and diagnostics |
| P02 Analyze | Components, SCCs, articulation points/blocks, tree/DAG and series-parallel recognition, degree/density, loops/multiedges, optional planarity, geometry demand | Versioned feature summary; expensive analysis is lazy |
| P03 Decompose | Semantic containers, independent components, suitable blocks, optional separators/weak coupling, coarse aggregation, local conflict scopes | Region/dependency plan; internal partitions do not create semantic containers |
| P04 Define interfaces | Boundary edge ends, side/order restrictions, size envelopes, coordinate transforms, routing/label reservations | Versioned contracts and bounded alternatives |
| P05 Select portfolio | Check capabilities and constraints; estimate cost and applicability; include previous geometry | Eligible strategies with reasons; distinguish unsupported from low expected value |
| P06 Construct coarse candidates | Region-specific structural strategies, limited order/orientation/embedding variants, deterministic seeds | Partial candidates with provenance; estimates are not publication scores |
| P07 Realize geometry | Actual shapes/sizes, container composition, ports, corridors, routes, labels and geometry validation | Complete comparable candidates or explicit failure |
| P08 Evaluate | Compute raw metrics once where possible; validate constraints; normalize and score | Evaluation vector, total score and measured work |
| P09 Allocate work | Initial fair exploration, then progress/cost-aware allocation with some alternative-path budget | Jobs and bounded candidate archive |
| P10 Refine | Structural alternatives, joint port/track/order moves, local routing, space requests, placement and compaction | Complete improvements; feedback to earlier phases when needed |
| P11 Compose/publish | Check interfaces and cross-region geometry; global evaluation; revision gate; atomic publication | Monotone incumbent score within one evaluation identity |
| P12 Stop/resume | Target, budget, cancellation, stagnation or planned-work exhaustion; save reusable state | Best complete result plus termination and validity status |
| P13 Process edit | Classify change, invalidate dependencies, warm-start locally, expand scope if necessary | New evaluation context and incremental invocation of P02–P12 |

These are dependency stages, not a rigid one-pass pipeline. Routing can request placement changes; labels can request routing space; child size changes can invalidate parent geometry. Every feedback loop has a work limit and must retain its last complete publishable state.

## 4. Recursive decomposition and composition

### 4.1 Decomposition choices

Independent components may be optimized concurrently, then packed. Packing must still respect global pins and placement requirements. SCCs identify directed feedback; biconnected blocks identify a different structural decomposition. Neither automatically becomes a container. Block composition must account for shared articulation vertices and incident-edge order.

For large inputs, allow coarse-to-fine graph aggregation as internal search state. Preserve all original edges through mappings; a final score must cover the requested projection, not a conveniently simplified graph. Limit region size and interface size based on measured work; node count alone is not a sufficient complexity estimate.

### 4.2 Boundary contract

Each region exposes:

- Stable attachment IDs and original edge/end identities, including parallel and antiparallel edges.
- Permitted sides, exact/relative anchors, angular ranges or boundary intervals, capacities and order groups.
- Required versus preferred rotation/order, and explicit allowed sharing/bus membership.
- Minimum/maximum or candidate dimensions; fixed dimensions when required.
- Reserved entry/exit corridors, label envelopes and margins.
- Local-to-parent transform and permitted transformations; rotations must transform all port and direction constraints consistently.
- Dependency and contract revision; ownership of each boundary route section.

Start with fixed contracts. Later, children return a bounded set of size/attachment alternatives. Parents select compatible combinations and request revisions. Avoid the Cartesian product of all child alternatives using bounded candidate sets, deduplication and beam selection. A failed bounded search is not proof of infeasibility.

Two planar child drawings do not imply a planar composition. Boundary order, shared resources and exterior routes must be checked. Keep required interface constraints; negotiate only declared degrees of freedom. Track repeated interface states to stop oscillation.

### 4.3 Parallelism

Parallelize independent components, alternative candidates over immutable inputs, or siblings whose interfaces are fixed. Dependent parent/child jobs wait for contract versions. One scheduler controls the total worker and memory budget; recursion must not create unbounded thread pools. Browser workers and native threads are execution backends, not algorithm requirements. WASM does not imply native parallelism.

Compose changes transactionally. Stale or incompatible regional results are discarded; disjoint updates are merged only after dependency/interface checks and complete evaluation. Initially prefer safe rejection over optimistic merging.

Determinism constraint (current invariant): candidates for one job see only the immutable input snapshot and are reduced in fixed configured order, so output is independent of thread count (`refinement_is_identical_for_any_thread_count`). An archive or adaptive allocation that feeds on earlier results must stay deterministic: allocation decisions are made only at batch boundaries, in fixed order, from work counters (never wall time or completion order). Any new scheduling needs this argument stated before it lands.

## 5. Layout strategy catalogue

These are target capabilities, not a requirement to implement every family in the first release. Each implementation declares its exact supported graph class, constraints, shapes, routing styles and output stages.

| Family | Construction / variants | Applicability and obligations |
| --- | --- | --- |
| Hierarchical / layered | Cycle treatment, rank assignment, barycenter/median/sifting order, coordinates | Logical arrows retained; return edges explicit; hard rank constraints respected; compact long-edge representation |
| Planar / topology–shape–metrics | Embedding, outer-face/block alternatives, orthogonal shape, metric realization | Abstract planarity is not box/port feasibility; preserve embedding through routing; high-degree boxes need a suitable model |
| Planarization | Planar subgraph, bounded edge insertion/reinsertion, then shape/metrics | Keep semantic edge IDs; temporary crossing vertices are not junctions; no minimal-crossing guarantee |
| Upward planarization | Directed planarization and constrained geometric realization | Check applicability to directed structure; handle cycles explicitly; do not silently assert all edges can be monotone |
| Organic | Force-directed, stress-based; later multilevel coarsening/refinement and force approximation | Internal energy only guides search; common score selects output; enforce pins/containment and repair overlaps; no quadratic all-pairs default for large regions |
| Tree | Root selection, child order, tidy/compact subtree placement, oriented/orthogonal variants | Require a tree/forest or explicitly retain non-tree edges as additional routes; preserve chosen roots and child order constraints |
| Radial tree | Root, subtree angular allocation, ring/radius spacing | Distinct from general radial layout; account for shapes and label tangential/radial demand; pin/port restrictions may rule out a variant |
| General radial, later | Center selection, shell assignment, angular order | Non-tree edges remain; define what radius represents rather than treating every graph as a tree |
| Orthogonal layout | Joint box arrangement, port rotation, orthogonal shape, compaction; topology-based or other declared construction | Distinct from routing orthogonally through fixed boxes; may reuse planarization or hierarchical building blocks |
| Circular | Circle/block assignment, cyclic order, circle sizing and inter-circle placement | Optional strategy; optimize chord order and label space; circular placement alone makes no planarity claim |
| Series-parallel | Recognize the declared class, choose/validate terminals when needed, decomposition tree, compose series and parallel components | Distinguish two-terminal series-parallel from other definitions; decomposition orientation and port order matter; extra edges require an explicit fallback, never deletion |
| Existing-layout repair | Retain valid geometry and structure, targeted local updates | First candidate for edits; movement and route stability enter the quality model |
| Specialized mixed regions, later | Swimlanes, sequence/timing constraints, comment/annotation placement | Regional capabilities and shared interfaces; never force semantic time order into an unrelated layout family |

Reference cross-check: OGDF exposes energy-based/multilevel, tree and radial-tree implementations [S1–S3]; ELK exposes multiple independent families rather than one universal engine [S4]. Series-parallel constraints and integrated labeling are documented by yFiles [S5]. This catalogue is a Arqlet design, not a claim of feature parity or copied implementation.

## 6. Routing catalogue and compatibility

| Routing strategy | Procedures and constraints |
| --- | --- |
| Straight | Clip to actual shape boundary, resolve endpoint directions, test all obstacles; diagnose collisions or use an explicitly allowed alternative |
| Orthogonal | Corridor graph/grid, A*, track assignment, bend/occupancy costs, port stubs; retain embedding constraints when supplied |
| Rounded orthogonal | Orthogonal path plus radius selection with clearance validation; rounded corners are not arbitrary curved routing |
| Polyline / octilinear | Visibility or directional path search; style-specific crossing, clearance and length metrics |
| Curved | Generate a safe corridor and fit spline/Bezier segments, or use a dedicated curve solver; validate curve envelopes and endpoint tangents |
| Bus-style | Explicit compatible edge groups; backbone candidates, branches, junctions, port assignment and shared-resource capacity |
| Local repair | Retain unaffected routes; reroute affected/conflicting edges with bounded scope escalation |

Curves need a versioned path representation beyond an untyped point list. Specify line/arc/Bezier segments, bounds, endpoint tangents and flattening tolerance where numerical approximation is used. Rendering, picking and evaluation must agree on the same geometry. A safe control polygon or safe endpoints alone do not establish obstacle clearance. Unsupported geometry must not be silently measured as a straight chord.

Bus-style is a routing topology, not just thicker rendering. Keep bus ID, member edges, backbone sections, branch attachments and legitimate sharing intervals. Crossing a bus is not a connection unless explicitly modeled. Preserve individual edge selection and direction. Report both physical shared-backbone length and logical member-path length; the quality model states which it uses. Port grouping, parallel track assignment, edge bundling and semantic hyperedges are distinct concepts. Bus routing must not invent connectivity.

YFiles documents the distinction between port/edge grouping and explicit buses [S6–S7]. Graphviz separately exposes straight, polyline, orthogonal and curved/spline routes and documents limitations of particular combinations [S10]. Therefore capability checks must be per combination, not just per named layout.

## 7. Port-positioning strategies

All edges have geometric attachment points even when explicit port objects or port markers are disabled. Port visibility is presentation only.

1. **Fixed:** preserve explicit anchor or relative offset; define behavior when its owner changes size.
2. **Side-constrained:** select a position within allowed sides/intervals, reserving fixed ports first.
3. **Directional / symmetric:** distribute slots consistently with flow direction and peer order; inexpensive baseline.
4. **Geometric / angular:** use relative geometry and shape boundary intersections; useful for radial, organic and straight connections.
5. **Corridor-aware:** select sides and order using actual first/last routing corridors instead of peer centers alone.
6. **Embedding-aware:** preserve cyclic edge-end order from a structural candidate; compatible side/slot assignment follows.
7. **Capacity/cost assignment:** match compatible edge ends to finite candidate slots or intervals; use bounded matching/flow or heuristics as appropriate.
8. **Group/bus-aware:** explicit shared endpoints or ordered branch groups; separate semantic capacity from visual spacing.
9. **Incremental:** prefer previous admissible assignments and change only the affected groups.

Inputs include allowed sides/angles, candidate compatibility IDs, capacities, relative order, minimum spacing, corner reservations, label envelopes and fixed/soft status. A matching solver can optimize assignment costs but does not by itself optimize full routes. Side/order/slot changes trigger rerouting of every affected edge. Return a space request or constraint diagnostic when demand does not fit. Boundary attachments use the same stable identity model.

For nonrectangular shapes, provide boundary point/normal and intersection queries; box envelopes may be conservative obstacles but must be identified as such. A circle need not pretend to have four rectangular sides. UML compartments may constrain attachment subregions. YFiles provides a useful reference for fixed/free candidate positions, capacities and matching [S8]; Arqlet must not relax fixed constraints merely to satisfy grouping.

## 8. Labeling strategies

Labeling covers node, edge, port, container and annotation labels. Host-provided measured bounds, font/measurement revision, padding and optional rotation constraints are inputs. Do not infer browser font metrics inside the core.

| Strategy | Behavior |
| --- | --- |
| Fixed / preserve | Keep explicit placement; expose occupied area to other phases |
| Generic post-layout | Generate owner-relative candidates and select low-conflict positions without changing boxes/routes |
| Integrated | Reserve space during placement/routing and request geometric changes when needed |
| Alternating joint refinement | Reposition labels, repair routes or spacing, then reevaluate the combined candidate |
| Incremental stable | Reconsider only invalidated labels, preferring previous positions |
| LOD / priority | Hide or abbreviate only by explicit view policy, with mappings and diagnostics; never silently omit required labels to improve score |

Candidate positions include node interior/exterior, source/middle/target edge locations, either side of a route, port-relative offsets and permitted curve tangents. Include readability, owner distance, collisions, ambiguity and motion in evaluation. Multiple labels on one owner remain distinct.

Start with deterministic greedy placement plus local exchanges; retain a candidate-conflict representation so bounded matching/constraint search can later replace the solver. If no collision-free placement exists, retain required labels with diagnostics unless constraints prohibit publication. Integrated labeling is not a guarantee under contradictory fixed constraints. Generic versus integrated labeling is established in yFiles [S9].

## 9. Common evaluation and acceptance

The default target is a versioned weighted scalar score, larger is better. Concrete weights and curves remain experimental. One possible utility family is:

`Q = 100 * sum(w_i * u_i(p_i)) / sum(w_i)`, with `u_i(p) = 1 / (1 + p/s_i)`.

Require finite nonnegative weights, positive scales, and a positive total weight. Record each raw metric, utility and weighted contribution. The formula is a hypothesis, not a calibrated default.

Two tiers, not one sum. Tier 1 is a hard constraint set: finite complete geometry and valid containment (`scene::validate_geometry`), plus any requirement-policy rule the request declares mandatory. Only candidates passing tier 1 are scored. Tier 2 is the score. Today's runtime makes overlaps (`sibling_overlaps`, `cross_branch_overlaps`), edge-node hits and container traversals absolute only by lexicographic priority in the key; `reference::hard_violations` is the only place they are mandatory. A weighted sum would silently make them tradeable. Therefore, whether each of those is tier 1 or a tier 2 term must be an explicit requirement decision recorded in an ADR, never a side effect of a weight.

Determinism of the score: the implemented key is integer-quantised (`round(x*100)`). A score must use fixed quantisation and a fixed reduction order. Float accumulation that can differ between native and WASM is not acceptable for acceptance decisions.

Metric groups: node overlaps (count and extent), edge/node penetration, unapproved edge sharing, hugging/proximity, crossing events and pairs including repeated crossings, bends/curvature, length/detour, area/aspect ratio, labels, preferred structure, and movement/port/route changes. Required margins are constraints; preferred margins are scored. Structural integrity (finite complete geometry, identities, valid containment, preserved logical connectivity) is always mandatory. Whether overlap is also forbidden is resolved from requirements rather than inferred from its score weight.

Do not use raw inverse length without a fixed reference and zero-length handling. Normalize against a frozen request-specific context; do not change denominators when a new candidate arrives. Missing metrics make a candidate unevaluable unless the model explicitly defines their applicability. Do not renormalize weights per candidate to reward missing capabilities. Curved and orthogonal metrics must be comparable where the quality model compares them; use style-specific utilities where necessary.

Nonlinear child scores are not additive. Aggregate raw counts/lengths with unique ownership, include inter-region conflicts, then evaluate globally. Local scores are search guides. Initial implementation may rescore complete drawings; later incremental metric updates must match full recomputation.

One completed candidate may improve some penalties while worsening others if its score improves and all required constraints hold. Search may traverse worse states. Publication retains the best complete eligible score for the same evaluation identity. Deterministic tie-breaking, numeric tolerances and score precision are versioned. A changed graph starts a new comparison context; its initial repair may score below the old graph's result.

## 10. Search control, operators and acceleration

Maintain a bounded diverse archive, not just one state. Give eligible strategies a small initial budget, then allocate work using score, observed improvement and measured cost, retaining exploration capacity. Start with a simple deterministic policy; do not require ML or a complex adaptive optimizer. Cheap estimated scores may prioritize work but cannot publish partial geometry. Prune by proved bounds only when such bounds exist; otherwise label pruning heuristic.

Operator contract: preconditions, modified decisions, dependency/affected-set calculation, reusable state, repair sequence, estimate of work, cancellation checkpoints, and complete-candidate outcome. Operators include order swaps, region flips, embedding alternatives, node movement, joint port/track reassignment, rip-up/reroute, label moves, space requests and compaction.

A space request names region, axis or boundary, estimated extra width, competing edges, blockers and minimum required versus preferred clearance. The layout chooses a legal displacement, grid expansion or container growth; the router never moves nodes implicitly. Width estimates are not feasibility proofs. Repeated-crossing repair preserves edge identities and validates all affected geometry rather than blindly swapping route pieces.

Spatial acceleration: obstacle R-tree or equivalent; row/column interval indexes for tracks; affected-route lookup; sparse/local routing graphs; reusable immutable routing preparation. Index changes must be transactional or revision-bound. An index accelerates queries but does not reduce a dense Cartesian grid automatically. Enlarge search windows on failure; do not infer global infeasibility from a local window. Measure preprocessing, expansions, rerouting, validation, scoring and composition separately.

## 11. Execution, edit handling and persistence

Every job carries evaluation identity, region and boundary versions, strategy/operator version and seed. One work budget covers all descendants. Check cancellation inside expensive phases; identify remaining uninterruptible operations. Stop reasons distinguish target reached, work/time budget, cancellation, stagnation, planned-work exhaustion, unsupported capabilities and definitive constraint contradictions. Exhausted search is not proof of infeasibility or optimality.

Current wire format: `Publication` is built as a hand-written JSON literal (`crates/runtime/src/lib.rs`, `dispatch_streaming`) with `key` as `Vec<i64>`, `schema_version == 1` and `engine_version == "0.1.0-prototype"` hard-checked. Any new score or region field needs an explicit version bump and a contract update; there is no patch/delta type yet.

Publish atomic complete geometry with monotonically improving score and stable sequence IDs. Throttle presentation independently of search. Revalidate before applying backend results after local edits. Native and WASM execution share geometric semantics; parallel completion order is not a tie-breaker. Adaptive time-budget searches need not be reproducible; deterministic work-budget mode must document its scheduling and numeric guarantees.

Persist best snapshots and optionally region analysis, rank/order, embeddings, ports, corridor topology, label candidates and indexes. Caches are optional and schema/version keyed; incompatible caches are discarded. Exact search resume and warm-start from a snapshot are separate capabilities.

Classify edits: coordinate/size, label measurement, port constraint, edge insertion/removal, hierarchy/collapse, quality model, presentation. Invalidate only relevant dependencies. Pure presentation should not recompute geometry; new topology can invalidate planar/tree/SP recognition. Pins introduced by a drag constrain subsequent candidates. Start with the previous layout, expand repair scope and reopen strategy choices when needed.

## 12. Existing implementation and required changes

Inventory re-checked at `main` after PR #40 (2026-10-23). Rows marked *added* were not in the original inventory, which was taken at `40728d9`. This is not a new benchmark report.

| Existing part | Reuse | Required evolution |
| --- | --- | --- |
| model/view/scene separation | Graph identity, projection, geometry, collapse mapping | Separate requirements, evaluation and search contracts; shape/path extensions |
| `layout::place` | Recursive container sizing/placement and dirty reuse | Per-region strategy dispatch and boundary alternatives |
| Hierarchical ordering candidates | Barycenter/median and current geometry construction | Strategy adapter; retain intermediate rank/order decisions |
| `topology` crate | DMP analysis, triangulation, canonical ordering, Chrobak–Payne, independent checker | Block composition, regional contracts, constrained box realization; not yet full TSM |
| Flat candidate | Experimental topology-derived placement/compaction; stop checks | Expose as supported bounded strategy; embedding-aware routing and nested support. Caveats to resolve first: it sets `port_placement = Angular` on the returned snapshot (`layout/src/lib.rs:2624`), and it refuses any graph with parents or non-biconnected/nonplanar input (`:2593`, `:2612`) |
| `ports` / `labels` | Current placement and decoration | Explicit staged contracts, assignment alternatives, integrated label feedback |
| Channel router | A*, occupancy rounds, fixed routes and active subset | Spatial locality, shared preparations, corridor/track ordering, additional route geometry |
| `local_channel_candidate` (in `crates/layout/src/lib.rs`, not `crates/routing`) | Affected edges/ports, bounded release of fixed routes, full fallback; skipped for `Bundling::Complete` | General region transaction, constraints and space-request loop |
| Two evaluators today | `layout::published_key` / `conflict_key` (lexicographic `Vec<i64>`, authoritative for acceptance) and `Quality` / `better_by` (tests and reporting only; includes `movement` and `area`, which the key ignores) | M0 must unify or retire one of them; no third metric set |
| Scene metrics | Shared raw geometry metrics | Quality model and curve/bus/label/stability semantics |
| `published_key` and internal gates | Existing baseline and comparison tests | Replace via evaluator interface at every acceptance site, not only final publication |
| Runtime progressive jobs | Candidate identity, work counters, parallel execution, publications | Multi-stage dependent jobs, region scheduler, archive and global budgets |
| Runtime session policy | Stale-result protection and latest publication handling | Quality/contract identities and regional composition rules |
| Reference editor | `crates/reference` holds the curated document (canonical JSON, SHA-256, validation, SVG import/export, grid `route`, `reroute`). The interactive editor is TypeScript in `apps/playground/editor/` (`history.ts`, `gestures.ts`, `session.ts`), which calls `reference::reroute` | Feed the same constraints and incremental transaction model; editor undo/redo stays host-side |
| WASM/native adapters | Host boundary and snapshot transport | New versioned request/result protocol, no host logic in geometry core |
| Benchmarks/curation | Frozen inputs, metrics, fingerprints and references | Score breakdowns, anytime traces, regional choices and ablations |
| *Added* Required clearance (`requiredClearance`) | Hard near-node constraint: placement gaps of 2 x (clearance + track), router margins and legacy obstacles inflated by clearance + track; unmet reported in `routingIssues` | Region-aware space requests (M3); the cost is measured (`benchmarks/results/clearance-sweep-20261023.md`) |
| *Added* Shadow evaluator and candidate log | `scene::quality` tiers, `bench --candidate-log`, candidate comparison and calibration split | Evaluator unification and calibration (M0 closure) |
| *Added* Work counters and budget ordering | Legacy router counters (`legacyCandidates`, `legacyGridPops`), channel A* counters, learned first-stage order under a budget | A cost-aware policy waits for measured costs per view |
| *Added* Publication v2 | Streamed publications carry `publicationVersion` and `score` (version 1 has neither) | Score-driven selection, if decided, needs the ADR and new baselines |
| *Added* Default objective order | Routed crossings ranked before near-node pairs (ADR 0002) | None; decision accepted, new fingerprint baseline recorded |

### Requirement versus preference

P01 has to classify every value as required or preferred. Today only `requiredClearance` is a requirement, and the rules below apply to it and to any future requirement:

- A required value is hard. A candidate that violates it is not published, whatever its crossings or length. Quality never trades it away; an unmet requirement is reported, never silently relaxed.
- A preferred value is a score term. The fixed `HUG_MARGIN` (12) that measures near-node proximity is preferred: it is a weighted tier-2 term, and the search may trade it against crossings and length.
- Contradictory fixed requirements need a diagnostic that names them, not a silent relaxation. Today only the clearance case is diagnosed (`required clearance C not met`); pin conflicts, port overflow and label conflicts are reported, but not as requirement diagnostics.
- A preference never becomes binding by accident. Defaults stay unset, and a requirement is active only when its option is set (checked for `requiredClearance` by the unchanged default fingerprints).

Current refinement rejects more than 1,000 nodes or 2,000 edges; do not remove this guard as a substitute for scaling work. Organic, dedicated tree/radial-tree, circular, series-parallel, full TSM, curved and explicit bus construction are target additions, not established current capabilities. Existing bundling is not automatically a bus implementation.

Logical interfaces to introduce: `OptimizationRequest`, `QualityModel`, `AnalysisSummary`, `RegionPlan`, `BoundaryContract`, `StrategyCapabilities`, `CandidateState`, `Evaluation`, `OperatorOutcome`, `SpaceRequest`, and `SearchSession`. Names are provisional, not a frozen public Rust ABI. Initially implement modules/adapters within existing crates; new crates are justified by dependency boundaries, not by this list alone.

## 13. Delivery sequence and acceptance

| Milestone | Deliverable | Acceptance |
| --- | --- | --- |
| M0 Evaluation contract | Raw metrics plus versioned score in shadow mode; requirement/search separation | Finite scores, fixed normalization, zero-edge cases, explainable tradeoffs; old selection reproducible |
| M1 Portfolio coordinator | Existing hierarchical and flat adapters, complete candidates, bounded archive and staged refinement | Shared acceptance everywhere; deterministic ties; progressive best score; cancellation retains complete result |
| M2 Independent regions | Components and containers with fixed boundaries; bounded parallel jobs | Composition includes cross-boundary conflicts; worker limits hold; mixed child strategies work |
| M3 Negotiated interfaces | Child size/port alternatives and parent feedback; local transaction/space requests | No lost edges or port identities; bounded oscillation; grid/free corridor opening; stale contracts rejected |
| M4 Strategy expansion | Tree/radial-tree and organic first as candidate additions; circular/SP/TSM by demonstrated need | Capability/constraint fixtures per family; no implicit spanning-tree edge loss or SP misclassification |
| M5 Routing/annotation expansion | Port assignment, integrated labels, curved paths and buses delivered separately | Shape-aware clipping, curve safety, explicit sharing, accurate picking/metrics and required labels retained |
| M6 Scale and resume | Sparse/local search, multilevel regions, reusable analysis, host handoff | Measured quality/time/memory curves; incremental/full metric agreement; stale cache rejection |

Relation to the existing schemes. The M-series in this document is the authoritative sequence for orchestration. The A-series (A0–A6, A4b-1…5) and P-series (P0–P7) in [algorithm-improvements.md](algorithm-improvements.md) are operator and verification packages that feed these milestones:

| M | Overlaps with | Note |
| --- | --- | --- |
| M0 Evaluation contract | P0 (baseline and measurement) | Shadow mode first; no selection change |
| M1 Portfolio coordinator | P0, P1 (staged refinement) | Whole-view jobs only at this stage |
| M2 Independent regions | §9.5a per-container recursion in algorithm-improvements | Doc §4 supersedes the duplicate description there |
| M3 Negotiated interfaces | P6, A4b-3, A4b-5 | Interface feedback and space requests |
| M4–M6 | A4b-4 and later P-items, as demonstrated by need | Gated by capability fixtures and measurements |

If a conflict appears between this table and algorithm-improvements, this document wins for sequencing and algorithm-improvements wins for operator details.

The order expresses dependencies, not a mandate to finish all of one milestone before any independent work. Every new strategy must provide a capability declaration, complete-candidate adapter, fixtures and cost measurements before automatic portfolio selection enables it. Do not implement all catalogue entries at once.

Required fixtures include: empty/singleton/disconnected graphs; cycles, self-loops and parallel edges; trees with cross-links; radial labels and nonrectangular nodes; valid/invalid two-terminal SP inputs; planar/nonplanar and high-degree boxes; hard port conflicts; repeated crossings; narrow corridors; nested mixed strategies with incompatible boundary order; bus branches and unrelated crossings; spline obstacle collisions; contradictory labels/pins; drag during background computation; cancellation in each expensive stage.

Fixture status (re-checked 2026-10-23):

- Exists: cycles, self-loops and parallel edges (runtime fixture, `benchmarks/generate.py` synthetic and network-vlan); planar/nonplanar analysis (topology crate, corpus); repeated crossings and narrow corridors (layout and routing tests, scene fixtures); hard port overflow (`overflowing_port_slots_are_reported`); pins honoured and refused cases (`crates/runtime/tests/pins.rs`, `pin_search.rs`, layout tests).
- Partial: empty graph (quality only); high-degree boxes (`dense_graph`, out-degree 2 only); contradictory pins and labels (pins yes, label conflicts no); drag during background computation (stale-job test in `session.rs`, not a background-computation test); cancellation (refinement and placement search only).
- Missing: singleton graph; disconnected graph (several components); trees with cross-links; nonrectangular nodes; nested mixed strategies; valid and invalid two-terminal SP inputs; radial labels; bus branches; spline obstacle collisions.
- The first M2 slice adds the singleton, disconnected, forest and cross-link tree fixtures (planned in §12 and roadmap §5), not the rest.

Record per run: revision, input/view/measurement/constraint hashes, quality version and scales, seed, work/time budget, region plan, strategy/operator provenance, score contributions, best-score-over-time/work, first valid result latency, final scene fingerprint and peak memory where measurable. Compare equal budgets on a fixed corpus, include repeats for timings, and separate quality calibration from held-out graph-family evaluation. Curated references guide preferences; they need not be feasible optima. Never claim speedup from changed inputs or changed score semantics.

## 14. Relationship to previous specifications

A0–A6 remain useful operator and verification packages. This specification adds their orchestration and explicitly proposes migrating lexicographic acceptance to a weighted quality model. Until M0/M1 are accepted, the current publication policy remains the implemented baseline. Earlier mandatory validity rules remain in force; any migration from forbidden geometry to a scored imperfection requires an explicit documented requirement-policy change, not just a different weight.

The earlier single hierarchical pipeline becomes one strategy path. Recursive placement remains, but general regional negotiation is new. Presentation profiles remain separate from quality models. No source code is imported through this specification. Before reuse or porting of third-party code, record the exact revision, license, obligations and provenance; documentation examples do not confer implementation reuse rights.

## 15. Source cross-check (accessed 2026-10-22)

These sources inform capabilities and distinctions, not blanket performance or optimality claims. Some OGDF pages identify the documented release as 2023.09. Exact complexity depends on graph class, fixed versus variable embedding, geometry and constraints; this specification deliberately does not copy an unqualified complexity table.

- **S1:** [OGDF energy-based algorithms](https://ogdf.github.io/doc/ogdf/group__gd-energy.html): force, stress and multilevel families; supports keeping organic strategies separate from routing.
- **S2:** [OGDF TreeLayout](https://ogdf.github.io/doc/ogdf/classogdf_1_1_tree_layout.html): root, orientation and subtree spacing choices.
- **S3:** [OGDF RadialTreeLayout](https://ogdf.github.io/doc/ogdf/classogdf_1_1_radial_tree_layout.html): dedicated radial-tree construction.
- **S4:** [ELK algorithm catalogue](https://eclipse.dev/elk/reference/algorithms.html): layered, force, stress, tree, radial, packing and overlap-removal modules; availability is not universal constraint compatibility.
- **S5:** [yFiles series-parallel layout](https://docs.yworks.com/yfiles-html/dguide/seriesparallel_layout/): dedicated structural layout with port and labeling considerations.
- **S6:** [yFiles edge routing](https://docs.yworks.com/yfiles-html/dguide/automatic-layouts-main-chapter/polyline_router.html): routing styles, label obstacles and explicit bus backbones/branches.
- **S7:** [yFiles edge and port grouping](https://docs.yworks.com/yfiles-html/dguide/layout-features/layout-edge_grouping.html): distinct grouping semantics; Arqlet's fixed constraints take precedence over grouping.
- **S8:** [yFiles port placement](https://docs.yworks.com/yfiles-html/dguide/port_placement/): fixed/free candidates, capacities and compatibility matching.
- **S9:** [yFiles label placement](https://docs.yworks.com/yfiles-html/dguide/label_placement/): generic versus integrated labeling and candidate positions.
- **S10:** [Graphviz splines](https://www.graphviz.org/docs/attrs/splines/): geometry styles and documented port/label limitations of orthogonal routing.
- **S11:** [OGDF framework overview](https://www.ogdf.uni-osnabrueck.de/about/): layered, planarization and upward-planarization frameworks. These are alternatives with reusable stages, not universal guarantees.

## Routing contract clarification (2026-10-24)

The amended [ADR 0005](adr/0005-routing-constraints.md) governs regional routing resolution. Mixed styles are required target behavior. Root absence is distinct from explicit orthogonal and from auto; legacy options only supply an undeclared fallback. Whole-edge style ownership initially follows the common containing area, with container endpoints treated in their parent area. Bus topology is separate from geometry. A common outline interface must validate full segments and clearances; cell-center masks alone do not provide contour-aware routing. Existing grid search can remain while contour-aware candidates and later visibility-graph routing are introduced. These are implementation requirements, not claims that PR #72 already supports them.

The contour-routing scope includes the operation contract and routing reuse matrix in [ADR 0005 §5](adr/0005-routing-constraints.md#5-shared-geometry-and-routing-algorithms). Implement shared geometry first, including approximation accuracy and revision-aware spatial queries, before extending search representations. Rounded-corner and spline realization must preserve clearance; bus planning requires explicit membership independently of geometry selection. These capabilities are staged extensions, not a requirement to implement all routing styles in the first contour-aware release.
