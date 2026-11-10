# ADR 0015: The user's decisions are engine values; the direct view replaces the separate curation mode

Date: 2026-10-27.
Status: accepted and implemented; the legacy curation draft remains for what the direct view cannot do.

Implements part of [ADR 0009](0009-unified-interactive-playground.md) and the history of [ADR 0006](0006-reactive-layout-session.md).

## Context

Editing was split in two: a direct view (move, resize, pin, collapse) and an exclusive curation mode with its own browser geometry
and undo stack. The undo of the direct view covered only container sizes. Gestures could end in a committed half-drag
(`pointercancel`), a release outside the window could be lost, and an engine reply of an old gesture could be applied.

## Decision

1. **Gestures end exactly once** (`apps/playground/gesture.js`): listeners on the window from the press to the end; only a release commits;
   `pointercancel`, a lost capture and Escape abandon the gesture and restore the engine's state; a model change abandons it and its replies
   are ignored; no background refinement during a gesture.
2. **An `Intent` is the user's decisions as one value** (`arqlet_model::Intent`: pins, requested sizes, container sizes, shapes, collapsed
   containers, direction, strategy, routing, ports, label positions). The engine reads it from a snapshot (`intent`) and restores it in one
   recomputation (`restore`); a restore is checked like any edit, so a stale undo is refused and an unknown name is refused. The page keeps
   the stack (`history.js`, 50 entries, one per action, cleared when a node is added or a graph is loaded); the runtime stays stateless.
3. **The direct view does what the curation mode did that can run through the engine:** move (also a selection moved as a whole, `others` in
   the move step), resize, pin, collapse, shape, direction, strategy, routing, placing a port (`port` operation, `user:<edge>:<end>` ports),
   placing a label (`labelPositions`, kept while the place is free, else a routing issue), undo and redo, saving a reference from the view.
4. **Not available in the direct view, and listed in the page:** moving a route segment, doglegs (Alt-click), route locks (Shift-click),
   size pins. They need engine rules of their own. A reference that holds them (or locked routes, moved labels, pending or changed routes)
   opens as a legacy draft, which still edits them.
5. Every operation runs through `dispatch` and therefore in the split, the browser-only and the backend-only mode; the backend allowlist
   is checked against the dispatcher by a test.

## Consequences

- Browser checks with real input: `gesture-robustness`, `history`, `selection-move`, `port-drag`, `label-drag`, `settle`, `rounded-routes`.
  A check once found a real regression (port drag targets that covered the nodes of a large view).
- Undo restores measured label sizes too, because requested and measured sizes are not separated yet (`requested_sizes` is open).
- The fixed-size border clamp of a drag applies to the grabbed node only; the constants for clearance and minimum sizes are still
  duplicated in the legacy editor; the growth of a container after a drop is still decided by the page.
- Publications are a typed `PublicationEnvelope` (`publicationVersion` 4, `kind`); the page counts stale results it discards. The page's
  and the Rust publication policy run the same cases (`examples/fixtures/session/cases.json`).
