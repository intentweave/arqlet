import {createStyle} from './style.js';
import {parseColour, sceneBuffers, BOX_STRIDE, SEGMENT_STRIDE, TRIANGLE_STRIDE} from './scene-buffers.js';
// Cluster overview (ADR 0018), the part without DOM or WebGL: the hierarchy of an `overview` result, the vertex buffers
// a renderer draws, picking and the excerpt of a drill-down. Pure; the engine computes every position.

/**
 * Indexes an overview result (`{ids, parent, x, y, r, edges, links, rootRadius}`) for drawing and picking. `edgeIds`
 * names the result's edges (the resolved edges of the graph, in input order; see `resolvedEdgeIds`); without them, edges
 * that leave a structured cluster start at their box instead of at their border port.
 */
export function overviewModel(overview, edgeIds = null) {
  if (overview?.schemaVersion !== 1) throw Error('Unsupported overview schema');
  const n = overview.ids.length;
  const children = Array.from({length: n}, () => []);
  const roots = [];
  for (let i = 0; i < n; i++) {
    const p = overview.parent[i];
    if (p >= 0) children[p].push(i); else roots.push(i);
  }
  // Depth, the number of leaves and of nodes below every node, and the cluster that gives every node its colour.
  const depth = new Int32Array(n), cluster = new Int32Array(n), leaves = new Int32Array(n), size = new Int32Array(n);
  const order = [];
  const stack = roots.map(i => [i, 0]);
  while (stack.length) {
    const [i, d] = stack.pop();
    depth[i] = d; order.push(i);
    for (const c of children[i]) stack.push([c, d + 1]);
  }
  for (let k = order.length - 1; k >= 0; k--) {
    const i = order[k];
    if (!children[i].length) leaves[i] = 1;
    const p = overview.parent[i];
    if (p >= 0) { leaves[p] += leaves[i]; size[p] += size[i] + 1; }
  }
  // The clusters that give the colours, so that they tell the main parts apart: from the top level, the largest one
  // that holds more than a quarter of the graph is opened (its children take its place), again and again; only a cluster
  // whose children include clusters is opened (a repository's `.`, then `packages` and `plugins`), never one of leaves.
  let tops = [...roots];
  const opened = new Set();
  const isOpenable = i => !opened.has(i) && children[i].some(c => children[c].length > 0);
  for (let round = 0; round < 64; round++) {
    const big = tops.filter(isOpenable).sort((a, b) => size[b] - size[a] || a - b)[0];
    if (big === undefined || size[big] + 1 <= n / 4) break;
    opened.add(big);
    tops = [...tops, ...children[big]];
  }
  const colourOf = new Map(tops.map(i => [i, i]));
  for (const i of order) {
    const p = overview.parent[i];
    cluster[i] = colourOf.has(i) ? i : p >= 0 ? cluster[p] : i;
  }
  // Chains: a node that is the only child of its parent and has the same disc (the overview's option `chains`) is drawn,
  // named and picked with that parent; `chainTop` is the outermost node of each node's chain (the node itself if none).
  const chainTop = new Int32Array(n);
  const same = (a, b) => Math.abs(overview.x[a] - overview.x[b]) + Math.abs(overview.y[a] - overview.y[b]) +
    Math.abs(overview.r[a] - overview.r[b]) <= 1e-6 * Math.max(overview.r[a], 1);
  for (const i of order) {
    const p = overview.parent[i];
    chainTop[i] = p >= 0 && children[p].length === 1 && same(p, i) ? chainTop[p] : i;
  }
  const index = new Map(overview.ids.map((id, i) => [id, i]));
  return {overview, n, children, roots, tops, depth, cluster, leaves, size, index, edgeIds, chainTop};
}

/** The ids of a graph's resolved edges, in input order: the edges of its `overview` result. */
export function resolvedEdgeIds(graph) {
  return graph.edges.filter(e => e.target != null).map(e => e.id);
}

/** The id prefix of the clusters the overview adds for communities (`with_communities` in the engine). */
export const COMMUNITY_PREFIX = 'community:';

/** A short name for a node: the last part of a path-like id; for a community, `≈` and its place in its container. */
/**
 * The name of node `i` of an overview model: its label from the data (`overview.labels`), else `displayName` of its id.
 */
export function nameOf(model, i) {
  const own = j => model.overview.labels?.[j] || displayName(model.overview.ids[j]);
  // The top of a chain names the chain: its members' names in order, a name repeated only once.
  const names = [own(i)];
  for (let j = i; model.chainTop && model.children[j].length === 1 && model.chainTop[model.children[j][0]] === i;) {
    j = model.children[j][0];
    if (own(j) !== names.at(-1)) names.push(own(j));
  }
  return names.join(' › ');
}

/** The nodes of `i`'s chain from its top down (just `i` without one); see `chainTop` of the model. */
export function chainOf(model, i) {
  const top = model.chainTop ? model.chainTop[i] : i, out = [top];
  for (let j = top; model.chainTop && model.children[j].length === 1 && model.chainTop[model.children[j][0]] === top;) {
    j = model.children[j][0];
    out.push(j);
  }
  return out;
}

export function displayName(id) {
  if (id.startsWith(COMMUNITY_PREFIX)) return `≈ ${id.split(':').at(-1)}`;
  return id.split('/').at(-1);
}

/** The id prefix of the stand-ins a structured excerpt adds for the clusters outside (see `structuredExcerpt`). */
export const BORDER_PREFIX = 'border:';

/** A colour per top-level cluster: hues on the golden angle, so neighbours in input order differ. RGB in 0..1. */
export function clusterColour(rootOrdinal) {
  const h = (rootOrdinal * 137.508) % 360;
  return hsl(h, 0.62, 0.62);
}

function hsl(h, s, l) {
  const k = n => (n + h / 30) % 12, a = s * Math.min(l, 1 - l);
  const f = n => l - a * Math.max(-1, Math.min(k(n) - 3, Math.min(9 - k(n), 1)));
  return [f(0), f(8), f(4)];
}

/** Floats per vertex of a shape (`centreX, centreY, halfWidth, halfHeight, cornerX, cornerY, red, green, blue, alpha,
 * kind`; kind 0 a filled disc, 1 a ring, 2 a box with a border, 3 a box outline) and of a line or triangle vertex
 * (`x, y, red, green, blue, alpha`). */
export const SHAPE_STRIDE = 11;

/** The outlines a disc can be drawn with, inscribed in its circle so that the packing stays free of overlaps. */
export const OUTLINES = ['circle', 'square', 'octagon', 'diamond'];
export const LINE_STRIDE = 6;
const CORNERS = [[-1, -1], [1, -1], [1, 1], [-1, -1], [1, 1], [-1, 1]];

function pushShape(out, cx, cy, hx, hy, [red, green, blue], alpha, kind) {
  for (const [kx, ky] of CORNERS) out.push(cx, cy, hx, hy, kx, ky, red, green, blue, alpha, kind);
}

/** The part of a structured cluster's scene that its disc shows: the cluster's own box when the scene has it (an excerpt
 * from `structuredExcerpt`), else the whole scene. */
export function frameOf(model, i, scene) {
  const id = model.overview.ids[i];
  return scene.nodes.find(node => node.id === id)?.bounds ?? scene.bounds;
}

/**
 * Where a structured cluster's scene sits in its disc: its frame (`frameOf`), scaled so that the frame's diagonal fits
 * the disc (less a margin) and centred on it. `apply([x, y])` maps a scene point to the world, `invert` back.
 */
export function placeScene(model, i, scene, margin = 0.94) {
  const o = model.overview, b = frameOf(model, i, scene);
  const scale = (2 * o.r[i] * margin) / Math.max(Math.hypot(b.width, b.height), 1e-6);
  const sx = b.x + b.width / 2, sy = b.y + b.height / 2;
  return {
    scale,
    apply: ([x, y]) => [o.x[i] + (x - sx) * scale, o.y[i] + (y - sy) * scale],
    invert: ([x, y]) => [sx + (x - o.x[i]) / scale, sy + (y - o.y[i]) / scale],
  };
}

/**
 * Provisional (ADR 0025 O4): the scene view's view (`{centreX, centreY, scale}` in the scene's coordinates and CSS pixels
 * per unit) that shows a structured cluster's scene exactly where the overview's view `view` shows it in its disc, and
 * back (`overviewViewOf`). A hand-over between the two moves nothing.
 */
export function sceneViewOf(model, i, scene, view) {
  const place = placeScene(model, i, scene), [centreX, centreY] = place.invert([view.centreX, view.centreY]);
  return {centreX, centreY, scale: view.scale * place.scale};
}
/** The overview's view that shows the cluster's scene where the scene view's `view` shows it (see `sceneViewOf`). */
export function overviewViewOf(model, i, scene, view) {
  const place = placeScene(model, i, scene), [centreX, centreY] = place.apply([view.centreX, view.centreY]);
  return {centreX, centreY, scale: view.scale / place.scale};
}

/**
 * For every node, the outermost structured (`expanded`, index → scene) or collapsed (`collapsed`, a set of indices)
 * cluster it lies in, itself excluded, or -1: the node is drawn by that cluster, not as a disc of its own.
 */
export function ownersOf(model, expanded, collapsed = new Set()) {
  const owner = new Int32Array(model.n).fill(-1);
  if (!expanded?.size && !collapsed.size) return owner;
  const parent = model.overview.parent;
  for (let i = 0; i < model.n; i++) {
    for (let p = parent[i]; p >= 0; p = parent[p]) if (expanded?.has(p) || collapsed.has(p)) owner[i] = p;
  }
  return owner;
}

/** The representations a policy can choose for a container (ADR 0018, addendum). */
export const REPRESENTATIONS = ['collapsed', 'organic', 'structured'];

/**
 * The representation of every container for a view (ADR 0018, addendum), top-down; a collapsed or structured container
 * decides for everything below it. For each container, in this order:
 *
 * 1. `decisions` (node id → representation): what the user chose. A structured choice needs at most `limit` nodes below.
 * 2. `rules` (node kind → representation, using the overview's `kinds`): rules of the data. A rule's `structured` applies
 *    once the cluster is at least `rulePixels` in radius on screen, and needs at most `limit` nodes below.
 * 3. With `auto`: collapsed below `collapsePixels` in radius on screen; structured from `structureShare` of the smaller
 *    side of the viewport when it holds at most `limit` nodes and its disc holds the `focus` (a world point: where the
 *    user zooms; default the view's centre), so that one cluster, the one zoomed into, switches; a cluster in `current`
 *    (shown structured now) stays structured down to half that size while it is in view, so that a small zoom does not
 *    switch it back and forth. Else organic.
 *
 * Returns `{collapsed, structured}`, sets of node indices.
 */
export function representationsOf(model, view, width, height, {
  decisions = new Map(), rules = {}, auto = true, limit = 300, current = new Set(), focus = null,
  collapsePixels = 14, structureShare = 0.35, rulePixels = 80, levelOfDetail = null,
} = {}) {
  const o = model.overview, collapsed = new Set(), structured = new Set();
  const screenRadius = i => o.r[i] * view.scale;
  const inView = i => {
    const x = width / 2 + (o.x[i] - view.centreX) * view.scale, y = height / 2 + (o.y[i] - view.centreY) * view.scale;
    const r = screenRadius(i);
    return x + r > 0 && y + r > 0 && x - r < width && y - r < height;
  };
  const structurePixels = structureShare * Math.min(width, height);
  const fits = i => model.size[i] <= limit;
  const [fx, fy] = focus ?? [view.centreX, view.centreY];
  const holdsFocus = i => (o.x[i] - fx) ** 2 + (o.y[i] - fy) ** 2 <= o.r[i] ** 2;
  const choose = i => {
    const decided = decisions.get(o.ids[i]);
    if (decided === 'collapsed' || decided === 'organic') return decided;
    if (decided === 'structured' && fits(i)) return 'structured';
    // A chain follows the first rule of its members' kinds, from its top down.
    const rule = o.kinds && o.kind ? chainOf(model, i).map(j => rules[o.kinds[o.kind[j]]]).find(r => r) : undefined;
    if (rule === 'collapsed' || rule === 'organic') return rule;
    if (rule === 'structured' && fits(i) && screenRadius(i) >= rulePixels && inView(i)) return 'structured';
    // The level of detail: clusters from its depth on, and of its kinds, are collapsed until the user opens them.
    if (levelOfDetail) {
      const {depth = null, kinds = []} = levelOfDetail;
      if (depth !== null && model.depth[i] >= depth) return 'collapsed';
      if (kinds.length && o.kinds && o.kind && chainOf(model, i).some(j => kinds.includes(o.kinds[o.kind[j]]))) return 'collapsed';
    }
    if (!auto) return 'organic';
    const r = screenRadius(i);
    if (r < collapsePixels) return 'collapsed';
    if (fits(i) && inView(i) && (current.has(i) ? r >= structurePixels / 2 : r >= structurePixels && holdsFocus(i))) {
      return 'structured';
    }
    return 'organic';
  };
  const stack = [...model.roots];
  while (stack.length) {
    const i = stack.pop();
    if (!model.children[i].length) continue;
    // A node inside a chain is shown as its top is: open, since the walk reached it.
    if (model.chainTop && model.chainTop[i] !== i) { stack.push(...model.children[i]); continue; }
    const representation = choose(i);
    if (representation === 'collapsed') collapsed.add(i);
    else if (representation === 'structured') structured.add(i);
    else stack.push(...model.children[i]);
  }
  return {collapsed, structured};
}

/**
 * The vertex data of a model, with the clusters of `expanded` (index → {scene}) drawn structured in their discs:
 * `discs` (containers outer to inner, then leaves; nothing inside a structured cluster), `edges` (overview edges as
 * lines; an edge with an end inside a structured cluster runs from that end's box, and edges inside one are left to its
 * scene), and per structured cluster its `back` boxes (containers), `routes` (lines), `arrows` (triangles at each route's
 * target, so the logical direction shows) and `front` boxes (leaves).
 */
export function overviewBuffers(model, expanded = new Map(), collapsed = new Set(), selected = -1, {shapes = {}, trace = null, galaxy = false, style = null, structured = 'cluster'} = {}) {
  const {overview: o, n, children, depth, cluster, tops} = model;
  // The appearance (style.js, ADR 0026): with a style that is not the default, the trace, the routes inside structured
  // clusters, the lines between clusters and the buses take its theme's colours, and the clusters its palette when it
  // brings one; without one the overview's own colours.
  const look = style ? createStyle(style) : null, themed = !!look && !look.isDefault, rgb = c => parseColour(c).slice(0, 3);
  // A path (`pathOf`): its discs and lines are drawn in amber; with `fade` everything else is dimmed.
  const AMBER = themed ? rgb(look.theme.trace) : [1, 0.78, 0.38];
  const NEUTRAL = themed ? rgb(look.theme.row) : [0.62, 0.7, 0.82], ROUTE = themed ? rgb(look.theme.route) : [0.6, 0.85, 0.75];
  const onPath = i => !!trace && (trace.nodes.has(i) || trace.clusters.has(i));
  const dim = (rgb, i) => (trace?.fade && !onPath(i) ? rgb.map(v => v * 0.35) : rgb);
  // The outline of a node by its kind, as a tenth added to the shape kind (the shader reads it back).
  const outlineCode = OUTLINES.map((_, k) => k / 10);
  const outline = i => {
    const k = o.kinds && o.kind ? OUTLINES.indexOf(shapes[o.kinds[o.kind[i]]]) : -1;
    return k > 0 ? outlineCode[k] : 0;
  };
  const ordinal = new Map(tops.map((r, k) => [r, k]));
  const colour = look?.hasOwnPalette ? i => rgb(look.palette[ordinal.get(cluster[i]) % look.palette.length]) : i => clusterColour(ordinal.get(cluster[i]));
  const owner = ownersOf(model, expanded, collapsed);
  // A container inside a chain shares its top's disc: only the top's ring is drawn.
  const folded = i => model.chainTop && model.chainTop[i] !== i && children[i].length > 0;
  const order = [...Array(n).keys()].filter(i => owner[i] < 0 && !folded(i)).sort((a, b) =>
    (children[b].length > 0) - (children[a].length > 0) || depth[a] - depth[b] || a - b);
  // Provisional: a bus of shape busLine that stands on its own (not inside a collapsed or structured cluster) is drawn as
  // its line with taps (`busLines`), not as a disc; its edges are its taps.
  const lines = new Map((o.busLines ?? []).filter(l => owner[l.node] < 0).map(l => [l.node, l]));
  const discs = [];
  for (const i of order) {
    if (lines.has(i)) continue;
    if (galaxy) {
      // A cluster is a cloud of light (a glow fainter the deeper it is, so nested clouds add up towards their cores),
      // a collapsed one a brighter glow, a leaf a star; the colours of the clusters as in the disc style.
      const [red, green, blue] = dim(colour(i), i);
      if (children[i].length > 0 && !collapsed.has(i)) pushShape(discs, o.x[i], o.y[i], o.r[i] * 1.15, o.r[i] * 1.15, [red, green, blue], 0.22 / (1 + depth[i]), 4);
      else if (collapsed.has(i)) pushShape(discs, o.x[i], o.y[i], o.r[i] * 1.3, o.r[i] * 1.3, [red, green, blue], 0.45, 4);
      else pushShape(discs, o.x[i], o.y[i], o.r[i] * 1.6, o.r[i] * 1.6, [Math.min(1, red * 1.25 + 0.1), Math.min(1, green * 1.25 + 0.1), Math.min(1, blue * 1.25 + 0.1)], 0.95, 5);
      continue;
    }
    // A collapsed cluster is one filled disc, a little darker than a leaf.
    const ring = children[i].length > 0 && !collapsed.has(i);
    const shade = ring ? 0.35 + 0.1 * Math.min(depth[i], 4) : collapsed.has(i) ? 0.72 : 1;
    const [red, green, blue] = dim(colour(i), i);
    pushShape(discs, o.x[i], o.y[i], o.r[i], o.r[i], [red * shade, green * shade, blue * shade], collapsed.has(i) ? 0.9 : 1, (ring ? 1 : 0) + outline(i));
  }
  // The path's nodes as amber rings on the disc that shows them now (themselves, or the collapsed cluster that holds them).
  if (trace) {
    const ringed = new Set();
    for (const i of trace.nodes) {
      const shown = owner[i] >= 0 && !expanded.has(owner[i]) ? owner[i] : i;
      if (owner[shown] >= 0 || ringed.has(shown)) continue;
      ringed.add(shown);
      pushShape(discs, o.x[shown], o.y[shown], o.r[shown], o.r[shown], AMBER, 1, 1 + outline(shown));
    }
  }
  // The selected cluster: a bright ring on top of its disc.
  if (selected >= 0 && owner[selected] < 0 && !lines.has(selected)) pushShape(discs, o.x[selected], o.y[selected], o.r[selected], o.r[selected], [1, 1, 1], 1, 1 + outline(selected));
  // Scene geometry per structured cluster, where each of its nodes' boxes is, and the border port of each edge that
  // leaves it.
  const back = [], routes = [], arrows = [], front = [];
  const boxCentre = new Map(), ports = new Map();
  for (const [i, {scene}] of expanded) {
    const place = placeScene(model, i, scene), c = colour(i), frame = frameOf(model, i, scene);
    const shown = node => node.id !== o.ids[i] && !node.id.startsWith(BORDER_PREFIX);
    // `structured: 'scene'` (ADR 0025 O3): the cluster's content as the scene view draws it, from `sceneBuffers` with
    // the style and the trace, placed into the disc; else boxes in the cluster's colour.
    const asScene = structured === 'scene', clippedRoutes = [], outwardIds = new Set();
    for (const node of scene.nodes) {
      if (!shown(node)) continue;
      const b = node.bounds;
      const [cx, cy] = place.apply([b.x + b.width / 2, b.y + b.height / 2]);
      const hx = (b.width / 2) * place.scale, hy = (b.height / 2) * place.scale;
      boxCentre.set(node.id, [cx, cy]);
      if (asScene) continue;
      const lit = trace?.nodeIds.has(node.id);
      if (node.container) pushShape(back, cx, cy, hx, hy, c.map(v => v * 0.5), 0.12, 3);
      else pushShape(front, cx, cy, hx, hy, lit ? AMBER : trace?.fade ? c.map(v => v * 0.35) : c, 1, 2);
    }
    for (const route of scene.routes) {
      const outward = route.target.startsWith(BORDER_PREFIX), inward = route.source.startsWith(BORDER_PREFIX);
      let inner = route.points.map(p => [p.x, p.y]);
      if (outward || inward) {
        const clipped = clipToFrame(inner, frame, inward);
        if (!clipped) continue;
        inner = clipped.points;
        // Keyed by cluster and edge: an edge between two structured clusters has a port in each.
        for (const id of new Set([route.id, ...(route.members ?? [])])) ports.set(`${i}:${id}`, place.apply(clipped.port));
      }
      if (asScene) {
        clippedRoutes.push({...route, points: inner.map(([x, y]) => ({x, y}))});
        if (outward) outwardIds.add(route.id);
        continue;
      }
      const points = inner.map(place.apply);
      const litRoute = trace && [route.id, ...(route.members ?? [])].some(id => trace.edgeIds.has(id));
      const [rr, rg, rb, ra] = litRoute ? [...AMBER, 1] : [...ROUTE, trace?.fade ? 0.25 : 0.9];
      for (let k = 1; k < points.length; k++) routes.push(...points[k - 1], rr, rg, rb, ra, ...points[k], rr, rg, rb, ra);
      if (points.length >= 2 && !outward) arrowHead(arrows, points.at(-2), points.at(-1), Math.max(4 * place.scale, 0.6));
    }
    if (asScene) {
      const excerpt = {...scene, schemaVersion: scene.schemaVersion ?? 1, nodes: scene.nodes.filter(shown), routes: clippedRoutes, ports: [], labels: [], crossings: [], membershipHulls: undefined};
      const sceneTrace = trace ? {nodes: [...trace.nodeIds], routes: [...trace.edgeIds], fade: !!trace.fade} : null;
      sceneInDisc(sceneBuffers(excerpt, {style, trace: sceneTrace, noArrow: outwardIds}), place, {back, front, routes, arrows});
    }
  }
  const edges = [];
  // Edges between collapsed clusters are drawn once per pair and direction, stronger the more they stand for.
  const bundled = new Map(), litBundles = new Set();
  for (let k = 0; k < o.edges.length / 2; k++) {
    const a = o.edges[2 * k], b = o.edges[2 * k + 1];
    if (lines.has(a) || lines.has(b)) continue;
    if (owner[a] >= 0 && owner[a] === owner[b]) continue;
    const ra = owner[a] >= 0 && collapsed.has(owner[a]) ? owner[a] : a;
    const rb = owner[b] >= 0 && collapsed.has(owner[b]) ? owner[b] : b;
    if ((ra !== a || rb !== b) && !(owner[a] >= 0 && expanded.has(owner[a])) && !(owner[b] >= 0 && expanded.has(owner[b]))) {
      const key = `${ra}>${rb}`;
      bundled.set(key, (bundled.get(key) ?? 0) + 1);
      if (trace?.edges.has(k)) litBundles.add(key);
      continue;
    }
    // An end inside a structured cluster is its border port there (or, without one, its box); in a collapsed cluster,
    // that cluster's centre.
    const end = i => {
      if (owner[i] < 0) return [o.x[i], o.y[i]];
      if (collapsed.has(owner[i])) return [o.x[owner[i]], o.y[owner[i]]];
      const port = model.edgeIds && ports.get(`${owner[i]}:${model.edgeIds[k]}`);
      return port ?? boxCentre.get(o.ids[i]) ?? [o.x[owner[i]], o.y[owner[i]]];
    };
    const from = end(a), to = end(b);
    // In the galaxy style every edge takes its source's colour: where a cluster's edges run, its light runs.
    const [red, green, blue] = galaxy || cluster[a] === cluster[b] ? colour(a) : NEUTRAL;
    // An edge that leaves a structured cluster is drawn stronger: it is one of few. It ends in an arrow at a disc outside.
    const leg = owner[a] >= 0 || owner[b] >= 0;
    const lit = trace?.edges.has(k);
    const alpha = lit ? 0.95 : (leg ? 0.3 : galaxy ? 0.05 : 0.06) * (trace?.fade ? 0.3 : 1);
    // In the galaxy style the lines are a darker shade of their cluster's colour: faint threads, not light.
    const [er, eg, eb] = lit ? AMBER : galaxy && !leg ? [red * 0.7, green * 0.7, blue * 0.7] : [red, green, blue];
    edges.push(...from, er, eg, eb, alpha, ...to, er, eg, eb, alpha);
    if (leg && owner[a] >= 0 && expanded.has(owner[a]) && !(owner[b] >= 0 && expanded.has(owner[b]))) {
      const target = owner[b] >= 0 ? owner[b] : b;
      const l = Math.hypot(to[0] - from[0], to[1] - from[1]);
      // An end inside the target's disc gets an empty arrow (three vertices all the same).
      const tip = l > o.r[target] ? [to[0] - (to[0] - from[0]) / l * o.r[target], to[1] - (to[1] - from[1]) / l * o.r[target]] : from;
      arrowHead(arrows, from, tip, Math.min(o.r[target] * 1.5, l * 0.2));
    }
  }
  for (const [key, count] of bundled) {
    const [ra, rb] = key.split('>').map(Number);
    if (ra === rb) continue;
    const lit = litBundles.has(key);
    const [red, green, blue] = lit ? AMBER : cluster[ra] === cluster[rb] ? colour(ra) : NEUTRAL;
    const alpha = lit ? 0.95 : Math.min(0.55, 0.06 + 0.04 * Math.sqrt(count)) * (trace?.fade ? 0.3 : 1);
    edges.push(o.x[ra], o.y[ra], red, green, blue, alpha, o.x[rb], o.y[rb], red, green, blue, alpha);
  }
  // The bus lines as thick orthogonal connectors: the line, and from each member (or the cluster that shows it) a tap that
  // leaves its disc at the rim (the stub), runs at a right angle to the line and, for a member beyond the line's ends,
  // along it to the end; a square junction where it meets the line. All as triangles, so they keep their width.
  const BUS = themed ? rgb(look.theme.bus) : [0.44, 0.58, 0.72], buses = [];
  for (const [i, l] of lines) {
    const lit = !!trace && trace.nodes.has(i);
    const [red, green, blue] = i === selected ? [1, 1, 1] : lit ? AMBER : dim(BUS, i);
    const half = Math.max(o.r[i] * 0.12, 0.25), across = l.y1 === l.y2;
    thickSegment(buses, [l.x1, l.y1], [l.x2, l.y2], half, [red, green, blue], 1);
    l.taps.forEach((m, k) => {
      const shown = owner[m] >= 0 && collapsed.has(owner[m]) ? owner[m] : m;
      const centre = owner[m] >= 0 && !collapsed.has(owner[m]) ? boxCentre.get(o.ids[m]) ?? [o.x[owner[m]], o.y[owner[m]]] : [o.x[shown], o.y[shown]];
      const rim = owner[m] >= 0 && !collapsed.has(owner[m]) ? 0 : o.r[shown];
      const foot = [l.feet[2 * k], l.feet[2 * k + 1]];
      // Where the tap meets the line's level: straight below or beside the member; the foot when the member is within
      // the line's span, else a corner from which the tap runs along the line to its end.
      const corner = across ? [centre[0], l.y1] : [l.x1, centre[1]];
      const away = Math.hypot(corner[0] - centre[0], corner[1] - centre[1]);
      const start = away > rim ? [centre[0] + (corner[0] - centre[0]) / away * rim, centre[1] + (corner[1] - centre[1]) / away * rim] : corner;
      const tapLit = lit && (trace.nodes.has(m) || trace.clusters.has(shown));
      const [tr, tg, tb] = tapLit ? AMBER : i === selected ? [1, 1, 1] : dim(BUS, i);
      const alpha = tapLit || i === selected ? 1 : trace?.fade ? 0.35 : 0.9;
      if (away > rim) thickSegment(buses, start, corner, half * 0.8, [tr, tg, tb], alpha);
      if (Math.hypot(foot[0] - corner[0], foot[1] - corner[1]) > 1e-6) thickSegment(buses, corner, foot, half * 0.8, [tr, tg, tb], alpha);
      thickSegment(buses, [foot[0] - half * 1.6, foot[1]], [foot[0] + half * 1.6, foot[1]], half * 1.6, [red, green, blue], 1);
    });
  }
  const f = a => new Float32Array(a);
  return {
    discs: f(discs), edges: f(edges), buses: f(buses), back: f(back), routes: f(routes), arrows: f(arrows), front: f(front),
    count: {
      discs: discs.length / SHAPE_STRIDE, edges: edges.length / LINE_STRIDE, buses: buses.length / LINE_STRIDE, back: back.length / SHAPE_STRIDE,
      routes: routes.length / LINE_STRIDE, arrows: arrows.length / LINE_STRIDE, front: front.length / SHAPE_STRIDE,
    },
  };
}

/**
 * The part of a route (scene points) inside the frame, and its border port: where the route crosses the frame's border.
 * `inward`: the route comes from outside (its source is a stand-in), so the part runs from the port to the end; otherwise
 * from the start to the port. Null when the route does not cross the border.
 */
export function clipToFrame(points, frame, inward) {
  const eps = 1e-6;
  const inside = ([x, y]) => x >= frame.x - eps && x <= frame.x + frame.width + eps && y >= frame.y - eps && y <= frame.y + frame.height + eps;
  const cross = (p, q) => {
    // The point where p (inside) → q (outside) leaves the frame: the smallest step along the segment that reaches a side.
    let t = 1;
    const d = [q[0] - p[0], q[1] - p[1]];
    for (const [axis, lo, hi] of [[0, frame.x, frame.x + frame.width], [1, frame.y, frame.y + frame.height]]) {
      if (d[axis] > 0) t = Math.min(t, (hi - p[axis]) / d[axis]);
      if (d[axis] < 0) t = Math.min(t, (lo - p[axis]) / d[axis]);
    }
    t = Math.max(0, t);
    return [p[0] + d[0] * t, p[1] + d[1] * t];
  };
  if (inward) {
    for (let k = points.length - 1; k > 0; k--) {
      if (inside(points[k]) && !inside(points[k - 1])) {
        const port = cross(points[k], points[k - 1]);
        return {port, points: [port, ...points.slice(k)]};
      }
    }
  } else {
    for (let k = 0; k + 1 < points.length; k++) {
      if (inside(points[k]) && !inside(points[k + 1])) {
        const port = cross(points[k], points[k + 1]);
        return {port, points: [...points.slice(0, k + 1), port]};
      }
    }
  }
  return null;
}

/** A filled triangle at `to`, pointing along `from` → `to`. */
// The scene view's buffers of a structured cluster (`sceneBuffers`) as the overview's parts, placed into the disc: frames
// to `back`, leaves, bus bars, junctions and marks to `front` (a fill and an outline each: a box of kind 3, or a disc and
// a ring for a circle), route and row segments to `routes` (one-pixel lines), arrowheads to `arrows`. Hulls and texts
// stay out (the overview names what it shows itself).
function sceneInDisc(buffers, place, parts) {
  const boxes = (data, out) => {
    for (let k = 0; k + BOX_STRIDE <= data.length; k += BOX_STRIDE) {
      const [x, y, w, h, , kind, strokeWidth] = data.subarray(k, k + 7);
      const fill = data.subarray(k + 8, k + 12), stroke = data.subarray(k + 12, k + 16);
      const [cx, cy] = place.apply([x + w / 2, y + h / 2]), hx = (w / 2) * place.scale, hy = (h / 2) * place.scale;
      const round = kind > 0.5;
      if (fill[3] > 0) pushShape(out, cx, cy, hx, hy, [fill[0], fill[1], fill[2]], fill[3], round ? 0 : 3);
      if (stroke[3] > 0 && strokeWidth > 0) pushShape(out, cx, cy, hx, hy, [stroke[0], stroke[1], stroke[2]], round ? stroke[3] : 0, round ? 1 : 3);
    }
  };
  const segments = (data, out) => {
    for (let k = 0; k + SEGMENT_STRIDE <= data.length; k += SEGMENT_STRIDE) {
      const a = place.apply([data[k], data[k + 1]]), b = place.apply([data[k + 2], data[k + 3]]), c = data.subarray(k + 9, k + 13);
      out.push(...a, c[0], c[1], c[2], c[3], ...b, c[0], c[1], c[2], c[3]);
    }
  };
  const triangles = (data, out) => {
    for (let k = 0; k + TRIANGLE_STRIDE <= data.length; k += TRIANGLE_STRIDE) out.push(...place.apply([data[k], data[k + 1]]), data[k + 3], data[k + 4], data[k + 5], data[k + 6]);
  };
  const [, containers, routeLayer, leaves, marks] = buffers.layers;
  boxes(containers.boxes, parts.back);
  segments(containers.segments, parts.routes);
  segments(routeLayer.segments, parts.routes);
  triangles(routeLayer.triangles, parts.arrows);
  boxes(routeLayer.boxes, parts.front);
  boxes(leaves.boxes, parts.front);
  segments(leaves.segments, parts.routes);
  boxes(marks.boxes, parts.front);
}

function arrowHead(out, from, to, size) {
  const dx = to[0] - from[0], dy = to[1] - from[1], l = Math.hypot(dx, dy);
  // Always three vertices, so that two states of one drawing have the same vertices to morph between.
  if (l === 0) { for (let k = 0; k < 3; k++) out.push(to[0], to[1], 0.6, 0.85, 0.75, 0); return; }
  const ux = dx / l, uy = dy / l, bx = to[0] - ux * size, by = to[1] - uy * size;
  const w = size * 0.5;
  for (const [x, y] of [to, [bx - uy * w, by + ux * w], [bx + uy * w, by - ux * w]]) out.push(x, y, 0.6, 0.85, 0.75, 0.95);
}

/**
 * The scene node under the world point inside a structured cluster: `{cluster, node}` (the node's scene record; the
 * smallest box holding the point), `{cluster, node: null}` inside the disc but on no box, or null outside every
 * structured cluster.
 */
export function pickExpanded(model, expanded, x, y) {
  const o = model.overview;
  for (const [i, {scene}] of expanded) {
    if ((o.x[i] - x) ** 2 + (o.y[i] - y) ** 2 > o.r[i] ** 2) continue;
    const [sx, sy] = placeScene(model, i, scene).invert([x, y]);
    let best = null;
    for (const node of scene.nodes) {
      if (node.id === o.ids[i] || node.id.startsWith(BORDER_PREFIX)) continue;
      const b = node.bounds;
      if (sx < b.x || sx > b.x + b.width || sy < b.y || sy > b.y + b.height) continue;
      if (!best || b.width * b.height < best.bounds.width * best.bounds.height) best = node;
    }
    return {cluster: i, node: best};
  }
  return null;
}

/**
 * A fisheye lens (Sarkar and Brown) in screen space: `{x, y, radius, magnification}` in CSS pixels. Inside `radius` around
 * (`x`, `y`) a point at the fraction r of the radius moves to M·r / ((M−1)·r + 1), M the magnification: the middle is
 * enlarged M times, the enlargement falls to 1 at the rim, and nothing outside moves. The disc of the lens maps onto
 * itself, so nothing is pushed out of it. Returns the point's place under the lens and the local enlargement
 * `[x, y, factor]`; without a lens (null, radius 0, magnification ≤ 1) the point itself and 1.
 */
export function lensAt(lens, x, y) {
  if (!lens || !(lens.radius > 0) || !(lens.magnification > 1)) return [x, y, 1];
  const dx = x - lens.x, dy = y - lens.y, r = Math.hypot(dx, dy) / lens.radius;
  if (r >= 1) return [x, y, 1];
  const m = lens.magnification, factor = m / ((m - 1) * r + 1);
  return [lens.x + dx * factor, lens.y + dy * factor, factor];
}

/** The screen point that `lensAt` moves to (`x`, `y`): its inverse, r = r′ / (M − (M−1)·r′). */
export function lensInverse(lens, x, y) {
  if (!lens || !(lens.radius > 0) || !(lens.magnification > 1)) return [x, y];
  const dx = x - lens.x, dy = y - lens.y, shown = Math.hypot(dx, dy) / lens.radius;
  if (shown >= 1 || shown === 0) return [x, y];
  const m = lens.magnification, r = shown / (m - (m - 1) * shown);
  return [lens.x + dx * r / shown, lens.y + dy * r / shown];
}

/**
 * The texts to write over the drawing for a view (`{centreX, centreY, scale}`) of `width` × `height` CSS pixels: the
 * names of clusters large enough on screen (at the top of their disc) and the labels of structured clusters' scenes when
 * their text is at least `minPixels` high. Positions in CSS pixels; at most `limit` texts, largest first, and none that
 * would overlap a larger one.
 */
export function labelsOf(model, expanded, view, width, height, {minPixels = 7, limit = 300, collapsed = new Set(), lens = null, galaxy = false, orbit = null, names = false, details = false} = {}) {
  const o = model.overview, out = [];
  // A screen point and the lens's enlargement there (`[x, y, factor]`); sizes on screen are multiplied by the factor, so
  // that the lens also shows the texts of what it enlarges.
  // With `orbit` (pseudo-3D) through its projection; a size there grows with the perspective's enlargement.
  const extent = o.rootRadius || 1;
  const screen = (x, y) => {
    const [px, py, f] = orbitPoint(orbit, view, extent, x, y);
    const [sx, sy, g] = lensAt(lens, width / 2 + px * view.scale, height / 2 + py * view.scale);
    return [sx, sy, g * f];
  };
  const visible = (x, y, pad) => x > -pad && y > -pad && x < width + pad && y < height + pad;
  const owner = ownersOf(model, expanded, collapsed);
  for (let i = 0; i < model.n; i++) {
    if (!model.children[i].length || owner[i] >= 0) continue;
    if (model.chainTop && model.chainTop[i] !== i) continue;
    const [cx, cy, factor] = screen(o.x[i], o.y[i]);
    const r = o.r[i] * view.scale * factor, text = nameOf(model, i);
    if (collapsed.has(i)) {
      // A collapsed cluster carries its name in its middle.
      if (r < 16) continue;
      if (visible(cx, cy, r)) out.push({text, x: cx, y: cy, size: Math.min(10 + r / 12, 15), weight: r, kind: 'cluster'});
      continue;
    }
    if (r < 36) continue;
    if (galaxy) {
      // A galaxy carries its name in its middle, with its colour (drawn as a dot on a dark tag). A single root around
      // everything is the sky, not a galaxy: its name would sit on the one in the middle.
      if (model.roots.length === 1 && i === model.roots[0]) continue;
      if (!visible(cx, cy, r)) continue;
      const ordinal = model.tops.indexOf(model.cluster[i]);
      out.push({text, x: cx, y: cy, size: Math.min(12 + r / 80, 15), weight: r, kind: 'cluster', colour: clusterColour(ordinal)});
      continue;
    }
    const [x, y] = screen(o.x[i], o.y[i] - o.r[i]);
    if (!visible(x, y + r, r)) continue;
    out.push({text, x, y: y + 12, size: Math.min(13 + r / 60, 18), weight: r, kind: 'cluster'});
  }
  // A bus drawn as a line carries its name above the middle of its bar, once the bar is long enough on screen.
  for (const [i, l] of busLinesOf(model)) {
    if (owner[i] >= 0) continue;
    const [x1, y1, f1] = screen(l.x1, l.y1), [x2, y2] = screen(l.x2, l.y2);
    const long = Math.hypot(x2 - x1, y2 - y1);
    if (long < 40 * f1) continue;
    const [x, y] = [(x1 + x2) / 2, (y1 + y2) / 2];
    if (!visible(x, y, long)) continue;
    out.push({text: nameOf(model, i), x, y: y - 9, size: Math.min(11 + long / 120, 14), weight: long / 2, kind: 'node'});
  }
  // Provisional (`names`): a node's name beside it once its disc is 6 pixels across or more on the screen (zoomed in, or
  // near in the orbit); with `details` also its kind and how many edges it has.
  if (names) {
    const degree = degreesOf(model);
    for (let i = 0; i < model.n; i++) {
      if (model.children[i].length || owner[i] >= 0) continue;
      const [cx, cy, factor] = screen(o.x[i], o.y[i]);
      const r = o.r[i] * view.scale * factor;
      if (r < 6 || !visible(cx, cy, r)) continue;
      const kind = o.kinds && o.kind ? o.kinds[o.kind[i]] : null;
      const text = nameOf(model, i) + (details ? [kind, degree[i] ? `${degree[i]} link${degree[i] === 1 ? '' : 's'}` : null].filter(Boolean).map(s => ` · ${s}`).join('') : '');
      const size = Math.min(9 + r / 3, 13);
      // Beside the disc on its right (the text is centred on `x`).
      out.push({text, x: cx + r + 4 + text.length * size * 0.275, y: cy, size, weight: r, kind: 'node'});
    }
  }
  for (const [i, {scene}] of expanded) {
    const place = placeScene(model, i, scene);
    const placed = new Set((scene.labels ?? []).map(l => l.id));
    // A node's name in its box (as the SVG viewer writes it), unless the scene placed the node's label itself.
    for (const node of scene.nodes) {
      if (placed.has(node.id) || node.id === o.ids[i] || node.id.startsWith(BORDER_PREFIX)) continue;
      const b = node.bounds;
      const [wx, wy] = place.apply(node.container ? [b.x + b.width / 2, b.y + 9] : [b.x + b.width / 2, b.y + b.height / 2]);
      const [x, y, factor] = screen(wx, wy);
      const high = Math.min(b.height, 24) * place.scale * view.scale * factor;
      const wide = b.width * place.scale * view.scale * factor;
      if (high < minPixels * 2.2 || wide < 24) continue;
      if (!visible(x, y, wide)) continue;
      const size = Math.min(high * 0.42, 14);
      // Cut to what fits the box, at about 0.55 em per character.
      const fit = Math.max(1, Math.floor((wide - 6) / (size * 0.55)));
      const text = node.label || node.id;
      out.push({text: text.length > fit ? text.slice(0, Math.max(fit - 1, 1)) + '…' : text, x, y, size, weight: high, kind: 'node'});
    }
    for (const label of scene.labels ?? []) {
      const b = label.bounds;
      const [wx, wy] = place.apply([b.x + b.width / 2, b.y + b.height / 2]);
      const [x, y, factor] = screen(wx, wy);
      const pixels = b.height * place.scale * view.scale * factor;
      if (pixels < minPixels) continue;
      if (!visible(x, y, 50)) continue;
      out.push({text: label.text, x, y, size: Math.min(pixels * 0.8, 16), weight: pixels, kind: label.kind});
    }
  }
  // Largest first; a text that would cover one already placed is left out (its box: about 0.55 em per character).
  out.sort((a, b) => b.weight - a.weight);
  const placed = [], kept = [];
  for (const label of out) {
    if (kept.length >= limit) break;
    // A galaxy's tag also holds its dot and margins.
    const w = label.text.length * label.size * 0.55 + (label.colour ? label.size * 2.6 : 4), h = label.size * (label.colour ? 1.9 : 1) + 2;
    const box = [label.x - w / 2, label.y - h / 2, label.x + w / 2, label.y + h / 2];
    if (placed.some(b => box[0] < b[2] && b[0] < box[2] && box[1] < b[3] && b[1] < box[3])) continue;
    placed.push(box);
    kept.push(label);
  }
  return kept;
}

/** The deepest node whose disc holds the world point (`x`, `y`), or -1. Descends from the top level. */
export function pickNode(model, x, y, collapsed = new Set()) {
  const {overview: o, children, roots} = model;
  // A bus drawn as a line (`busLines`) is picked by its bar (within the bar's width, at least a tenth of a leaf beyond
  // it), not by its disc; a disc under the point comes first.
  const lines = busLinesOf(model);
  const onBar = i => {
    const l = lines.get(i), dx = l.x2 - l.x1, dy = l.y2 - l.y1, len2 = dx * dx + dy * dy;
    const t = len2 > 0 ? Math.max(0, Math.min(1, ((x - l.x1) * dx + (y - l.y1) * dy) / len2)) : 0;
    const px = l.x1 + t * dx, py = l.y1 + t * dy, reach = Math.max(o.r[i] / 3, 0.5) * 1.5;
    return (x - px) ** 2 + (y - py) ** 2 <= reach * reach;
  };
  const inside = i => !lines.has(i) && (o.x[i] - x) ** 2 + (o.y[i] - y) ** 2 <= o.r[i] ** 2;
  let found = -1, candidates = roots;
  for (;;) {
    const next = candidates.find(inside) ?? candidates.find(i => lines.has(i) && onBar(i));
    if (next === undefined) return found >= 0 && model.chainTop ? model.chainTop[found] : found;
    found = next;
    // A collapsed cluster is picked as a whole.
    if (collapsed.has(next)) return found;
    candidates = children[next];
  }
}

// A segment from `a` to `b` as two triangles `half` wide on each side (line vertices: x, y, red, green, blue, alpha).
function thickSegment(out, a, b, half, [red, green, blue], alpha) {
  const dx = b[0] - a[0], dy = b[1] - a[1], len = Math.hypot(dx, dy) || 1;
  const nx = -dy / len * half, ny = dx / len * half;
  const quad = [[a[0] + nx, a[1] + ny], [b[0] + nx, b[1] + ny], [b[0] - nx, b[1] - ny], [a[0] - nx, a[1] - ny]];
  for (const k of [0, 1, 2, 0, 2, 3]) out.push(...quad[k], red, green, blue, alpha);
}

/**
 * Provisional (orbit, pseudo-3D): the depth of a world point, from a smooth field at the scale of the clusters (what lies
 * close together, a galaxy, lies at a similar depth) plus a little of its own (a cluster's thickness). In [-1.15, 1.15];
 * `extent` is the drawing's radius. The renderer's shaders compute the same.
 */
export function orbitDepth(x, y, extent) {
  const qx = x / Math.max(extent, 1) * 3, qy = y / Math.max(extent, 1) * 3;
  const field = Math.sin(qx * 1.7 + 0.3) * Math.cos(qy * 1.3 - 0.7) * 0.7 + Math.sin((qx + qy) * 2.3) * 0.3;
  const s = Math.sin(x * 12.9898 + y * 78.233) * 43758.5453;
  return field + ((s - Math.floor(s)) - 0.5) * 0.3;
}

/**
 * Provisional (orbit): where a world point is drawn, as `[x, y, factor]` relative to the view's centre in world units, with
 * the perspective's enlargement there: turned by `orbit.yaw` (about the vertical) and `orbit.pitch` (about the
 * horizontal) around the view's centre, its depth `orbitDepth * orbit.depth`, seen from three extents away.
 */
export function orbitPoint(orbit, view, extent, x, y) {
  const rx = x - view.centreX, ry = y - view.centreY;
  if (!orbit) return [rx, ry, 1];
  let px = rx, py = ry, pz = orbitDepth(x, y, extent) * orbit.depth;
  const cy = Math.cos(orbit.yaw), sy = Math.sin(orbit.yaw);
  [px, pz] = [cy * px + sy * pz, -sy * px + cy * pz];
  const cp = Math.cos(orbit.pitch), sp = Math.sin(orbit.pitch);
  [py, pz] = [cp * py - sp * pz, sp * py + cp * pz];
  const dist = Math.max(extent, 1) * 3, f = dist / Math.max(dist + pz, dist * 0.2);
  return [px * f, py * f, f];
}

/** How many edges every node has (either end), kept with the model. */
export function degreesOf(model) {
  if (!model.degree) {
    const d = new Int32Array(model.n), e = model.overview.edges;
    for (let k = 0; k < e.length; k++) d[e[k]]++;
    model.degree = d;
  }
  return model.degree;
}

/** The bus lines of a model by node index (`busLines` of the overview), kept with the model. */
export function busLinesOf(model) {
  if (!model.busLines) model.busLines = new Map((model.overview.busLines ?? []).map(l => [l.node, l]));
  return model.busLines;
}

/** The view (`centreX`, `centreY`, `scale` in pixels per unit) that shows the disc of node `i` (-1: everything). */
export function viewOf(model, i, width, height, margin = 0.92) {
  const o = model.overview;
  const [cx, cy, r] = i < 0 ? [0, 0, o.rootRadius] : [o.x[i], o.y[i], o.r[i]];
  return {centreX: cx, centreY: cy, scale: margin * Math.min(width, height) / (2 * Math.max(r, 1e-6))};
}

/** The ids of node `i` and every node below it, in input order. */
export function subtreeOf(model, i) {
  const out = [], stack = [i];
  while (stack.length) {
    const k = stack.pop();
    out.push(k);
    for (const c of model.children[k]) stack.push(c);
  }
  return out.sort((a, b) => a - b).map(k => model.overview.ids[k]);
}

/**
 * The flow direction for cluster `i` drawn structured, so that its border ports face its neighbours: the edges that
 * leave it pull towards their targets, the edges that enter it from their sources' side; the sum of these unit vectors
 * picks `lr`, `rl`, `tb` or `bt` (y grows downwards). Null when the pulls cancel out (less than a fifth of the edges), or
 * when no edge crosses the cluster's border; the layout's default applies then.
 */
export function flowDirectionOf(model, i) {
  const o = model.overview, parent = o.parent;
  const inside = x => { for (let p = x; p >= 0; p = parent[p]) if (p === i) return true; return false; };
  let vx = 0, vy = 0, count = 0;
  const pull = (x, sign) => {
    const dx = o.x[x] - o.x[i], dy = o.y[x] - o.y[i], l = Math.hypot(dx, dy);
    if (l > 0) { vx += sign * dx / l; vy += sign * dy / l; count++; }
  };
  for (let k = 0; k < o.edges.length / 2; k++) {
    const a = o.edges[2 * k], b = o.edges[2 * k + 1], ia = inside(a), ib = inside(b);
    if (ia && !ib) pull(b, 1);
    else if (ib && !ia) pull(a, -1);
  }
  if (!count || Math.hypot(vx, vy) < count / 5) return null;
  return Math.abs(vx) >= Math.abs(vy) ? (vx > 0 ? 'lr' : 'rl') : (vy > 0 ? 'tb' : 'bt');
}

/**
 * The excerpt a structured cluster lays out: cluster `id` itself (as a top-level container) with everything below it, the
 * edges between them, and for the edges that leave it a stand-in per cluster outside (`BORDER_PREFIX` + its id): the
 * outer end's ancestor that is a sibling of one of the cluster's ancestors (or the outer end itself). A leaving edge keeps
 * its id and direction and runs to or from its stand-in, so the layered layout routes it out of the cluster's box on the
 * side its flow uses; the crossing is its border port (ADR 0017). `direction` (see `flowDirectionOf`) sets that flow.
 */
export function structuredExcerpt(graph, model, id, {direction = null} = {}) {
  const c = model.index.get(id);
  if (c === undefined) throw Error(`Unknown node: ${id}`);
  const parent = model.overview.parent, ids = model.overview.ids;
  const inside = new Set(subtreeOf(model, c));
  // The nesting is the overview's: a community the overview added becomes a container of its own.
  const own = new Map(graph.nodes.map(n => [n.id, n]));
  const below = [];
  const stack = [c];
  while (stack.length) { const k = stack.pop(); below.push(k); stack.push(...model.children[k]); }
  below.sort((a, b) => a - b);
  const above = new Set();
  for (let p = parent[c]; p >= 0; p = parent[p]) above.add(p);
  const standIn = outer => {
    let x = model.index.get(outer);
    while (parent[x] >= 0 && !above.has(parent[x]) && !above.has(x)) x = parent[x];
    return BORDER_PREFIX + ids[x];
  };
  const nodes = below.map(k => {
    const base = own.get(ids[k]) ?? {id: ids[k], kind: 'community', label: displayName(ids[k])};
    return {...base, parentId: k === c ? null : ids[parent[k]]};
  });
  const standIns = new Map();
  const edges = [];
  let leaving = 0;
  for (const e of graph.edges) {
    if (e.target == null) continue;
    const s = inside.has(e.source), t = inside.has(e.target);
    if (s && t) { edges.push(e); continue; }
    if (!s && !t) continue;
    leaving++;
    const outer = s ? e.target : e.source, stand = standIn(outer);
    if (!standIns.has(stand)) standIns.set(stand, {id: stand, kind: 'border', label: displayName(stand.slice(BORDER_PREFIX.length)), parentId: null});
    edges.push(s ? {...e, target: stand} : {...e, source: stand});
  }
  const excerpt = {nodes: [...nodes, ...standIns.values()], edges, ...(direction ? {defaultDirection: direction} : {})};
  // The options to import the excerpt with: a file's unrelated functions stand in a grid, not in one long row.
  return {graph: excerpt, options: {isolatedGrid: true}, leaving, inner: edges.length - leaving, standIns: standIns.size};
}

/**
 * Provisional: an `overviewTrace` or `overviewNeighbourhood` answer as the model's indices, for `overviewBuffers`'
 * `trace`: the path's nodes, its edges (indices into `overview.edges`, through `edgeIds`), the clusters that hold the
 * nodes, and the ids of nodes and edges. `fade` dims everything else.
 */
export function pathOf(model, answer, {fade = true} = {}) {
  const nodes = new Set(), clusters = new Set();
  for (const id of answer.nodes ?? []) {
    const i = model.index.get(id);
    if (i === undefined) continue;
    nodes.add(i);
    for (let p = model.overview.parent[i]; p >= 0; p = model.overview.parent[p]) clusters.add(p);
  }
  const edgeIds = new Set(answer.edges ?? []), edges = new Set();
  (model.edgeIds ?? []).forEach((id, k) => { if (edgeIds.has(id)) edges.add(k); });
  return {nodes, edges, clusters, nodeIds: new Set(answer.nodes ?? []), edgeIds, fade};
}

/**
 * Provisional (reveal in the overview): the decisions that open every cluster holding a node of `path` down to it — each
 * such cluster `organic` (shown open) — as id → representation, to merge into the user's decisions.
 */
export function revealPathDecisions(model, path) {
  return new Map([...path.clusters].map(i => [model.overview.ids[i], 'organic']));
}

/** The view that shows the discs of the nodes `indices` whole (all of the overview when there are none). */
export function viewOfNodes(model, indices, width, height, margin = 0.85) {
  const o = model.overview;
  let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
  for (const i of indices) {
    x0 = Math.min(x0, o.x[i] - o.r[i]); y0 = Math.min(y0, o.y[i] - o.r[i]);
    x1 = Math.max(x1, o.x[i] + o.r[i]); y1 = Math.max(y1, o.y[i] + o.r[i]);
  }
  if (!(x1 > x0)) return viewOf(model, -1, width, height);
  return {centreX: (x0 + x1) / 2, centreY: (y0 + y1) / 2, scale: margin * Math.min(width / (x1 - x0), height / Math.max(y1 - y0, 1e-6))};
}
