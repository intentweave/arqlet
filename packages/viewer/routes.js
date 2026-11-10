// Route selection support: details of one scene route, computed from the scene's absolute points.
// Pure: no DOM. The crossing test is strict (interior crossings only; touching ends are not counted).

const EPS = 1e-6;

// Segments of a polyline with repeated points removed; collinear points stay (they are not bends).
function segments(points) {
  const cleaned = points.filter((p, i) => i === 0 || Math.hypot(p.x - points[i - 1].x, p.y - points[i - 1].y) > EPS);
  return cleaned.slice(1).map((p, i) => [cleaned[i], p]);
}

// Direction changes of a polyline; collinear and repeated points do not count.
export function bendsOf(points) {
  const cleaned = points.filter((p, i) => i === 0 || Math.hypot(p.x - points[i - 1].x, p.y - points[i - 1].y) > EPS);
  let bends = 0;
  for (let i = 1; i + 1 < cleaned.length; i++) {
    const ax = cleaned[i].x - cleaned[i - 1].x, ay = cleaned[i].y - cleaned[i - 1].y;
    const bx = cleaned[i + 1].x - cleaned[i].x, by = cleaned[i + 1].y - cleaned[i].y;
    if (Math.abs(ax * by - ay * bx) > EPS) bends++;
  }
  return bends;
}

export function pathLength(points) {
  return segments(points).reduce((sum, [a, b]) => sum + Math.hypot(b.x - a.x, b.y - a.y), 0);
}

function properCrossing([a, b], [c, d]) {
  const orient = (p, q, r) => (q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x);
  const o1 = orient(a, b, c), o2 = orient(a, b, d), o3 = orient(c, d, a), o4 = orient(c, d, b);
  return ((o1 > EPS && o2 < -EPS) || (o1 < -EPS && o2 > EPS)) && ((o3 > EPS && o4 < -EPS) || (o3 < -EPS && o4 > EPS));
}

export function crosses(pointsA, pointsB) {
  return segments(pointsA).some(s => segments(pointsB).some(t => properCrossing(s, t)));
}

// Details of one route; null when the scene has no such route. `kinds` maps an edge id to its kind.
export function routeDetails(scene, routeId, kinds = {}) {
  const route = scene.routes.find(r => r.id === routeId);
  if (!route) return null;
  const crossings = scene.routes.filter(other => other.id !== route.id && crosses(route.points, other.points)).map(other => other.id);
  return {
    id: route.id,
    kind: kinds[route.id] ?? '',
    source: route.source,
    target: route.target,
    length: Math.round(pathLength(route.points)),
    bends: bendsOf(route.points),
    crossings,
    members: route.members ?? [],
  };
}

// The drawing the engine's shared metrics read, from a scene: absolute node bounds, drawn routes, and the ports
// that identify each route end. The engine is the only place that computes conflicts.
export function drawingOf(scene) {
  const ports = new Map(scene.ports.map(p => [`${p.edgeId}:${p.endpoint}`, p.id]));
  return {
    nodes: scene.nodes.map(n => ({
      id: n.id, parentId: n.parentId ?? null, container: !!n.container,
      x: n.bounds.x, y: n.bounds.y, width: n.bounds.width, height: n.bounds.height,
    })),
    edges: scene.routes.map(r => ({
      id: r.id, source: r.source, target: r.target, points: r.points,
      sourcePort: ports.get(`${r.id}:source`) ?? null, targetPort: ports.get(`${r.id}:target`) ?? null,
    })),
    trackSpacing: 6,
    // Members of one bus may share lines and meet (ADR 0005).
    ...(scene.buses?.length ? {buses: scene.buses} : {}),
  };
}

// The conflicts of one route as the engine counts them. `run` is the engine's request function.
export function routeConflictsOf(run, scene, routeId) {
  const result = run({op: 'routeConflicts', drawing: drawingOf(scene), edgeId: routeId});
  if (!result.ok) throw Error(result.error);
  return result.conflicts;
}
