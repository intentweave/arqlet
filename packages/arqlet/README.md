# arqlet

**Interactive graphs, continuously arranged.** Arqlet lays out nested graphs with
ports, cycles, self-loops and parallel edges, routes their edges and keeps the layout
up to date as the graph changes. This package is the single entry point for the
engine (WebAssembly, runs in browsers and Node) and its viewers.

```sh
npm install arqlet
```

```js
import {loadEngine, createElkLayout} from 'arqlet';

const run = await loadEngine();          // the engine shipped with @arqlet/engine-client
const elk = createElkLayout(run);        // an elk.layout(graph)-style call
const laidOut = await elk.layout({
  id: 'root',
  children: [{id: 'a', width: 40, height: 30}, {id: 'b', width: 40, height: 30}],
  edges: [{id: 'e1', sources: ['a'], targets: ['b']}, {id: 'e2', sources: ['b'], targets: ['a']}],
});
// laidOut.children have x/y; laidOut.edges have sections. elk.diagnostics lists ELK
// options without an Arqlet counterpart.
```

Every engine operation is a JSON request: `run({op: 'scene', snapshot})`, for
example. `renderSvg(svgElement, scene)` draws a scene; `toReactFlow(scene)` maps it to
React Flow nodes and edges. Types for snapshots and scenes come from
`@arqlet/contracts`.

### A session: drill-down that keeps positions

`createElkLayout` lays out from scratch on every call, like ELK. For collapsing and
expanding groups, keep a session: it holds the current snapshot and sends each change
as an operation on it, so unchanged parts keep their place.

```js
import {loadEngine, createSession} from 'arqlet';

const session = createSession(await loadEngine(), {onChange: () => redraw()});
await session.import(graph);            // {nodes: [{id, kind, label, parentId}], edges: [{id, kind, source, target}]}
await session.collapse('group-1');      // expand, toggle, or apply({op: 'direction', nodeId, direction: 'tb'})
const scene = await session.scene();    // renderSvg(svg, scene) or toReactFlow(scene)
```

For large graphs, import with groups that start collapsed (`options: {communities: 40}`, or `groups` your domain
gives, or `levelOfDetail: {depth: 1}`), and follow a path between two nodes with `session.trace(from, to)` and
`session.reveal(nodes)`; see the repository README, section *Large graphs*.

Changes run one after the other on the newest snapshot. A change that a new `import`
(or `reset()`) overtook resolves to `{ok: false, stale: true}` and is not applied. In
a browser, run the engine in a worker: `createSession(workerRun(createEngineWorker()))`. The snapshot stays in the
engine: `session.snapshot` is the view's state, and `await session.exportSnapshot()` gives the whole snapshot.

Dragging (provisional): positions are in the parent's frame, as `toReactFlow` gives `node.position`. The engine moves
the node step by step and keeps it pinned where it is dropped; positions that arrive while a step is computed replace
each other.

```js
const drag = session.drag(node.id, node.position);   // onNodeDragStart
drag.move(node.position);                            // onNodeDrag (each event)
await drag.drop({settle: true});                     // onNodeDragStop; or drag.cancel()
await session.unpin(node.id);                        // pin(id, position?) / unpin(id)
```

A view controller (provisional) keeps what a user does between two scenes, so a host does not write it again: one
state object with `subscribe`, the view operations, a highlight that stays until it is cleared, back and forward over
the decisions (opening, paging, filters), and the difference between the scenes of every step for animation. No DOM
and no framework; in React the binding is `useSyncExternalStore`:

```js
import {createViewController} from 'arqlet';

const view = createViewController(createSession(await loadEngine()), {page: 12});
const state = useSyncExternalStore(view.subscribe, view.getState);   // {scene, diff, busy, error, highlight, hide, canBack, canForward}
await view.import(graph, {options: {levelOfDetail: {depth: 0}}});
await view.open('group-1');                       // close, toggle, reveal(nodes)
await view.showRest('group-1');                   // the next page of its `+N more` box
await view.trace('a', 'z');                       // kept as state.highlight while the view changes; clearHighlight()
await view.select({edgeId: 'e7'});                // state.details: the edges this line stands for, kept up to date
view.hover({nodeId: 'a'});                        // state.hover, for a tooltip (no engine call)
await view.find('parser');                        // state.search: matches by name or id, also hidden ones
await view.focus('n42');                          // opens what hides it, selects it; state.focus.bounds for the camera
const {bookmark} = await view.bookmark();         // the view as a value; encodeBookmark(bookmark) for a URL
await view.openBookmark(decodeBookmark(text));    // one step back undoes it; another graph's is refused
await view.navigate('right');                     // keys: left/right/up/down, parent/child, next/previous, out/in

// Refinement on a second worker, beside the interactive steps; applied only while the view is unchanged.
const refining = createViewController(session, {refine: workerRefineRun(createEngineWorker(wasmUrl))});
await refining.refine({op: 'refineProgressive'});  // state.refining meanwhile; state.refined: applied, stale or cancelled
await view.filter({nodeKinds: ['test']});
await view.back();                                // forward(); cancel() drops steps not started yet
```

### Cluster overview

For a whole graph (tens of thousands of nodes), the `overview` operation lays it out as nested discs, and the WebGL
renderer draws it; a cluster can then be shown structured, as its layered layout in place:

```js
import {loadEngine, overviewModel, resolvedEdgeIds, createOverviewRenderer, bindOverviewNavigation, viewOf} from 'arqlet';

const run = await loadEngine();
const {overview} = run({op: 'overview', graph});                // nodes with parentId form the clusters
const model = overviewModel(overview, resolvedEdgeIds(graph));
const renderer = createOverviewRenderer(canvas, model, {labels: labelCanvas});
renderer.view = viewOf(model, -1, canvas.clientWidth, canvas.clientHeight);
renderer.resize();
bindOverviewNavigation(canvas, renderer, {onClick: i => console.log(overview.ids[i])});
```

A fisheye lens can follow the pointer: it enlarges the middle (here 3 times) and leaves everything outside its radius
in place, and the texts of what it enlarges appear. Picking sees through it.

```js
bindOverviewNavigation(canvas, renderer, {onClick, lens: () => ({radius: 180, magnification: 3})});
// or renderer.setLens({x, y, radius, magnification}) / renderer.setLens(null) by hand
```

When nobody touches it, the drawing can breathe: the discs grow and shrink a little in a slow wave from the middle, and
the next input stops it. Nothing moves; it is off unless asked for, and off when the system asks for reduced motion.

```js
bindOverviewNavigation(canvas, renderer, {idle: {after: 8000, breathe: {amplitude: 0.06, period: 4000}}});
```

See `apps/playground/overview.js` in the repository for collapsing, structuring in place and animated changes.

In React, the renderer belongs to an effect; a sketch (not tested in this repository):

```jsx
function Overview({model, onSelect}) {
  const canvas = useRef(null), labels = useRef(null);
  useEffect(() => {
    const renderer = createOverviewRenderer(canvas.current, model, {labels: labels.current});
    renderer.view = viewOf(model, -1, canvas.current.clientWidth, canvas.current.clientHeight);
    renderer.resize();
    const unbind = bindOverviewNavigation(canvas.current, renderer, {onClick: i => onSelect(model.overview.ids[i])});
    const observer = new ResizeObserver(() => renderer.resize());
    observer.observe(canvas.current);
    return () => { observer.disconnect(); unbind(); renderer.destroy(); };
  }, [model]);
  return (
    <div style={{position: 'relative', width: '100%', height: '100%'}}>
      <canvas ref={canvas} style={{position: 'absolute', inset: 0, width: '100%', height: '100%'}} />
      <canvas ref={labels} style={{position: 'absolute', inset: 0, width: '100%', height: '100%', pointerEvents: 'none'}} />
    </div>
  );
}
```

Arqlet reads graphs in the JSON form [ELK](https://eclipse.dev/elk/) uses. It is not a
1:1 ELK, contains no ELK code and is not affiliated with the Eclipse Foundation.

> **Status: prototype.** The API, the JSON protocol and the snapshot format are
> provisional and may change in any release.

Part of [Arqlet](https://github.com/intentweave/arqlet), an incremental graph-layout
engine in Rust. See the repository for documentation and the current status.

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
