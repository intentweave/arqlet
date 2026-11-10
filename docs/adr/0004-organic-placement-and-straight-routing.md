# ADR 0004: Organic placement as a layout strategy, and straight routing as an edge option

Status: accepted for the first slice (2026-10-24). Not a portfolio decision: the default output is unchanged, and the comparison is in `benchmarks/results/organic-20261024.md`.

Decision:

1. **`organic`** is a layout strategy (`LayoutStrategy::Organic`) for an area. The children are placed by stress majorization on their graph distances (unweighted, undirected; components drift apart), starting from a deterministic pivot placement, then separated by their real sizes. There is no randomness; the iteration count is a budget (`organicIterations`, default 300).
2. An organic area needs a **free direction**, like `radial`. A fixed or allowed direction is refused with the reason, never ignored: a stress placement is not a ranked layout. *Amended 2026-10-26: this refusal is removed. The strategy an area is given wins over its direction, which stays in the document and applies again when the strategy changes; a strategy that a container only inherits yields to a direction the container declares itself, so such a container stays layered inside an organic parent.*
3. The distance matrix is quadratic, so an organic area is **bounded to 300 children**; a larger area is refused before placement.
4. **`straight`** (historical API; regional configuration and legacy fallback are superseded by [ADR 0005](0005-routing-constraints.md)) is an edge option (`edgeRouting`, default orthogonal and omitted). It is a separate option, usable with any arrangement. Each edge is one segment from its source port to its target port on the real node borders. Straight routing never bends: an edge whose segment passes through a foreign node is a routing issue. Self-loops and parallel or antiparallel edges between the same two nodes are refused with their name; they need their own treatment.
5. Organic with orthogonal routing is allowed, as before; the orthogonal router runs on the organic placement.
6. `organic` is not in the automatic strategy search yet; it is chosen explicitly.

Consequences and open points:
- Straight edges can pass through nodes after a placement. The rule is to report them, never to bend them silently. Repairing the placement so that straight edges stay clear is open, and it decides whether straight routing is useful on real graphs.
- Parallel and antiparallel edges coincide on a straight line; most benchmark graphs have some. *Amended 2026-10-26: they are drawn. Each takes ports of its own, so the segments lie side by side; self-loops stay refused. *Amended 2026-10-27: a self-loop is no longer refused. It follows the loop rule: never drawn straight, drawn as a loop around its node in the other styles the area admits (else orthogonal), while the ordinary edges stay straight. A loop on a circle leaves and returns at two points of the border; the loops of one node are turned against each other and are never bundled.* Bundles are not designed.*
- Warm start from existing positions is accepted by the placement function but not yet wired to the layout.
- The organic strategy on dense DAGs is much worse than the hierarchical layout (`workflow-dag`); the comparison records this, it is not a portfolio claim.
