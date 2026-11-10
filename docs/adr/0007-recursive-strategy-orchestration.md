# ADR 0007: Recursive strategy orchestration and regional composition

Date: 2026-10-24.
Status: accepted architectural direction; implementation is partial.

## Context

[Recursive optimization](../recursive-optimization.md) defines the strategy catalogue and top-down workflow. Several capabilities now exist, including structured composition, strategy constraints and alternative candidate refinement. They do not remove the need for one explicit architectural decision connecting regional search to the [reactive session in ADR 0006](0006-reactive-layout-session.md).

## Decision

Use one renderer-independent Rust coordinator to plan and refine regions recursively. A region can use a different eligible layout strategy from its parent or siblings. Strategies and repair operators share complete-candidate validation and an interchangeable evaluator. The current quality decision remains governed by [ADR 0002](0002-quality-model.md); this ADR introduces no new metric weights or acceptance order.

Semantic containment, optimization partition and membership groups are different concepts. Containment provides one spatial owner and local coordinate system. Analysis may create other optimization regions without changing semantic membership or duplicating positions. Potential overlapping groups do not automatically become independently solvable containers.

Resolve authored direction, strategy, routing, ports and other requirements by scope before dispatch. Explicit fixed options remain binding during construction, repair and continued computation. Free or allowed options permit only declared choices; selected results are not serialized back as authored constraints.

Region interfaces include attachment identities/order, size and geometry requirements, coordinate transforms, and dependency versions. Begin with fixed interfaces; bounded alternatives and negotiation may follow. Composing individually valid or planar children does not prove that the parent drawing is valid or planar. Check boundary and exterior routes and evaluate the complete composed candidate.

Keep a bounded set of useful candidates. Allocate work across construction paths and refinements without enumerating the full product of child alternatives. Search can explore temporarily inferior states; only complete eligible improvements are published under the active evaluation context. Failed search does not prove infeasibility.

One resource scheduler limits all regional and strategy jobs. Parallelize alternatives or regions with compatible fixed interfaces; respect parent/child dependencies. Do not recursively create independent unbounded thread pools.

## Relationship to the reactive session

The recursive plan is the computation structure; the reactive session is its lifecycle and input/publication authority. They are not separate competing optimizers.

- Accepted input deltas invalidate affected artifacts, regions and interfaces in the existing plan.
- Unchanged children, topology analysis, solver checkpoints and local geometry can be reused when dependencies match.
- Start repair within the affected region; expand to dependent neighbors/ancestors as required.
- Force/Stress warmstart and exact resume belong to the regional strategy state, under the compatibility rules of ADR 0006.
- Publications share the session's current input identity. A renderer never starts a second authoritative regional plan.
- Browser/backend execution uses this same logical plan and authority; transport changes do not change semantics.

## Consequences and documentation ownership

The detailed procedure, capability catalogue, operator design and delivery stages remain in [recursive-optimization.md](../recursive-optimization.md). This ADR records the decision and its reasons instead of duplicating that document. [ADR 0006](0006-reactive-layout-session.md) owns delta/interaction and resume semantics. [ADR 0008](0008-engine-client-viewer-boundaries.md) owns engine/client/viewer responsibilities.

Current code must be assessed separately from these target decisions. The original specification's implementation inventory is a dated baseline. It must not be read as a current completion report.

Acceptance: demonstrate regional strategy selection with inherited requirements, complete composition, bounded work, safe cancellation/publication and dependency-aware reuse after an edit. Validate claimed local improvements in the composed context. More algorithm families or a newly calibrated score are not prerequisites for this architectural boundary.
