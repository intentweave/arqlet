// Provisional (ADR 0019, step 4): a scene from its columns (`session.columns()` of the engine client), the inverse of the
// engine client's `columnsOfScene`. The SVG viewer takes columns through it. Pure: no DOM.
//
// What a scene object holds that the columns do not: nothing a viewer draws. Coordinates are the columns' `f32`. A route
// end or a conflict party outside the scene (index `NONE`) has no id any more: a route end then reads '' (the engine
// builds a route only between nodes of the scene), and a conflict leaves such a party out.

const NONE = 0xffffffff;
const SIDES = ['left', 'right', 'top', 'bottom'];
const SHAPES = ['rect', 'square', 'circle', 'busLine'];

const rect = (a, i) => ({x: a[i * 4], y: a[i * 4 + 1], width: a[i * 4 + 2], height: a[i * 4 + 3]});
const point = (a, i) => ({x: a[i * 2], y: a[i * 2 + 1]});
const range = n => Array.from({length: n}, (_, i) => i);

/** Whether `scene` is the columns of a scene rather than a scene object. */
export const isSceneColumns = scene => scene?.nodeBounds instanceof Float32Array;

/** The scene object of `columns` (as `build_scene` gives it, with `f32` coordinates). */
export function sceneOfColumns(c) {
  const nodeAt = i => (i === NONE ? null : c.nodeIds[i]);
  const rowsOf = new Map();
  c.rowNode.forEach((node, r) => {
    if (!rowsOf.has(node)) rowsOf.set(node, []);
    rowsOf.get(node).push({id: c.rowIds[r], label: c.rowLabels[r], bounds: rect(c.rowBounds, r)});
  });
  const nodes = c.nodeIds.map((id, i) => {
    const flags = c.nodeFlags[i];
    const n = {id, kind: c.nodeKinds[c.nodeKind[i]], label: c.nodeLabels[i], parentId: nodeAt(c.nodeParent[i])};
    if (c.nodeShape[i]) n.shape = SHAPES[c.nodeShape[i]];
    Object.assign(n, {bounds: rect(c.nodeBounds, i), container: !!(flags & 1), aggregate: !!(flags & 2), collapsed: !!(flags & 4), invalid: !!(flags & 8)});
    if (rowsOf.has(i)) n.rows = rowsOf.get(i);
    return n;
  });
  const routes = c.routeIds.map((id, i) => {
    const flags = c.routeFlags[i];
    const r = {id};
    const kind = c.routeKinds[c.routeKind[i]];
    if (kind) r.kind = kind;
    r.source = nodeAt(c.routeSource[i]) ?? '';
    r.target = nodeAt(c.routeTarget[i]) ?? '';
    const from = c.routeOffsets[i], to = c.routeOffsets[i + 1];
    r.points = range(to - from).map(k => point(c.routePoints, from + k));
    r.weight = c.routeWeight[i];
    if (c.routeBackWeight[i] !== NONE) r.backWeight = c.routeBackWeight[i];
    r.invalid = !!(flags & 1);
    if (flags & 2) r.rounded = true;
    const first = c.routeMemberOffsets[i], last = c.routeMemberOffsets[i + 1];
    if (last > first) r.members = range(last - first).map(k => c.edgeIds[c.routeMembers[first + k]]);
    return r;
  });
  const conflicts = c.conflictReasons.map((reason, i) => ({
    kind: c.conflictKinds[c.conflictKind[i]],
    nodes: range(c.conflictNodeOffsets[i + 1] - c.conflictNodeOffsets[i]).map(k => c.conflictNodes[c.conflictNodeOffsets[i] + k]).filter(j => j !== NONE).map(j => c.nodeIds[j]),
    routes: range(c.conflictRouteOffsets[i + 1] - c.conflictRouteOffsets[i]).map(k => c.conflictRoutes[c.conflictRouteOffsets[i] + k]).filter(j => j !== NONE).map(j => c.routeIds[j]),
    reason,
  }));
  const scene = {
    schemaVersion: c.schemaVersion, revision: c.revision, layoutVersion: c.layoutVersion,
    nodes, routes,
    ports: c.portIds.map((id, i) => ({
      id, nodeId: nodeAt(c.portNode[i]) ?? '', edgeId: c.edgeIds[c.portEdge[i]], endpoint: c.portFlags[i] & 2 ? 'target' : 'source',
      side: SIDES[c.portSide[i]], position: point(c.portPosition, i), explicit: !!(c.portFlags[i] & 1),
    })),
    labels: c.labelIds.map((id, i) => ({id, kind: c.labelKinds[c.labelKind[i]], text: c.labelTexts[i], bounds: rect(c.labelBounds, i)})),
    crossings: range(c.crossingEdge.length).map(i => ({
      edgeId: c.edgeIds[c.crossingEdge[i]], containerId: nodeAt(c.crossingContainer[i]) ?? '', ordinal: c.crossingOrdinal[i],
      side: SIDES[c.crossingSide[i]], position: point(c.crossingPosition, i), derived: !!(c.crossingFlags[i] & 1),
    })),
    showPorts: c.showPorts,
  };
  if (conflicts.length) scene.conflicts = conflicts;
  if (c.buses.length) scene.buses = c.buses;
  if (c.busLines?.length) scene.busLines = c.busLines;
  if (c.junctions.length) scene.junctions = range(c.junctions.length / 2).map(i => point(c.junctions, i));
  scene.bounds = {x: c.bounds[0], y: c.bounds[1], width: c.bounds[2], height: c.bounds[3]};
  return scene;
}
