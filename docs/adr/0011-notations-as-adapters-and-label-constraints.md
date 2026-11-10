# ADR 0011: Diagram notations are adapters; the engine knows geometry and constraints

Date: 2026-10-25.
Status: accepted direction; the label constraints are implementation pending.

Related: [ADR 0008](0008-engine-client-viewer-boundaries.md) (engine, client and viewer boundaries), [ADR 0009](0009-unified-interactive-playground.md) (interactive playground), [ADR 0010](0010-containment-tree-and-container-sizing.md) (container sizes).

## Context

The playground shows an entity-relationship diagram as entity boxes with attribute rows and labelled relationship edges. This was done in the viewer (`packages/viewer/svg.js`) on top of a generator that marks entities with the node kind `entity`, and an edge kind that carries the relationship type. Two problems came up:

- A relationship drawn as a diamond node read as the wrong thing: it is the relationship's type, which is data, not a layout element.
- The edge labels are drawn by the viewer after the engine has laid out the diagram. They can cover nodes. The engine does not know the labels exist, so no placement or routing step can avoid them.

The question was whether the engine should get an ER mode, or whether notations are a separate concern.

## Decision 1: notations are adapters, not engine modes

The engine does not switch its behaviour on the names of a notation (ER, UML, BPMN, org chart). A notation is an adapter in three parts:

| Layer | Task |
| --- | --- |
| Data adapter | Maps a notation's content to the graph: an entity is a node, its attributes are children or rows, a relationship is an edge with its type as the edge kind. |
| Presentation projection | Decides how content is drawn: an entity as a box with rows, an attribute as text, an edge kind as a label. |
| Engine | Places nodes, determines ports, routes edges, and keeps label areas free. It knows geometry and constraints, not class or primary key. |

The engine sees a node as a box with a size, an outline, and possibly ports. It needs no notion of a class.

## Decision 2: ports belong to content and are anchors, not sides

A port is more than a side of a box. An adapter may need:

- an anchor on a specific row (for example, the left edge of the `id` row of an entity);
- a fixed position or an allowed range relative to the node;
- allowed exit directions;
- an update when the node changes size or collapses.

The adapter maps a semantic identifier (an attribute) to a port. The engine respects the geometric rules of that port. Attribute rows are therefore not required to be laid-out child nodes. Where a row is hidden by collapsing, the adapter states the rule for the edges that attach to it, for example a substitute port on the collapsed node.

## Decision 3: label areas are an engine constraint, independent of any notation

An edge label is content the engine must place. The rules are:

- The consumer provides the label's text and measured size, and the placement requirements (for example, the preferred position on the route).
- The engine returns a valid position. A label may not overlap a node, unless the layout explicitly reports that it cannot avoid it.
- If the engine finds no free place, it must make room (move nodes, change the route) or report a conflict. It must not silently accept an overlap.
- The viewer draws no label that the engine has not placed. An unchecked label drawn over a node is a defect, not a rendering choice.

This rule applies to every notation and to every diagram that has labels.

## Decision 4: the viewer measures and displays; it does not decide where content goes

The viewer reports the measured size of its content back to the engine. It does not place labels or ports itself, and it does not reinterpret the engine's result.

## Decision 5: a node may have inner rows with their own geometry

A node can carry an ordered list of rows, each with a stable ID, a measured or given size, inner padding and a row gap. This is general inner node geometry. The engine knows only that a node has rows; it does not know what a row means (an attribute, a method, a port name).

- The engine computes the node's size from its rows and returns the rectangle of each row. The viewer draws the content inside those rectangles and reports size changes back.
- Rows are not graph nodes and not containment. They do not take part in organic, tree or the outer placement, so they cannot break the arrangement of the diagram.
- A port may bind to a row: a row ID, a side (left or right) and an alignment within the row. The engine computes the port's position from the row rectangle. Consumers do not compute offsets themselves, which would duplicate the geometry.
- Collapsing a node is a change of its rows. The adapter states the rule for edges that attach to a hidden row, for example a substitute port on the node's header. An edge does not disappear.
- The first step is a single column of rows with padding, a row gap, and left or right row ports. Several columns, tables, or arbitrary nested content are not part of it.

The current port model (node, side, relative offset between 0 and 1) cannot express a row, so the row binding is a new port variant: a port with a `row` field sits on its side, left or right, at the middle of that row.

**The box stays the default.** A port without a row is on the node's box, as before; edges are attached to the box unless a row is named. The row binding is an option, not a change of the model's default. A row port that names no row of its node, or a side other than left or right, is refused by name.

## Consequences

- The viewer's current edge-label drawing (`edgeKinds` in `svg.js`) is replaced by engine-placed labels. Until then it is a known gap, and the playground marks the ER notation as such.
- The generator keeps the notation's data (entities, attributes, relationship types). The projection decides how to show it.
- New notations (tables, component boxes, UML) need an adapter and a projection, not an engine mode.
- Attribute rows are a structure of the node, not a styling detail. Their geometry moves into the engine (Decision 5); the viewer keeps the styling inside the rectangles the engine returns.

## Not decided here

- The format of labels and ports in the graph JSON and in the engine request. Labels are presently engine options (`edgeLabels`); ports are the existing port model. Whether they move into the graph data is a separate decision.
- The collision search itself: which candidates to try, and how to route around a label. The label constraints step in the roadmap specifies it.
