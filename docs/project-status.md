# Project status — 2026-10-21

A working prototype, not yet a production-ready large-graph layouter. Native/WASM parity, snapshot handoff, revisions, nesting, and a shared renderer-independent scene contract are in place. The goal of >100K nodes is not yet demonstrated; refinement currently limits the visible projection to 1000 nodes/2000 edges.

## Algorithm

In place: hierarchical placement, hybrid direction, local free positioning/placement search, automatic and explicit ports, channel routing with A*, occupancy costs, track separation, refinement candidate selection, native cooperative budgets. The measured A* speedup concerns a small frozen example and is not a statement about large graphs.

Open, in this order:

1. Keep the best valid channel-routing round; at present the last round is used. The evaluation must distinguish shared permitted connection segments from unintended overlap.
2. Detect overfull channels and re-route them specifically; where there is no room, extend the placement locally. Larger penalty costs alone do not create space.
3. Evaluate the automatic port side and port order together with routing, and keep explicit port requirements. Adjust port spacing and node sizes to the actual connection demand.
4. Optimize local node moves, rank spacing and container sizes against the channel demand. Afterwards reduce edge proximity to borders, crossings, bends and length, without worsening valid geometry.
5. Connect finer abort points within the router and the browser host clock. Native `budgetMs` is soft; a candidate that has started may finish its calculation.
6. Localize the routing grid, measure spatial indices and reuse. Then investigate size steps up to >100K in the whole graph, with separately bounded visible projections.

## Browser and visualization

In place: SVG demo and React Flow adapter, example selection, demo configuration profiles, direction per container, collapse/expand, labels/port markings, backend/browser/split mode.

New: drag panning, zoom at the mouse pointer, zoom out, an extended JSON field for layout/routing options, a link to the comparison page. The JSON field is a developer feature and does not yet replace a fully developed configuration form.

Open: derive demo and benchmark profiles from the same source (currently different profiles with partly the same names), named form fields with validation, saving/loading own config sets, synchronized panning in the comparison view, viewport culling/LOD. Current navigation supports mouse/pointer and mouse wheel; two-finger pinch zoom is not implemented.

## Evaluation

In place: frozen inputs/hashes, four benchmark profiles, shared metric version 2, runtime/memory data, snapshot determinism, process-group timeout and native profile budgets.

New: `apps/playground/compare.html` loads two snapshot files from the demo or a benchmark. Both are re-evaluated with the current WASM engine, rendered side by side, and optionally scaled to the same size. Structural differences are reported; configurations can be inspected. Crossings with shared end nodes are shown separately. Missing routes are rejected. No runtime is invented from a snapshot.

Open: select results directly from a run; connect input/profile hash, runtime, memory and abort status from run metadata; mark geometric differences; repeated comparisons of all four unified profiles. Labels and container traversals need more precise metrics of their own. Currently there is no automatic tuning or Pareto selection.

## Validation of this continuation

Demo build with native/WASM parity, UI regression checks, server tests and viewer goldens passed. Four additional comparison/navigation tests passed (comparison with the real WASM engine). No visual browser acceptance was performed. No new routing quality improvement is claimed in this continuation: the focus is a more reliable manual comparison basis.

The detailed implementation specification, including priorities and acceptance, is in [Algorithm improvements](algorithm-improvements.md).

The English specification [Curated Diagram References and Visual Editing](curated-references.md)
describes the planned editor, SVG reference artifacts and their evaluation connection.
