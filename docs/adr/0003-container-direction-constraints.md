# ADR 0003: Direction and layout strategy constraints for the root and every container

Status: accepted (2026-10-24). Amended the same day, twice:
- The root uses the same constraint types as a container, and the orientations are `lr`, `rl`, `tb` and `bt`.
- Direction and layout strategy are two independent constraints. `hybrid` is removed: a free direction chooses per area by compactness, which is what `hybrid` did. `shortest` is an optimization criterion and not a layout strategy; it stays readable in the legacy direction slot until it has its own parameter.

Context: `containerDirections` took one direction per container, and its only meaning was "use this direction". Two problems followed. First, the hierarchical composition read only a container's own entry and ignored ancestors, while the default layout inherited from ancestors, so the two paths disagreed. Second, the composition ignored the authored directions altogether until #63, which invalidated earlier comparisons. An author also cannot say "any direction" or "one of these". Authored choices are constraints: the search may optimise within them but must not override them.

Decision:

1. **Direction** (`defaultDirection`, `containerDirections`), the orientation of the rank axis:
   - `"lr"`, `"rl"`, `"tb"`, `"bt"`: fixed. `rl` and `bt` mirror `lr` and `tb`; edges keep their logical direction.
   - `"auto"`: free. The layout chooses per area. Without a search the choice is compactness (the criterion `hybridBy`), so a free direction is what an earlier `hybrid` graph meant.
   - `["lr", "tb"]`: allowed. The layout chooses among the listed orientations. A one-element list is fixed; an empty list, a duplicate, or an orientation outside the four is refused.
2. **Layout strategy** (`defaultStrategy`, `containerStrategies`), a rule about how an area is arranged:
   - `"none"`: the plain arrangement by direction (the default; the root's absent entry).
   - `"tree"`: the children form a tree or forest, placed along the area's direction (each parent centred over its children; the depths are the columns or rows). Refused, with the reason, when the links contain a cycle, two parents for one node, or a parallel or antiparallel pair. A self-loop is not part of the tree skeleton: the check and the arrangement ignore it, and the router draws it (amended 2026-10-27, after commit 98f91ae). No link is removed; routes come from the regular router.
   - `"radial"`: the children form a tree or forest drawn on rings. It needs a free direction: a fixed or allowed direction is refused, because a ranked layout cannot be replaced silently by rings.
   - `"auto"`: the best eligible strategy wins, by the shared key of the composition. The default layout takes `none`.
   - `["tree", "radial"]`: allowed. The search chooses among the listed strategies.
   - `"shortest"` is refused here. It is a criterion for choosing an orientation per area; it gets its own parameter class later.
3. Both axes inherit separately. A missing container entry on an axis takes the nearest ancestor's entry on that axis; an entry on a container overrides its ancestors on that axis. A missing root direction is `"lr"`; a missing root strategy is `"none"`.
4. The mode of an area follows from both axes (`Graph::area_mode`): `none` with a free direction chooses per area by compactness; `none` with a fixed or allowed direction uses that orientation; `tree` places along the area's direction (free: compactness is not used, `lr` is the default); `radial` is a mode of its own. Combinations that cannot hold are refused with the reason.
5. The composition's search tries, per area, the strategies its constraint allows (for `auto`: `none`, `shortest`, `tree`, `radial`; a candidate that the graph cannot take is skipped), then the orientations its direction allows where the strategy leaves the orientation open. One pass; a change is kept only when the whole composition improves its shared key.

Consequences:
- Earlier graphs keep their meaning. A direction slot holding `hybrid` reads as a free direction; `shortest` reads as a free direction with the `shortest` criterion. A document without any strategy field keeps the earlier rule: a container with its own direction and no strategy is plain. Writing a graph returns the legacy form where the graph has that form, so stored documents keep their identity; the two stored `state-tcp-moved` documents were resealed once (their `hybrid` direction is now written as `auto`, the same layout).
- The default layout, the composition, and the editor read the same constraints, so inheritance applies in every path.
- The contract: `defaultDirection`, `containerDirections`, `defaultStrategy` and `containerStrategies` accept the forms above. `Orientation` and `LayoutStrategy` are exported.
- The demo has two controls, direction and strategy, each with fixed or free or allowed choices, and `inherit` for containers.

Not decided here:
- The shortest-end-to-end criterion as a parameter class. It is named here, not designed.
- The geometric meaning of a direction for cyclic graphs. A direction describes the arrangement: `lr` places the ranks left to right; it does not require every edge to run monotonically. Back edges are allowed.
- Other properties the plan lists (placement constraints, port placement, admissible attachments). Numeric properties such as spacing are inherited values, not three-state choices.
- A drawing-level check that each candidate satisfies its constraints. The constraints hold by construction in both paths; the check is still to be added.
- Tree orientation beyond what the direction gives, and spanning-tree variants for graphs with extra links (those links would need explicit routing).
