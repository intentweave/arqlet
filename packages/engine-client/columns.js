// Provisional (ADR 0019, step 4): the scene as columns. The engine's own columns (`session.columns()` on a resident
// session in WASM) and this function give the same arrays; this one serves engines that keep their session elsewhere
// (the backend answers JSON only) and is the reference the parity tests compare against. Pure: no DOM, no engine.

/** "No node" in an index column (`u32::MAX`). */
export const NONE = 0xffffffff;
/** Bits of `nodeFlags`. */
export const NODE_FLAGS = {container: 1, aggregate: 2, collapsed: 4, invalid: 8};
/** Bits of `routeFlags`. */
export const ROUTE_FLAGS = {invalid: 1, rounded: 2};
/** Bits of `portFlags`: authored, and the target end (the source end otherwise). */
export const PORT_FLAGS = {explicit: 1, target: 2};
/** Values of `portSide` and `crossingSide`. */
export const SIDES = {left: 0, right: 1, top: 2, bottom: 3};
/** Values of `nodeShape` (an unknown shape counts as `rect`). */
export const SHAPES = {rect: 0, square: 1, circle: 2};

// Strings kept once, numbered in the order they first occur.
const table = () => {
  const items = [], index = new Map();
  return {items, id: text => { if (!index.has(text)) { index.set(text, items.length); items.push(text); } return index.get(text); }};
};
const rects = (list, get) => {
  const out = new Float32Array(list.length * 4);
  list.forEach((item, i) => { const b = get(item); out.set([b.x, b.y, b.width, b.height], i * 4); });
  return out;
};
const points = (list, get) => {
  const out = new Float32Array(list.length * 2);
  list.forEach((item, i) => { const p = get(item); out[i * 2] = p.x; out[i * 2 + 1] = p.y; });
  return out;
};

/**
 * The columns of a scene (as `session.scene()` gives it), in the scene's order. Id and text tables (`nodeIds`, `nodeLabels`,
 * `nodeKinds`, `routeIds`, `portIds`, `edgeIds`, `labelIds`, `labelTexts`, `labelKinds`, `rowIds`, `rowLabels`,
 * `conflictKinds`, `conflictReasons`, `buses`), `bounds` (x, y, width, height of everything drawn), `showPorts`, and typed
 * arrays: see `SceneColumns`. Coordinates are narrowed to `f32` as the engine does.
 */
export function columnsOfScene(scene) {
  const nodes = scene.nodes ?? [];
  const routes = scene.routes ?? [];
  const ports = scene.ports ?? [];
  const crossings = scene.crossings ?? [];
  const labels = scene.labels ?? [];
  const conflicts = scene.conflicts ?? [];
  const index = new Map();
  nodes.forEach((n, i) => index.set(n.id, i));
  const at = id => (index.has(id) ? index.get(id) : NONE);
  const routeIndex = new Map();
  routes.forEach((r, i) => routeIndex.set(r.id, i));
  const atRoute = id => (routeIndex.has(id) ? routeIndex.get(id) : NONE);

  const nodeBounds = rects(nodes, n => n.bounds);
  const nodeParent = new Uint32Array(nodes.length);
  const nodeFlags = new Uint32Array(nodes.length);
  const nodeKind = new Uint32Array(nodes.length);
  const nodeShape = new Uint32Array(nodes.length);
  const nodeKinds = table();
  nodes.forEach((n, i) => {
    nodeParent[i] = n.parentId == null ? NONE : at(n.parentId);
    nodeFlags[i] = (n.container ? 1 : 0) | (n.aggregate ? 2 : 0) | (n.collapsed ? 4 : 0) | (n.invalid ? 8 : 0);
    nodeKind[i] = nodeKinds.id(n.kind);
    nodeShape[i] = SHAPES[n.shape] ?? 0;
  });

  const routeSource = new Uint32Array(routes.length);
  const routeTarget = new Uint32Array(routes.length);
  const routeWeight = new Uint32Array(routes.length);
  const routeFlags = new Uint32Array(routes.length);
  const routeOffsets = new Uint32Array(routes.length + 1);
  let total = 0;
  for (const r of routes) total += r.points.length;
  const routePoints = new Float32Array(total * 2);
  let cursor = 0;
  routes.forEach((r, i) => {
    routeSource[i] = at(r.source);
    routeTarget[i] = at(r.target);
    routeWeight[i] = Math.min(r.weight ?? 0, NONE);
    routeFlags[i] = (r.invalid ? 1 : 0) | (r.rounded ? 2 : 0);
    for (const p of r.points) { routePoints[cursor++] = p.x; routePoints[cursor++] = p.y; }
    routeOffsets[i + 1] = cursor / 2;
  });

  // Ports and boundary crossings name edges; the edges are numbered once, ports first.
  const edges = table();
  const portNode = Uint32Array.from(ports, p => at(p.nodeId));
  const portEdge = Uint32Array.from(ports, p => edges.id(p.edgeId));
  const portFlags = Uint32Array.from(ports, p => (p.explicit ? 1 : 0) | (p.endpoint === 'target' ? 2 : 0));
  const portSide = Uint32Array.from(ports, p => SIDES[p.side]);
  const crossingEdge = Uint32Array.from(crossings, x => edges.id(x.edgeId));
  const labelKinds = table();

  const conflictKinds = table();
  const conflictNodes = [], conflictRoutes = [], conflictNodeOffsets = [0], conflictRouteOffsets = [0];
  for (const k of conflicts) {
    for (const id of k.nodes) conflictNodes.push(at(id));
    for (const id of k.routes) conflictRoutes.push(atRoute(id));
    conflictNodeOffsets.push(conflictNodes.length);
    conflictRouteOffsets.push(conflictRoutes.length);
  }
  const conflictKind = Uint32Array.from(conflicts, k => conflictKinds.id(k.kind));

  const rows = [];
  nodes.forEach((n, i) => { for (const row of n.rows ?? []) rows.push({row, node: i}); });
  const b = scene.bounds;

  return {
    revision: scene.revision,
    layoutVersion: scene.layoutVersion,
    nodeIds: nodes.map(n => n.id),
    nodeLabels: nodes.map(n => n.label),
    nodeKinds: nodeKinds.items,
    routeIds: routes.map(r => r.id),
    portIds: ports.map(p => p.id),
    edgeIds: edges.items,
    labelIds: labels.map(l => l.id),
    labelTexts: labels.map(l => l.text),
    labelKinds: labelKinds.items,
    rowIds: rows.map(r => r.row.id),
    rowLabels: rows.map(r => r.row.label),
    conflictKinds: conflictKinds.items,
    conflictReasons: conflicts.map(k => k.reason),
    buses: scene.buses ?? [],
    bounds: b ? [b.x, b.y, b.width, b.height] : [0, 0, 0, 0],
    showPorts: Boolean(scene.showPorts),
    nodeBounds, nodeParent, nodeFlags, nodeKind, nodeShape,
    routeSource, routeTarget, routeWeight, routeFlags, routeOffsets, routePoints,
    portNode, portEdge, portFlags, portSide, portPosition: points(ports, p => p.position),
    crossingEdge,
    crossingContainer: Uint32Array.from(crossings, x => at(x.containerId)),
    crossingOrdinal: Uint32Array.from(crossings, x => Math.min(x.ordinal, NONE)),
    crossingSide: Uint32Array.from(crossings, x => SIDES[x.side]),
    crossingFlags: Uint32Array.from(crossings, x => (x.derived ? 1 : 0)),
    crossingPosition: points(crossings, x => x.position),
    labelKind: Uint32Array.from(labels, l => labelKinds.id(l.kind)),
    labelBounds: rects(labels, l => l.bounds),
    rowNode: Uint32Array.from(rows, r => r.node),
    rowBounds: rects(rows, r => r.row.bounds),
    conflictKind,
    conflictNodeOffsets: Uint32Array.from(conflictNodeOffsets),
    conflictNodes: Uint32Array.from(conflictNodes),
    conflictRouteOffsets: Uint32Array.from(conflictRouteOffsets),
    conflictRoutes: Uint32Array.from(conflictRoutes),
    junctions: points(scene.junctions ?? [], p => p),
  };
}
