# ADR 0010: Containment tree and container size policy

Date: 2026-10-24.
Status: accepted direction; implementation pending.

Related: [ADR 0003](0003-container-direction-constraints.md) (direction and strategy per container), [ADR 0005](0005-routing-constraints.md) (routing per container), [ADR 0006](0006-reactive-layout-session.md) (authored versus measured sizes, size policies), [ADR 0008](0008-engine-client-viewer-boundaries.md) (ownership and packages), [ADR 0009](0009-unified-interactive-playground.md) (interaction scope).

## Context

Containers already have authored direction, strategy and routing settings, but no authored size. Their extent is derived from their children, padding and header. The playground offers a "grow containers" checkbox that is off by default and grows a container only when a node is dragged past its border. Authored container settings are reachable only through a set of selects in the inspector, which show no hierarchy and no provenance.

ADR 0006 already names size policies (intrinsic/auto, minimum, maximum, fixed) and forbids reinterpreting a fixed size as a minimum without a diagnostic. This ADR chooses the policy and the configuration model, and adds the hierarchy view that makes containment visible and editable.

## Decision 1: size policy per container and axis

Every container has a width and a height policy. Each axis is one of three states:

| State | Stored as | Effect |
| --- | --- | --- |
| Content (`auto`) | nothing | The extent follows the children, padding and header |
| Minimum (default) | `min` | The extent is at least `min` and grows with the children |
| Fixed | `fixed` | The extent is exactly `fixed`; content that does not fit is a conflict |

`min` and `fixed` are exclusive on one axis. Content is the state with neither, so it needs no field. This keeps the stored model to two optional numbers per axis and gives the three choices of option D.

Rules:

- Values are positive finite numbers in layout units (ADR 0006), not pixels.
- Sizes are per container and do not inherit. A parent's size never sets a child's size. Direction, strategy and routing keep their own inheritance (ADR 0003, ADR 0005).
- The root view has no size policy in this version.
- Leaves are outside this ADR. Leaf resizing belongs to ADR 0009, section 4.
- A stored axis with neither field is equal to the absent axis. Serialization omits it, so existing documents and default results stay byte-identical (`LayoutOptions` rule in `CLAUDE.md`).

The default is `minimum`. Without a stored minimum, that is exactly today's content-derived size, so the default changes no result. Check against the stored fingerprints (`benchmarks/results/fingerprints-20261021T103351Z.jsonl`) before the change is accepted.

## Decision 2: behavior of each state

- **Minimum:** a child that exceeds the extent grows the container and its ancestors, as derived layout. The extent never falls below `min`. Shrinking below `min` requires an explicit command.
- **Fixed:** the engine checks that the children, padding and header fit. If they do not, it returns a scoped conflict `container-fixed-below-content` that names the required minimum. The container is marked invalid, as today. The engine never grows or clips the container silently.
- **Content:** the extent always equals the content. No stored size is needed.

Version 1 checks fixed sizes only. It does not lay the interior out again inside a fixed box. Today `containerRefine` derives the size from the interior layout, so a fixed box needs a layout change in `crates/layout`. That change is a later step and is not promised here.

## Decision 3: interaction with the canvas

- **Grow on demand (replaces the checkbox):** dragging a node past a container's border in minimum or content state stores the new extent as `min`. It is one undoable transaction (ADR 0009). In fixed state the drag is refused with the conflict reason.
- **Reset to content:** an explicit command removes `min` on one or both axes.
- **Set fixed:** an explicit command in the inspector and the tree. The value must be at least the required content, which the engine reports.
- **Switch state:** explicit only. Changing the state never happens as a side effect of another edit.

Consequence: the grow-on-demand behavior moves from opt-in to default. That changes today's default playground behavior and is the consequence of choosing option B as the default.

## Decision 4: containment tree

The containment tree is a view of the same IDs, not a second hierarchy. It is a framework-independent package (`packages/containment-tree`, [ADR 0008](0008-engine-client-viewer-boundaries.md)) that emits intents and renders the state it receives.

Rows show root, containers and leaves in the containment relation. Each row shows:

- label and stable ID;
- collapsed, pinned and invalid state;
- the size state of each axis, such as `min 240`, `fixed 300` or `auto`;
- for containers, the effective direction, strategy and routing, with provenance: set here, inherited from which ancestor, or default.

Behavior:

- **Selection sync:** selecting a row selects the node on the canvas. Selecting a node reveals its ancestors and scrolls its row into view.
- **Collapse and expand:** the tree controls the same engine operation as the canvas control (`toggle`). The state lives in the engine, not in the tree.
- **Keyboard:** the WAI-ARIA tree pattern: arrow keys, Home and End, Enter to select, expand and collapse, and each row exposes `aria-expanded` and `aria-selected`.
- **Settings:** the selected container's direction, strategy, routing and size policy are edited here. Each change goes through the same versioned configuration path as the canvas (ADR 0009, section 2). Unsupported choices are shown as unavailable with the reason.
- **Conflicts:** a row with a conflict shows the reason and selects the affected container.
- **Filter:** a text filter changes the displayed rows only. It never changes engine state.

Not in this version:

- Reparenting by dragging a row. It is a semantic change and needs its own command and ADR (ADR 0009, section 4).
- Batch editing of several containers. It is a later addition on the same intent path.
- Editing leaf sizes from the tree.

The tree never computes geometry. If the engine has no answer yet, the row shows the last accepted state and a pending marker.

## Configuration model

The configuration is one document per container, with one field per concern:

```
containers: {
  "<id>": {
    sizing: {
      width:  { min?: number } | { fixed: number },
      height: { min?: number } | { fixed: number }
    },
    direction?, strategy?, routing?   // existing fields, ADR 0003 and ADR 0005
  }
}
```

Read and write rules:

- An absent axis means content. A `min` or `fixed` field is written only when it differs from the absent state.
- Each field has a source: set here, inherited (direction, strategy, routing only), or default. The tree and inspector show the source.
- Validation happens in the engine. Invalid values are rejected with a diagnostic, and the previous accepted state remains in place.
- Changes travel as a scoped patch through the update protocol (ADR 0006, section 4, "Set size policy"), in every execution mode.

## Delivery

1. **Engine:** the sizing field, its serialization and validation, fixed-size conflicts. Tests: default fingerprints unchanged, minimum grows ancestors, fixed conflict is reported, invalid values are rejected.

   *Status: implemented in the engine, with these deviations from the text above:*
   - *The conflict is reported as the existing geometry issue `container_bounds` (the child does not fit its container). The named kind `container-fixed-below-content` with the required size needs a sizing-aware validator; it is a follow-up.*
   - *A set size makes the composition mandatory: the flat joint layout does not honor container sizes, so when the key comparison would pick it, the composition still wins. The decision reason is still recorded.*
   - *Sizes were first applied only by the structured refinement. Since step 3 the placement of the plain layout applies them too (`crates/layout`, `place`), so the background refinement that the playground runs after an edit shows them. A fixed size below its content is refused explicitly by the structured refinement ("No valid placement candidate; geometry constraints could not be satisfied"); reporting it as a scoped conflict with the authored size kept is an open item.*
   - *Patches merge per container: an entry replaces that container's size, `null` removes it, other containers keep theirs.*
   - *Default output is unchanged by construction: with no stored size, the extent function is the identity and the composition rule is not reached. The stored fingerprints (`fingerprints-20261021T103351Z.jsonl`) are not regenerated in this step; that needs an idle machine (see `CLAUDE.md`).*
2. **Tree, read-only:** rows, provenance, size badges, selection sync. Unit tests for the tree model and a browser check for selection sync and keyboard.

   *Status: implemented in `packages/containment-tree` (model and WAI-ARIA renderer), shown in the playground's sidebar. Checks: `packages/containment-tree/model.test.mjs` (7 tests) and `apps/playground/browser-checks/containment-tree.mjs` (rows, row to canvas, canvas to row, arrow keys). Deviations from the text above:*
   - *Direction, strategy and routing rows with provenance are not shown yet. Their effective values come from resolvers that live in `app.js`; moving them into a shared module belongs to step 3, where the settings are edited.*
   - *Rows follow the scene's order; no sorting or filter yet.*
   - *Selecting a node on the drawing needed a fix: a press without movement now selects the node in the drag handler. The captured pointer had sent the click to the drawing, so the node never received it. This was already the case before the sizing work.*
3. **Tree actions:** collapse and expand, settings through the configuration path, conflict display.

   *Status: implemented for collapse and expand and for container sizes; the rest is open.*
   - *Collapse and expand: Right opens a closed container and Left closes an open one, as in the WAI-ARIA tree pattern; the form has an Expand/Collapse button. Both send the engine's `toggle` operation for that container.*
   - *Sizes: the form sets each axis to content, at least, or fixed, and "Use content" removes the entry. The entry goes through `configure` as a per-container patch, and the fields show the accepted state. Values that are not positive numbers are refused in the form before anything is sent.*
   - *Conflicts: the form and the row name the conflict. The fixed-below-content case is explicit but not yet a scoped conflict (see step 1).*
   - *Direction, strategy and routing: the tree's form shows each value in force with its provenance, and sets or clears the container's own value through the engine's `direction`, `strategy` and `routing` operations. The resolution rules now live in `packages/containment-tree/settings.js` and are the inspector's rules, moved. Allowed sets are shown but edited in the inspector.*
   - *The engine refuses a value that a container cannot take, with its reason in the status line (for example, `tree` for children that do not form a tree). The form keeps showing the accepted value.*
4. **Canvas gestures:** grow on demand as minimum, reset to content, remove the checkbox. Browser check: drag past border, undo, reset.

   *Status: grow on drop and the fixed border are implemented; the checkbox is gone. A drop that makes a container larger stores the grown axes as `min` (one `configure`, the containers above the node only, and no growth on a cancelled move). A fixed axis stops the drag at its border, for every fixed container above the node, and the status says so. Pinned children now enlarge their container (`crates/layout`, `place`), so a dropped leaf stays inside its border after a reset to content. Browser check: `apps/playground/browser-checks/grow-on-drag.mjs`. A container with an authored size has a reset control on the canvas (`resetControl` in `packages/viewer/svg.js`); it removes both axes. Resetting one axis stays in the tree. Each axis has its own reset control (`↔` width, `↕` height). Ctrl/Cmd+Z restores the size change before the last one (the tree, the reset, a drop that grows a container; `sizingUndo` in `app.js`, 50 steps, cleared when another graph loads). A resize handle at the bottom-right corner sets the minimum of both axes on release; a fixed axis is not changed there. Browser checks: `grow-on-drag.mjs` and `resize-container.mjs`. Open: undo for other edits (pins, moves), and a resize that sets a fixed size.*

Each step is verified with `sh apps/playground/build.sh`, the browser checks and the fingerprint comparison where defaults could change.

## Status: named conflict, and the configuration tree

- *A fixed size below its content is named: `container-fixed-below-content`, with the size it needs. It is computed for the plain layout and for the compositions (`fixed_size_conflicts` in `crates/runtime`), and the authored size is kept. The layout search accepts it now: `is_ineligible_geometry` (`crates/model`) is the one rule for the search and the refinement. An authored conflict does not make a candidate ineligible, and neither does an overlap that involves a node inside the conflicted container, because the box pushes its children out. Every other issue still does (test `a_fixed_size_below_its_content_is_refined_and_kept_as_a_named_conflict`, and the model test `overlaps_inside_a_conflicted_fixed_container_are_authored_and_others_are_not`).*
- *Design direction for the configuration tree: it works like the settings editor of an IDE. Each container is a row; expanded, it lists its settings (direction, strategy, routing, width, height) as editable rows with their source ("from outer", "set here", "default"), a reset per setting, and a filter over the tree. Today the settings sit in a form below the tree; they move into the tree.*
- *Routing set on a container was not kept: the background refinement rebuilt the routes orthogonally and dropped the straight-edge set. Fixed in `refine_snapshot_progressive` (the input's straight edges are re-applied) and `apply_straight_edges` is public. Measured in the browser: 63 of 63 routes change and stay changed after 24 s. The engine now reports the straight edges that cross nodes as routing conflicts instead of hiding them.*
- *Direction and strategy changes were measured in the drawing: a direction moves 39 of 40 nodes; a strategy the direction does not allow is refused with its reason.*
- *Open: moving a container (ADR 0009, section 4) is not implemented, and the form still sits below the tree rather than next to the selected row.*

## Open questions

- Maximum size (ADR 0006 allows it). Not in version 1.
- Fixed size with interior relayout inside the box. Needs a layout change; see Decision 2.
- Reparenting: separate command and ADR.
- Whether leaves get size policies in the same form.
