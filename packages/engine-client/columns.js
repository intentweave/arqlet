// Provisional (ADR 0019, step 4): the scene as columns. The engine's own columns (`session.columns()` on a resident
// session in WASM) and this function give the same arrays; this one serves engines that keep their session elsewhere
// (the backend answers JSON only) and is the reference the parity test compares against. Pure: no DOM, no engine.

/** "No node" in an index column (`u32::MAX`). */
export const NONE = 0xffffffff;
/** Bits of `nodeFlags`. */
export const NODE_FLAGS = {container: 1, aggregate: 2, collapsed: 4, invalid: 8};
/** Bits of `routeFlags`. */
export const ROUTE_FLAGS = {invalid: 1, rounded: 2};

/**
 * The columns of a scene (as `session.scene()` gives it), in the scene's order: `nodeIds`, `routeIds` (id tables);
 * `nodeBounds` (x, y, width, height per node), `nodeParent`, `nodeFlags`, `routeSource`, `routeTarget`, `routeWeight`,
 * `routeFlags`, `routeOffsets` (route `i` is points `routeOffsets[i]..routeOffsets[i+1]`) and `routePoints` (x, y
 * alternating). Coordinates are narrowed to `f32` as the engine does.
 */
export function columnsOfScene(scene) {
  const nodes = scene.nodes ?? [];
  const routes = scene.routes ?? [];
  const index = new Map();
  nodes.forEach((n, i) => index.set(n.id, i));
  const at = id => (index.has(id) ? index.get(id) : NONE);
  const nodeBounds = new Float32Array(nodes.length * 4);
  const nodeParent = new Uint32Array(nodes.length);
  const nodeFlags = new Uint32Array(nodes.length);
  nodes.forEach((n, i) => {
    nodeBounds.set([n.bounds.x, n.bounds.y, n.bounds.width, n.bounds.height], i * 4);
    nodeParent[i] = n.parentId == null ? NONE : at(n.parentId);
    nodeFlags[i] = (n.container ? 1 : 0) | (n.aggregate ? 2 : 0) | (n.collapsed ? 4 : 0) | (n.invalid ? 8 : 0);
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
  return {
    revision: scene.revision,
    layoutVersion: scene.layoutVersion,
    nodeIds: nodes.map(n => n.id),
    routeIds: routes.map(r => r.id),
    nodeBounds, nodeParent, nodeFlags, routeSource, routeTarget, routeWeight, routeFlags, routeOffsets, routePoints,
  };
}
