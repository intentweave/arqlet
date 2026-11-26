// Probe for ADR 0028 (step B0): hierarchical edge bundling computed from an engine scene, outside the engine. Sections 4
// and 5 of the ADR: which edges are eligible, the control polygon (three kinds of control points), the straightened cubic
// B-spline, parallel offsets, the cut at the node outlines and the guard that lets a curve enter no node its unbundled
// route does not enter. Pure and deterministic; nothing here is the engine's code. Used by `bundling-probe.mjs`.

export const RULES = {
  spacing: 6, // between the strands of edges with the same two ends (the track spacing)
  sampleLength: 8, minSegments: 8, maxSegments: 48,
  clearance: 6, // DEFAULT_REQUIRED_CLEARANCE, when the options name none
  header: 36, padding: 12, // GeometryRules: where a free place may lie inside a container
  freeStep: 4, freeMargins: [12, 6, 0], // free place: grid step, and the room from the children, tried in this order
};
export const VARIANTS = ['centre', 'free', 'gate'];
const EPS = 1e-6;

export const centreOf = r => ({x: r.x + r.width / 2, y: r.y + r.height / 2});
export const lengthOf = points => points.reduce((sum, p, i) => i ? sum + Math.hypot(p.x - points[i - 1].x, p.y - points[i - 1].y) : 0, 0);

// ---------- The view ----------
export function viewOf(scene, snapshot) {
  const nodes = new Map(scene.nodes.map(n => [n.id, n]));
  const children = new Map();
  for (const n of scene.nodes) if (n.parentId) { if (!children.has(n.parentId)) children.set(n.parentId, []); children.get(n.parentId).push(n); }
  const chain = id => { const out = []; for (let p = nodes.get(id)?.parentId; p; p = nodes.get(p)?.parentId) out.push(p); return out; };
  const required = snapshot.options?.requiredClearance;
  return {scene, snapshot, nodes, children, chain, clearance: required === undefined ? RULES.clearance : required ?? 0,
    grid: nodeGrid(scene.nodes), free: new Map()};
}

/** Section 4: the eligible edges (scene order) and the others counted by the first reason that applies. */
export function eligibility(view) {
  const {scene, snapshot, chain} = view;
  const straight = new Set(snapshot.straightEdges ?? []);
  const bound = new Set((snapshot.graph.edgePorts ?? []).filter(p => p.sourcePort || p.targetPort).map(p => p.edgeId));
  const bus = new Set([...(scene.buses ?? []).flat(), ...(scene.busLines ?? []).flatMap(b => b.taps.map(t => t.edge)), ...(scene.hubRefs ?? []).map(h => h.edge)]);
  const labelled = new Set(Object.keys(snapshot.options?.edgeLabels ?? {}));
  const excluded = {selfLoop: 0, containment: 0, bus: 0, notStraight: 0, detoured: 0, explicitPort: 0, labelled: 0, sameContainer: 0};
  const eligible = [];
  for (const route of scene.routes) {
    const {id, source, target} = route;
    const up = chain(source), down = chain(target);
    const lca = up.find(a => down.includes(a)) ?? null;
    const sourceSide = lca ? up.slice(0, up.indexOf(lca)) : up;
    const targetSide = lca ? down.slice(0, down.indexOf(lca)) : down;
    const reason = source === target ? 'selfLoop'
      : up.includes(target) || down.includes(source) ? 'containment'
      : bus.has(id) ? 'bus'
      : !straight.has(id) ? 'notStraight'
      : route.points.length !== 2 ? 'detoured'
      : bound.has(id) ? 'explicitPort'
      : labelled.has(id) ? 'labelled'
      : !sourceSide.length && !targetSide.length ? 'sameContainer' : null;
    if (reason) excluded[reason]++;
    else eligible.push({id, source, target, base: route.points, sourceSide, targetSide, holding: new Set([...up, ...down])});
  }
  return {eligible, excluded};
}

// ---------- Control points ----------
const distanceToBox = (p, r) => Math.hypot(Math.max(r.x - p.x, 0, p.x - r.x - r.width), Math.max(r.y - p.y, 0, p.y - r.y - r.height));

/** The point nearest the container's centre, on a grid inside its content area, with room from every child: 12 units,
 * else 6, else 0 (outside every child); the centre when the children cover the whole area. */
export function freePlace(box, childBoxes) {
  const c = centreOf(box);
  const x0 = box.x + RULES.padding, x1 = box.x + box.width - RULES.padding;
  const y0 = box.y + RULES.header, y1 = box.y + box.height - RULES.padding;
  const best = RULES.freeMargins.map(() => null);
  // Grid points are centred on the container's centre, so that the centre itself is a candidate.
  const step = RULES.freeStep;
  for (let j = Math.ceil((y0 - c.y) / step); c.y + j * step <= y1; j++) {
    for (let i = Math.ceil((x0 - c.x) / step); c.x + i * step <= x1; i++) {
      const p = {x: c.x + i * step, y: c.y + j * step};
      const d = Math.hypot(p.x - c.x, p.y - c.y);
      if (best[0] && d >= best[0].d) continue;
      let room = Infinity;
      for (const r of childBoxes) {
        const inside = p.x > r.x && p.x < r.x + r.width && p.y > r.y && p.y < r.y + r.height;
        room = Math.min(room, inside ? -1 : distanceToBox(p, r));
        if (room < 0) break;
      }
      for (let m = 0; m < RULES.freeMargins.length; m++) {
        const need = RULES.freeMargins[m];
        const ok = need > 0 ? room >= need - EPS : room > EPS;
        if (ok && (!best[m] || d < best[m].d)) best[m] = {...p, d};
      }
    }
  }
  const found = best.find(Boolean);
  return found ? {x: found.x, y: found.y} : c;
}

/** Where the ray from the container's centre towards `toward` meets its border. */
export function gate(node, toward) {
  const r = node.bounds, c = centreOf(r);
  const dx = toward.x - c.x, dy = toward.y - c.y;
  if (Math.hypot(dx, dy) < EPS) return c;
  if (node.shape === 'circle') {
    const radius = Math.min(r.width, r.height) / 2, len = Math.hypot(dx, dy);
    return {x: c.x + dx / len * radius, y: c.y + dy / len * radius};
  }
  const t = Math.min(Math.abs(dx) > EPS ? r.width / 2 / Math.abs(dx) : Infinity, Math.abs(dy) > EPS ? r.height / 2 / Math.abs(dy) : Infinity);
  return {x: c.x + dx * t, y: c.y + dy * t};
}

/** Section 4: source centre, a point per source container below the LCA (inner first), one per target container (outer
 * first), target centre. A gate faces the centre of the outermost box on the other side (the LCA's child there), so that
 * edges between the same two such boxes share their gates. */
export function controlPolygon(view, edge, variant) {
  const {nodes, children, free} = view;
  const outermost = (side, end) => centreOf(nodes.get(side.length ? side[side.length - 1] : end).bounds);
  const point = (id, toward) => {
    const node = nodes.get(id);
    if (variant === 'centre') return centreOf(node.bounds);
    if (variant === 'gate') return gate(node, toward);
    if (!free.has(id)) free.set(id, freePlace(node.bounds, (children.get(id) ?? []).map(n => n.bounds)));
    return free.get(id);
  };
  const towardTarget = outermost(edge.targetSide, edge.target), towardSource = outermost(edge.sourceSide, edge.source);
  return [centreOf(nodes.get(edge.source).bounds),
    ...edge.sourceSide.map(id => point(id, towardTarget)),
    ...[...edge.targetSide].reverse().map(id => point(id, towardSource)),
    centreOf(nodes.get(edge.target).bounds)];
}

/** What the hierarchy of a view offers the method (diagnostics, not a gate). A section of a cubic spline is the same for
 * two edges only where they share four consecutive control points; edges between the same two outermost boxes are the
 * ones that can come close at all. */
export function structureOf(view, eligible) {
  const points = {}, pairs = new Map(), windows = new Map(), containers = new Set();
  const vias = eligible.map(e => [...e.sourceSide, ...[...e.targetSide].reverse()]);
  const pairOf = e => JSON.stringify([e.sourceSide.at(-1) ?? e.source, e.targetSide.at(-1) ?? e.target].sort());
  const keysOf = via => via.slice(3).map((_, i) => { const w = via.slice(i, i + 4); return [JSON.stringify(w), JSON.stringify([...w].reverse())].sort()[0]; });
  eligible.forEach((e, i) => {
    const n = vias[i].length + 2;
    points[n] = (points[n] ?? 0) + 1;
    for (const id of vias[i]) containers.add(id);
    pairs.set(pairOf(e), (pairs.get(pairOf(e)) ?? 0) + 1);
    for (const k of new Set(keysOf(vias[i]))) windows.set(k, (windows.get(k) ?? 0) + 1);
  });
  let centreCovered = 0, freeMoved = 0;
  for (const id of containers) {
    const box = view.nodes.get(id).bounds, c = centreOf(box), children = (view.children.get(id) ?? []).map(n => n.bounds);
    if (children.some(r => c.x > r.x && c.x < r.x + r.width && c.y > r.y && c.y < r.y + r.height)) centreCovered++;
    const p = freePlace(box, children);
    if (Math.hypot(p.x - c.x, p.y - c.y) > EPS) freeMoved++;
  }
  return {controlPoints: points, containersOnPaths: containers.size, centreCovered, freeMoved,
    withCompanion: eligible.filter(e => pairs.get(pairOf(e)) > 1).length,
    withSharedSection: eligible.filter((e, i) => keysOf(vias[i]).some(k => windows.get(k) > 1)).length};
}

// ---------- The curve ----------
/** Holten, equation (1): every control point moved towards its place on the chord. */
export function straighten(points, strength) {
  const n = points.length, first = points[0], last = points[n - 1];
  return points.map((p, i) => {
    const t = i / (n - 1);
    return {x: strength * p.x + (1 - strength) * (first.x + t * (last.x - first.x)), y: strength * p.y + (1 - strength) * (first.y + t * (last.y - first.y))};
  });
}

/** A B-spline with an open uniform knot vector, cubic (degree 2 or 1 for three or two points), as `segments + 1` points. */
export function bspline(points, segments) {
  const n = points.length, degree = Math.min(3, n - 1), spans = n - degree;
  const knots = [];
  for (let i = 0; i < n + degree + 1; i++) knots.push(Math.min(spans, Math.max(0, i - degree)));
  const out = [];
  for (let s = 0; s <= segments; s++) {
    const u = spans * s / segments;
    const k = degree + Math.min(spans - 1, Math.floor(u));
    const d = [];
    for (let j = 0; j <= degree; j++) d.push({...points[k - degree + j]});
    for (let r = 1; r <= degree; r++) {
      for (let j = degree; j >= r; j--) {
        const i = k - degree + j;
        const a = (u - knots[i]) / (knots[i + degree - r + 1] - knots[i]);
        d[j] = {x: (1 - a) * d[j - 1].x + a * d[j].x, y: (1 - a) * d[j - 1].y + a * d[j].y};
      }
    }
    out.push(d[degree]);
  }
  return out;
}

export const segmentCount = polygon => Math.min(RULES.maxSegments, Math.max(RULES.minSegments, Math.ceil(lengthOf(polygon) / RULES.sampleLength)));

/** Section 5: the translation of each of the eligible edges that join the same two ends, keyed by edge id. */
export function parallelOffsets(view, edges) {
  const groups = new Map();
  for (const e of edges) {
    const ends = [e.source, e.target].sort();
    const key = JSON.stringify(ends);
    if (!groups.has(key)) groups.set(key, {ends, ids: []});
    groups.get(key).ids.push(e.id);
  }
  const offsets = new Map();
  for (const {ends, ids} of groups.values()) {
    const [a, b] = ends.map(id => view.nodes.get(id).bounds);
    const from = centreOf(a), to = centreOf(b), len = Math.hypot(to.x - from.x, to.y - from.y) || 1;
    const normal = {x: -(to.y - from.y) / len, y: (to.x - from.x) / len};
    const half = Math.min(a.width, a.height, b.width, b.height) / 2;
    const spacing = ids.length > 1 ? Math.max(0, Math.min(RULES.spacing, (half - 1) * 2 / (ids.length - 1))) : 0;
    ids.sort().forEach((id, k) => {
      const t = (k - (ids.length - 1) / 2) * spacing;
      offsets.set(id, {x: normal.x * t, y: normal.y * t});
    });
  }
  return offsets;
}

// ---------- Outlines ----------
/** Whether the segment enters the node's outline grown by `margin` (as `geom::enters` and the metrics judge it). */
export function enters(a, b, node, margin = 0) {
  const r = node.bounds;
  if (node.shape === 'circle') {
    const cx = r.x + r.width / 2, cy = r.y + r.height / 2, radius = Math.min(r.width, r.height) / 2 + margin;
    const dx = b.x - a.x, dy = b.y - a.y, len = dx * dx + dy * dy;
    const t = len > 0 ? Math.min(1, Math.max(0, ((cx - a.x) * dx + (cy - a.y) * dy) / len)) : 0;
    return Math.hypot(a.x + dx * t - cx, a.y + dy * t - cy) < radius - 1e-3;
  }
  const x0 = r.x - margin, y0 = r.y - margin, x1 = r.x + r.width + margin, y1 = r.y + r.height + margin;
  const dx = b.x - a.x, dy = b.y - a.y;
  let t0 = 0, t1 = 1;
  for (const [p, q] of [[-dx, a.x - x0], [dx, x1 - a.x], [-dy, a.y - y0], [dy, y1 - a.y]]) {
    if (Math.abs(p) < EPS) { if (q < -EPS) return false; continue; }
    const t = q / p;
    if (p < 0) t0 = Math.max(t0, t); else t1 = Math.min(t1, t);
  }
  if (t1 - t0 <= EPS) return false;
  const x = a.x + dx * (t0 + t1) / 2, y = a.y + dy * (t0 + t1) / 2;
  return x > x0 + EPS && x < x1 - EPS && y > y0 + EPS && y < y1 - EPS;
}
export const pathEnters = (points, node, margin = 0) => points.some((p, i) => i > 0 && enters(points[i - 1], p, node, margin));

const within = (p, node) => {
  const r = node.bounds;
  if (node.shape === 'circle') return Math.hypot(p.x - r.x - r.width / 2, p.y - r.y - r.height / 2) <= Math.min(r.width, r.height) / 2;
  return p.x >= r.x && p.x <= r.x + r.width && p.y >= r.y && p.y <= r.y + r.height;
};
/** The point where the segment from `a` (inside) to `b` (outside) leaves the outline. */
function exit(a, b, node) {
  let lo = 0, hi = 1;
  for (let i = 0; i < 60; i++) {
    const mid = (lo + hi) / 2;
    if (within({x: a.x + (b.x - a.x) * mid, y: a.y + (b.y - a.y) * mid}, node)) lo = mid; else hi = mid;
  }
  return {x: a.x + (b.x - a.x) * lo, y: a.y + (b.y - a.y) * lo};
}
/** Section 5: the curve without its way inside its two nodes; null when it never gets out of one. */
export function cut(points, source, target) {
  const trim = (list, node) => {
    const i = list.findIndex(p => !within(p, node));
    if (i < 0) return null;
    return i === 0 ? list : [exit(list[i - 1], list[i], node), ...list.slice(i)];
  };
  const head = trim(points, source);
  const tail = head && trim([...head].reverse(), target);
  return tail && tail.length >= 2 ? tail.reverse() : null;
}

// ---------- The guard ----------
const CELL = 256;
function nodeGrid(nodes) {
  const cells = new Map();
  for (const n of nodes) {
    const r = n.bounds;
    for (let x = Math.floor(r.x / CELL); x <= Math.floor((r.x + r.width) / CELL); x++) {
      for (let y = Math.floor(r.y / CELL); y <= Math.floor((r.y + r.height) / CELL); y++) {
        const key = `${x},${y}`;
        if (!cells.has(key)) cells.set(key, []);
        cells.get(key).push(n);
      }
    }
  }
  return cells;
}
function near(view, points) {
  const found = new Set(), margin = view.clearance + 1;
  for (let i = 1; i < points.length; i++) {
    const a = points[i - 1], b = points[i];
    for (let x = Math.floor((Math.min(a.x, b.x) - margin) / CELL); x <= Math.floor((Math.max(a.x, b.x) + margin) / CELL); x++) {
      for (let y = Math.floor((Math.min(a.y, b.y) - margin) / CELL); y <= Math.floor((Math.max(a.y, b.y) + margin) / CELL); y++) {
        for (const n of view.grid.get(`${x},${y}`) ?? []) found.add(n);
      }
    }
  }
  return found;
}
/** Section 5: why the curve is refused (`node`, `clearance`, `end`, `label`), or null when it adds no hit. */
export function refusal(view, edge, points, candidates = near(view, points)) {
  for (const node of candidates) {
    if (node.id === edge.source || node.id === edge.target || edge.holding.has(node.id)) continue;
    if (pathEnters(edge.base, node)) continue;
    if (pathEnters(points, node)) return 'node';
    if (view.clearance > 0 && !pathEnters(edge.base, node, view.clearance) && pathEnters(points, node, view.clearance)) return 'clearance';
  }
  for (const id of [edge.source, edge.target]) if (pathEnters(points, view.nodes.get(id))) return 'end';
  for (const label of view.scene.labels ?? []) {
    if (!pathEnters(edge.base, label) && pathEnters(points, label)) return 'label';
  }
  return null;
}

/** One edge bundled with one kind of control points: tried at the strength, half and a quarter of it, else unbundled.
 * `refused` names why the curve at full strength was not taken. */
export function bundleEdge(view, edge, offset, variant, strength) {
  const polygon = controlPolygon(view, edge, variant);
  let refused = null;
  for (const [level, factor] of [['full', 1], ['weakened', 0.5], ['weakened', 0.25]]) {
    const straightened = straighten(polygon, strength * factor).map(p => ({x: p.x + offset.x, y: p.y + offset.y}));
    const curve = cut(bspline(straightened, segmentCount(straightened)), view.nodes.get(edge.source), view.nodes.get(edge.target));
    const why = curve ? refusal(view, edge, curve) : 'end';
    if (!why) return {level, strength: strength * factor, points: curve, refused};
    refused ??= why;
  }
  return {level: 'kept', strength: 0, points: edge.base, refused};
}

// ---------- Ink ----------
const TILE = 256;
/** The number of cells of a raster of `size` units that the polylines pass through. */
export function inkCells(paths, size) {
  const tiles = new Map();
  let count = 0;
  const mark = (x, y) => {
    const tx = Math.floor(x / TILE), ty = Math.floor(y / TILE), key = tx * 1048576 + ty;
    let tile = tiles.get(key);
    if (!tile) tiles.set(key, tile = new Uint8Array(TILE * TILE));
    const at = (x - tx * TILE) + (y - ty * TILE) * TILE;
    if (!tile[at]) { tile[at] = 1; count++; }
  };
  for (const points of paths) {
    for (let i = 1; i < points.length; i++) {
      const ax = points[i - 1].x / size, ay = points[i - 1].y / size, bx = points[i].x / size, by = points[i].y / size;
      let x = Math.floor(ax), y = Math.floor(ay);
      const ex = Math.floor(bx), ey = Math.floor(by);
      const dx = bx - ax, dy = by - ay, sx = Math.sign(dx), sy = Math.sign(dy);
      let tx = sx ? ((sx > 0 ? x + 1 : x) - ax) / dx : Infinity, ty = sy ? ((sy > 0 ? y + 1 : y) - ay) / dy : Infinity;
      mark(x, y);
      for (let steps = Math.abs(ex - x) + Math.abs(ey - y); steps > 0; steps--) {
        if (tx < ty) { x += sx; tx += Math.abs(1 / dx); } else { y += sy; ty += Math.abs(1 / dy); }
        mark(x, y);
      }
    }
  }
  return count;
}
