// Provisional (ADR 0025, steps V1 and V2): the pure part of the scene renderer. No DOM, no WebGL: tested in Node.
//
// - `sceneBuffers`: what the WebGL renderer (`scene-gl.js`) draws, as typed arrays in the drawing order of `renderSvg`
//   (hulls, containers, routes, leaves, marks), with the same colours, widths and outlines, plus the texts for the 2D
//   canvas over it. The viewer computes no positions: every coordinate is the scene's.
// - `sceneIndex`: a uniform grid over the scene's elements, for picking and for the elements in view.
// - `nearPolicy`: which layer draws (far: WebGL, near: SVG), by the elements in view and their size on screen, with
//   hysteresis.
// - `sceneExcerpt`: the part of a scene in a rectangle, which the near layer hands to `renderSvg`.
import {isSceneColumns, sceneOfColumns} from './scene-columns.js';
import {createStyle} from './style.js';

/** Floats per box instance: x, y, width, height, corner radius, kind (0 rect, 1 circle), stroke width, dash (0/1),
 * fill RGBA, stroke RGBA (premultiplied later in the shader). */
export const BOX_STRIDE = 16;
/** Floats per segment instance: x0, y0, x1, y1, width (world), depth, dash on, dash off, distance at the start, RGBA. */
export const SEGMENT_STRIDE = 13;
/** Floats per triangle vertex: x, y, depth, RGBA. */
export const TRIANGLE_STRIDE = 7;
/** The layers in drawing order. */
export const LAYERS = ['hulls', 'containers', 'routes', 'leaves', 'marks', 'overlay'];

// A CSS colour as RGBA in 0..1: hex (3, 4, 6 or 8 digits), rgb()/rgba(), hsl()/hsla(); anything else (a named colour)
// through a canvas where there is one, else opaque black.
const canvasColours = new Map();
export function parseColour(css, alpha = 1) {
  const text = String(css).trim();
  let m = /^#([0-9a-f]{3,8})$/i.exec(text);
  if (m) {
    let h = m[1];
    if (h.length <= 4) h = [...h].map(c => c + c).join('');
    const v = n => parseInt(h.slice(n, n + 2), 16) / 255;
    return [v(0), v(2), v(4), (h.length === 8 ? v(6) : 1) * alpha];
  }
  m = /^rgba?\(\s*([\d.]+)[\s,]+([\d.]+)[\s,]+([\d.]+)(?:\s*[,/]\s*([\d.]+%?))?\s*\)$/i.exec(text);
  if (m) return [m[1] / 255, m[2] / 255, m[3] / 255, (m[4] === undefined ? 1 : m[4].endsWith('%') ? parseFloat(m[4]) / 100 : +m[4]) * alpha];
  m = /^hsla?\(\s*([\d.]+)(?:deg)?[\s,]+([\d.]+)%[\s,]+([\d.]+)%(?:\s*[,/]\s*([\d.]+%?))?\s*\)$/i.exec(text);
  if (m) return hsl(+m[1], +m[2], +m[3], (m[4] === undefined ? 1 : m[4].endsWith('%') ? parseFloat(m[4]) / 100 : +m[4]) * alpha);
  if (!canvasColours.has(text)) {
    let out = [0, 0, 0, 1];
    try {
      const ctx = (globalThis.OffscreenCanvas ? new OffscreenCanvas(1, 1) : globalThis.document?.createElement('canvas'))?.getContext('2d');
      if (ctx) { ctx.fillStyle = '#000'; ctx.fillStyle = text; const n = ctx.fillStyle; if (n !== text) out = parseColour(n); }
    } catch { /* black */ }
    canvasColours.set(text, out);
  }
  const c = canvasColours.get(text);
  return [c[0], c[1], c[2], c[3] * alpha];
}
const hex = parseColour;
// hsl(h s% l% / a) as renderSvg writes it for hulls.
function hsl(h, s, l, a = 1) {
  s /= 100; l /= 100;
  const k = n => (n + h / 30) % 12, f = n => l - s * Math.min(l, 1 - l) * Math.max(-1, Math.min(k(n) - 3, 9 - k(n), 1));
  return [f(0), f(8), f(4), a];
}
const NONE = [0, 0, 0, 0];

// The theme's colours (style.js) as the buffers need them: RGBA for shapes, CSS for the texts. Cached per style.
const coloursOf = (() => {
  const cache = new WeakMap();
  return style => {
    if (cache.has(style)) return cache.get(style);
    const t = style.theme;
    const c = {
      frame: hex(t.frameFill), frameStroke: hex(t.frameStroke), invalid: hex(t.invalid),
      leaf: hex(t.leafFill), leafStroke: hex(t.leafStroke), selectedFill: hex(t.selectedFill), selectedStroke: hex(t.selectedStroke),
      route: hex(t.route), activeRoute: hex(t.activeRoute), arrow: hex(t.arrow),
      bus: hex(t.bus), busSelected: hex(t.selectedStroke), hubFill: hex(t.hubFill), hubStroke: hex(t.hubStroke),
      portSource: hex(t.portSource), portTarget: hex(t.portTarget), crossing: hex(t.crossing), label: hex(t.labelFill),
      pin: hex(t.pin), trace: hex(t.trace), hover: hex(t.hover), junctionFill: hex(t.junctionFill), junctionStroke: hex(t.junctionStroke),
      text: t.text, title: t.title, row: t.row, busText: t.busText, labelText: t.labelText,
    };
    cache.set(style, c);
    return c;
  };
})();
/** The default theme's colours (RGBA and CSS), as renderSvg draws them without a style. */
export const COLOURS = coloursOf(createStyle());
// The first two numbers of a dash array ("6 4") as on and off lengths; none for none.
const dashOf = dash => { const v = String(dash ?? '').split(/[\s,]+/).map(Number).filter(x => x > 0); return v.length ? [v[0], v[1] ?? v[0]] : [0, 0]; };

const withAlpha = (c, a) => [c[0], c[1], c[2], c[3] * a];

class Floats {
  constructor() { this.data = new Float32Array(256); this.length = 0; }
  push(...values) {
    if (this.length + values.length > this.data.length) {
      const next = new Float32Array(Math.max(this.data.length * 2, this.length + values.length));
      next.set(this.data.subarray(0, this.length));
      this.data = next;
    }
    for (const v of values) this.data[this.length++] = v;
  }
  done() { return this.data.slice(0, this.length); }
}

// Ear clipping of a simple polygon (the hull's points); returns triangles as index triples. A degenerate rest is dropped.
export function triangulate(points) {
  const n = points.length;
  if (n < 3) return [];
  let area = 0;
  for (let i = 0; i < n; i++) { const a = points[i], b = points[(i + 1) % n]; area += a.x * b.y - b.x * a.y; }
  const ccw = area > 0;
  const idx = Array.from({length: n}, (_, i) => i);
  const cross = (a, b, c) => (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
  const inside = (p, a, b, c) => {
    const d1 = cross(a, b, p), d2 = cross(b, c, p), d3 = cross(c, a, p);
    return ccw ? d1 > 0 && d2 > 0 && d3 > 0 : d1 < 0 && d2 < 0 && d3 < 0;
  };
  const out = [];
  let guard = 0;
  while (idx.length > 3 && guard++ < n * n) {
    let clipped = false;
    for (let i = 0; i < idx.length; i++) {
      const ia = idx[(i + idx.length - 1) % idx.length], ib = idx[i], ic = idx[(i + 1) % idx.length];
      const a = points[ia], b = points[ib], c = points[ic];
      const turn = cross(a, b, c);
      if (ccw ? turn <= 0 : turn >= 0) continue;
      if (idx.some(j => j !== ia && j !== ib && j !== ic && inside(points[j], a, b, c))) continue;
      out.push([ia, ib, ic]);
      idx.splice(i, 1);
      clipped = true;
      break;
    }
    if (!clipped) break;
  }
  if (idx.length === 3) out.push([idx[0], idx[1], idx[2]]);
  return out;
}

// The arrowhead of renderSvg's marker (viewBox 0 0 10 10, path 0,0 10,5 0,10, refX 9, refY 5, 8 user units): its
// three corners for a line that ends at `end` in direction (dx, dy).
export function arrowCorners(end, dx, dy) {
  const len = Math.hypot(dx, dy) || 1, ux = dx / len, uy = dy / len;
  const at = (mx, my) => {
    const u = (mx - 9) * 0.8, v = (my - 5) * 0.8;
    return [end.x + ux * u - uy * v, end.y + uy * u + ux * v];
  };
  return [at(0, 0), at(10, 5), at(0, 10)];
}

/** The scene object of a scene or of its columns. */
export const sceneOf = scene => (isSceneColumns(scene) ? sceneOfColumns(scene) : scene);

/**
 * What the WebGL renderer draws for `scene` (a scene or its columns): per layer the box instances, segment instances
 * and triangle vertices, the texts, and the index. `state` as renderSvg's options: `selected` (node id), `selectedRoute`
 * (route id), `pinned` (ids; a dot on each pinned leaf), `trace` ({nodes, routes, distance, fade}: the path in amber,
 * the rest receding, as renderSvg's trace), `style` (a sheet or a resolved style, style.js). The last layer (overlay) is
 * left empty: `hoverBuffers` fills it.
 */
export function sceneBuffers(input, state = {}) {
  const scene = sceneOf(input);
  const style = createStyle(state.style), COLOURS = coloursOf(style), looks = style.bind(scene);
  const radiusOf = (look, r, fallback) => (look.radius === 'pill' ? Math.min(r.width, r.height) / 2 : look.radius ?? fallback);
  if (scene.schemaVersion !== 1) throw Error('Unsupported scene schema');
  const {selected = null, selectedRoute = null, trace = null} = state;
  // Routes drawn without an arrowhead (the cluster overview: a route that leaves a structured cluster ends at its rim).
  const noArrow = state.noArrow instanceof Set ? state.noArrow : new Set(state.noArrow ?? []);
  const pinned = state.pinned instanceof Set ? state.pinned : new Set(state.pinned ?? []);
  // The trace (renderSvg's applyTrace): routes and nodes on the path in amber and stronger, the other routes receding;
  // with fade, every node the fainter the farther it is from the path.
  const onPath = new Set(trace?.nodes ?? []), onTraceRoute = new Set(trace?.routes ?? []);
  const hops = id => (trace?.distance instanceof Map ? trace.distance.get(id) : trace?.distance?.[id]);
  const faint = id => { if (!trace?.fade || onPath.has(id)) return 1; const d = hops(id); return d === undefined ? 0.12 : Math.max(0.15, 1 - 0.3 * d); };
  const traced = (id, colour, width, opacity) => {
    if (!trace) return {colour: withAlpha(colour, opacity), width};
    if (onTraceRoute.has(id)) return {colour: COLOURS.trace, width: width + 1.5};
    return {colour: withAlpha(colour, trace.fade ? 0.08 : 0.25), width};
  };
  const layers = LAYERS.map(() => ({boxes: new Floats(), segments: new Floats(), triangles: new Floats()}));
  const [hullLayer, containerLayer, routeLayer, leafLayer, markLayer] = layers;
  const texts = [];
  const nodes = new Map(scene.nodes.map(n => [n.id, n]));
  const parents = new Set(scene.nodes.filter(n => n.container).map(n => n.id));
  const conflicted = new Set(), conflictedRoutes = new Set();
  for (const c of scene.conflicts ?? []) { for (const id of c.nodes ?? []) conflicted.add(id); for (const id of c.routes ?? []) conflictedRoutes.add(id); }
  const invalid = id => conflicted.has(id) || nodes.get(id)?.invalid;
  const depthOf = n => { let d = 0; while (n.parentId && nodes.has(n.parentId)) { d++; n = nodes.get(n.parentId); } return d; };
  const box = (layer, r, radius, kind, strokeWidth, fill, stroke, dash = 0) =>
    layer.boxes.push(r.x, r.y, r.width, r.height, radius, kind, strokeWidth, dash, ...fill, ...stroke);
  const text = t => texts.push(t);

  // Hulls (ADR 0022): filled, dashed outline, the name at the topmost point; one hue per dimension.
  const hulls = scene.membershipHulls ?? [];
  const dimensions = [...new Set(hulls.map(h => h.dimension))];
  for (const h of hulls) {
    if (!(h.points?.length >= 3)) continue;
    const hue = (dimensions.indexOf(h.dimension) * 67 + 191) % 360, fill = hsl(hue, 60, 55, 0.12), stroke = hsl(hue, 60, 65, 0.55);
    for (const [a, b, c] of triangulate(h.points)) for (const p of [h.points[a], h.points[b], h.points[c]]) hullLayer.triangles.push(p.x, p.y, 0, ...fill);
    let dist = 0;
    for (let i = 0; i < h.points.length; i++) {
      const a = h.points[i], b = h.points[(i + 1) % h.points.length];
      hullLayer.segments.push(a.x, a.y, b.x, b.y, 1.4, 0, 6, 4, dist, ...stroke);
      dist += Math.hypot(b.x - a.x, b.y - a.y);
    }
    const at = h.points.reduce((a, b) => (b.y < a.y || (b.y === a.y && b.x < a.x) ? b : a));
    const c = hsl(hue, 55, 78);
    text({x: at.x + 6, y: at.y - 5, text: h.label, size: 11, colour: `rgb(${c.slice(0, 3).map(v => Math.round(v * 255)).join(',')})`});
  }

  // Containers, outer first.
  for (const n of scene.nodes.filter(n => parents.has(n.id)).sort((a, b) => depthOf(a) - depthOf(b))) {
    const r = n.bounds, round = n.shape === 'circle', a = faint(n.id), lit = onPath.has(n.id), look = looks.node(n);
    const stroke = withAlpha(lit ? COLOURS.trace : invalid(n.id) ? COLOURS.invalid : look.stroke ? hex(look.stroke) : COLOURS.frameStroke, a);
    const fill = withAlpha(look.fill ? hex(look.fill) : COLOURS.frame, a), width = lit ? 2.5 : look.strokeWidth ?? 1, dash = look.dash ? 1 : 0;
    const title = look.text ?? COLOURS.title;
    if (round) {
      const rad = Math.min(r.width, r.height) / 2, mx = r.x + r.width / 2, my = r.y + r.height / 2;
      box(containerLayer, {x: mx - rad, y: my - rad, width: 2 * rad, height: 2 * rad}, rad, 1, width, fill, stroke, dash);
      text({x: mx, y: my - rad + Math.min(34, rad * 0.3), text: n.label.slice(0, 55), size: 14, colour: fadeCss(title, a), align: 'center'});
    } else {
      box(containerLayer, r, radiusOf(look, r, 10), 0, width, fill, stroke, dash);
      text({x: r.x + 12, y: r.y + 25, text: '▾ ' + n.label.slice(0, 55), size: 14, colour: fadeCss(title, a)});
    }
    if (n.kind === 'entity') containerLayer.segments.push(r.x, r.y + 34, r.x + r.width, r.y + 34, 1, 0, 0, 0, 0, ...COLOURS.frameStroke);
  }

  // Routes, then bus taps and hub stubs; each line has its own depth so that its overlapping segments blend once.
  const active = scene.routes.find(r => r.id === selectedRoute);
  const endpoints = new Set(active ? [active.source, active.target] : []);
  const lines = [];
  for (const route of scene.routes) {
    const weight = route.weight ?? 1, isActive = route === active, dim = !!active && !isActive, look = looks.edge(route);
    const aggregated = route.backWeight !== undefined && route.backWeight !== null, back = aggregated ? route.backWeight : 0, total = weight + back;
    const width = (look.width ?? (aggregated ? Math.min(7, 1 + 1.2 * Math.log2(total + 1)) : Math.min(4, 1.6 + 0.4 * (weight - 1)))) + (isActive ? 1.5 : 0);
    const opacity = dim ? 0.2 : isActive ? 1 : look.opacity ?? (aggregated ? 0.3 + 0.55 * Math.min(1, Math.log2(total + 1) / 6) : 0.75);
    const colour = route.invalid || conflictedRoutes.has(route.id) ? COLOURS.invalid : isActive ? COLOURS.activeRoute : look.stroke ? hex(look.stroke) : COLOURS.route;
    lines.push({points: route.points, ...traced(route.id, colour, width, opacity), start: back > 0, arrow: look.arrow === 'none' || noArrow.has(route.id) ? null : look.arrow ? hex(look.arrow) : COLOURS.arrow, dash: dashOf(look.dash)});
    if ((aggregated && total > 1) || weight > 1) {
      const mid = route.points[Math.floor(route.points.length / 2)];
      text({x: mid.x + 6, y: mid.y - 6, text: aggregated && back ? `${weight} ⇄ ${back}` : String(weight), size: 12, colour: COLOURS.text});
    }
  }
  for (const bus of scene.busLines ?? []) for (const tap of bus.taps) lines.push({points: tap.points, ...traced(tap.edge, COLOURS.route, 1.6, active ? 0.2 : 0.75)});
  for (const ref of scene.hubRefs ?? []) lines.push({points: ref.points, ...traced(ref.edge, COLOURS.route, 1.6, active ? 0.2 : 0.85)});
  lines.forEach((line, i) => {
    // Later lines nearer (smaller depth), as later SVG elements cover earlier ones.
    const depth = 1 - (i + 1) / (lines.length + 2);
    const p = line.points;
    const [on, off] = line.dash ?? [0, 0];
    let along = 0;
    for (let k = 0; k + 1 < p.length; k++) {
      routeLayer.segments.push(p[k].x, p[k].y, p[k + 1].x, p[k + 1].y, line.width, depth, on, off, along, ...line.colour);
      along += Math.hypot(p[k + 1].x - p[k].x, p[k + 1].y - p[k].y);
    }
    if (p.length >= 2 && line.arrow !== null) {
      const arrowColour = line.arrow ?? COLOURS.arrow;
      const arrow = (end, from) => { for (const [x, y] of arrowCorners(end, end.x - from.x, end.y - from.y)) routeLayer.triangles.push(x, y, depth, ...arrowColour); };
      const last = p.length - 1;
      let j = last - 1; while (j > 0 && p[j].x === p[last].x && p[j].y === p[last].y) j--;
      arrow(p[last], p[j]);
      if (line.start) { let s = 1; while (s < last && p[s].x === p[0].x && p[s].y === p[0].y) s++; arrow(p[0], p[s]); }
    }
  });
  for (const bus of scene.busLines ?? []) {
    const r = bus.bounds, on = bus.id === selected, a = faint(bus.id), lit = onPath.has(bus.id);
    box(routeLayer, r, Math.min(r.width, r.height) / 2, 0, lit ? 2.5 : 1, withAlpha(on ? COLOURS.busSelected : COLOURS.bus, a), withAlpha(lit ? COLOURS.trace : on ? COLOURS.busSelected : COLOURS.leafStroke, a));
    const flat = r.width >= r.height, at = busLabelAt(bus, flat);
    text(flat ? {x: at, y: r.y - 6, text: bus.label.slice(0, 40), size: 12, colour: COLOURS.busText, align: 'center'}
      : {x: r.x + r.width + 14, y: at, text: bus.label.slice(0, 40), size: 12, colour: COLOURS.busText, align: 'center', angle: -Math.PI / 2});
  }
  for (const ref of scene.hubRefs ?? []) {
    const r = ref.label;
    box(routeLayer, r, 3, 0, 0.8, COLOURS.hubFill, COLOURS.hubStroke);
    text({x: r.x + 4, y: r.y + r.height / 2, text: (ref.outgoing ? '→ ' : '← ') + ref.text.slice(0, 32), size: 10.5, colour: COLOURS.busText, baseline: 'middle'});
  }
  for (const j of scene.junctions ?? []) box(routeLayer, {x: j.x - 3.5, y: j.y - 3.5, width: 7, height: 7}, 3.5, 1, 1, COLOURS.junctionFill, COLOURS.junctionStroke);

  // Leaves.
  for (const n of scene.nodes.filter(n => !parents.has(n.id))) {
    const r = n.bounds, faded = (active && !endpoints.has(n.id) ? 0.4 : 1) * faint(n.id), lit = onPath.has(n.id), look = looks.node(n);
    const fill = withAlpha(n.id === selected ? COLOURS.selectedFill : look.fill ? hex(look.fill) : COLOURS.leaf, faded);
    const stroke = withAlpha(lit ? COLOURS.trace : invalid(n.id) ? COLOURS.invalid : n.id === selected ? COLOURS.selectedStroke : look.stroke ? hex(look.stroke) : COLOURS.leafStroke, faded);
    const strokeWidth = lit ? 2.5 : look.strokeWidth ?? 1;
    const attribute = n.kind === 'attribute', round = n.shape === 'circle' && !attribute && !n.rows?.length;
    const dash = n.kind === 'rest' || look.dash ? 1 : 0;
    const cx = r.x + r.width / 2, cy = r.y + r.height / 2;
    if (!attribute) {
      if (n.shape === 'circle') { const rad = Math.min(r.width, r.height) / 2; box(leafLayer, {x: cx - rad, y: cy - rad, width: 2 * rad, height: 2 * rad}, rad, 1, strokeWidth, fill, stroke, dash); }
      else box(leafLayer, r, radiusOf(look, r, 7), 0, strokeWidth, fill, stroke, dash);
    }
    // A pinned leaf: renderSvg's dot (drawn there when the host has no pin control, which the far layer never has).
    if (pinned.has(n.id) && !attribute) {
      const rad = Math.min(r.width, r.height) / 2, px = round ? cx + rad * 0.62 : r.x + r.width - 10, py = round ? cy - rad * 0.62 : r.y + 10;
      box(leafLayer, {x: px - 4, y: py - 4, width: 8, height: 8}, 4, 1, 0, withAlpha(COLOURS.pin, faded), NONE);
    }
    const own = look.text ?? COLOURS.text, colour = faded < 1 ? fadeCss(own, faded) : own;
    if (!n.rows?.length) {
      const label = (n.collapsed ? '▸ ' : '') + n.label.slice(0, 23);
      text(round ? {x: cx, y: cy + 5, text: label, size: 13, colour, align: 'center'} : {x: r.x + 10, y: attribute ? r.y + r.height / 2 + 5 : r.y + 32, text: label, size: 13, colour});
    } else {
      n.rows.forEach((row, i) => { const b = row.bounds; text({x: b.x + 6, y: b.y + b.height / 2 + 5, text: row.label, size: i === 0 ? 14 : 13, weight: i === 0 ? 600 : 400, colour: i === 0 ? colour : fadeCss(COLOURS.row, faded)}); });
      const t = n.rows[0].bounds;
      if (n.rows.length > 1) leafLayer.segments.push(r.x, t.y + t.height + 2, r.x + r.width, t.y + t.height + 2, 1, 0, 0, 0, 0, ...withAlpha(COLOURS.frameStroke, faded));
    }
  }

  // Marks: ports, crossings, edge labels.
  for (const port of scene.showPorts ? scene.ports : []) {
    const p = port.position;
    box(markLayer, {x: p.x - 3, y: p.y - 3, width: 6, height: 6}, 3, 1, 1, port.endpoint === 'source' ? COLOURS.portSource : COLOURS.portTarget, COLOURS.frame);
  }
  for (const c of scene.crossings ?? []) box(markLayer, {x: c.position.x - 3, y: c.position.y - 3, width: 6, height: 6}, 0, 0, 0, COLOURS.crossing, NONE);
  for (const label of scene.labels ?? []) {
    const r = label.bounds;
    box(markLayer, r, 3, 0, 0, COLOURS.label, NONE);
    text({x: r.x + 4, y: r.y + r.height / 2, text: label.text, size: 12, colour: COLOURS.labelText, baseline: 'middle'});
  }

  return {
    scene,
    state: {...state},
    bounds: scene.bounds,
    layers: layers.map(l => ({boxes: l.boxes.done(), segments: l.segments.done(), triangles: l.triangles.done()})),
    texts,
    index: sceneIndex(scene),
  };
}

/**
 * The overlay layer for the element under the pointer: renderSvg's hover outline (#7fb4ff) on a leaf, a container or a
 * bus bar; empty for none. Small, so that a hover never rebuilds the scene's buffers.
 */
export function hoverBuffers(scene, id, styleSheet) {
  const style = createStyle(styleSheet), COLOURS = coloursOf(style);
  const out = {boxes: new Floats(), segments: new Floats(), triangles: new Floats()};
  const n = id == null ? null : scene.nodes.find(m => m.id === id) ?? scene.busLines?.find(b => b.id === id);
  if (n && n.kind !== 'attribute') {
    const r = n.bounds, bar = !scene.nodes.includes(n), round = !bar && n.shape === 'circle' && !(n.rows?.length && !n.container);
    if (round) { const rad = Math.min(r.width, r.height) / 2, cx = r.x + r.width / 2, cy = r.y + r.height / 2; out.boxes.push(cx - rad, cy - rad, 2 * rad, 2 * rad, rad, 1, 2, 0, ...NONE, ...COLOURS.hover); }
    else {
      const own = bar ? {} : style.bind(scene).node(n), radius = bar || own.radius === 'pill' ? Math.min(r.width, r.height) / 2 : own.radius ?? (n.container ? 10 : 7);
      out.boxes.push(r.x, r.y, r.width, r.height, radius, 0, 2, 0, ...NONE, ...COLOURS.hover);
    }
  }
  return {boxes: out.boxes.done(), segments: out.segments.done(), triangles: out.triangles.done()};
}

const fadeCss = (css, a) => {
  if (a >= 1) return css;
  const c = hex(css);
  return `rgba(${Math.round(c[0] * 255)},${Math.round(c[1] * 255)},${Math.round(c[2] * 255)},${+(c[3] * a).toFixed(4)})`;
};

// As svg.js: where along a bus bar its name goes.
function busLabelAt(bus, flat) {
  const r = bus.bounds, lo = flat ? r.x : r.y, hi = flat ? r.x + r.width : r.y + r.height;
  const cuts = [];
  for (const tap of bus.taps) for (const p of tap.points) if (flat ? p.y < r.y - 0.5 : p.x > r.x + r.width + 0.5) cuts.push(flat ? p.x : p.y);
  if (!cuts.length) return (lo + hi) / 2;
  const marks = [lo, ...cuts.filter(c => c > lo && c < hi).sort((a, b) => a - b), hi];
  let best = [lo, hi], width = -1;
  for (let i = 1; i < marks.length; i++) if (marks[i] - marks[i - 1] > width) { width = marks[i] - marks[i - 1]; best = [marks[i - 1], marks[i]]; }
  return (best[0] + best[1]) / 2;
}

const intersects = (a, b) => a.x <= b.x + b.width && b.x <= a.x + a.width && a.y <= b.y + b.height && b.y <= a.y + a.height;

/**
 * A uniform grid over the scene: nodes (leaves, containers, bus bars) by their boxes, lines (routes, bus taps, hub
 * stubs) by their segments. `query(rect)` gives the elements that may touch `rect`; `pick(x, y, tolerance)` the element
 * under a world point as renderSvg's hit order has it: a leaf or a bus bar, else a line within the tolerance, else the
 * deepest container.
 */
export function sceneIndex(scene) {
  const b = scene.bounds;
  const items = [];
  for (const n of scene.nodes) items.push({kind: 'node', id: n.id, container: !!n.container, bounds: n.bounds, depth: 0, parentId: n.parentId});
  for (const bus of scene.busLines ?? []) items.push({kind: 'node', id: bus.id, container: false, bounds: bus.bounds, bus: true});
  const byId = new Map(items.map(i => [i.id, i]));
  for (const i of items) { let d = 0, p = i.parentId; while (p && byId.has(p) && d < 1000) { d++; p = byId.get(p).parentId; } i.depth = d; }
  const nodeCount = items.length;
  const segments = [];
  const line = (id, points, width) => {
    const at = items.length;
    items.push({kind: 'route', id, points, width});
    for (let k = 0; k + 1 < points.length; k++) segments.push([at, points[k], points[k + 1]]);
  };
  for (const r of scene.routes) line(r.id, r.points, Math.min(7, 1.6 + 0.4 * ((r.weight ?? 1) - 1)));
  for (const bus of scene.busLines ?? []) for (const tap of bus.taps) line(tap.edge, tap.points, 1.6);
  for (const ref of scene.hubRefs ?? []) line(ref.edge, ref.points, 1.6);
  const count = Math.max(1, nodeCount + segments.length);
  const cell = Math.max(8, Math.sqrt((b.width * b.height) / count) * 2);
  const cols = Math.max(1, Math.ceil(b.width / cell)), rows = Math.max(1, Math.ceil(b.height / cell));
  const cells = new Map();
  const clampCol = x => Math.min(cols - 1, Math.max(0, Math.floor((x - b.x) / cell)));
  const clampRow = y => Math.min(rows - 1, Math.max(0, Math.floor((y - b.y) / cell)));
  const add = (x0, y0, x1, y1, entry) => {
    for (let r = clampRow(Math.min(y0, y1)); r <= clampRow(Math.max(y0, y1)); r++)
      for (let c = clampCol(Math.min(x0, x1)); c <= clampCol(Math.max(x0, x1)); c++) {
        const key = r * cols + c;
        let list = cells.get(key);
        if (!list) cells.set(key, list = []);
        list.push(entry);
      }
  };
  for (let i = 0; i < nodeCount; i++) { const r = items[i].bounds; add(r.x, r.y, r.x + r.width, r.y + r.height, i); }
  segments.forEach(([, p, q], s) => add(p.x, p.y, q.x, q.y, nodeCount + s));
  const visit = (rect, f) => {
    for (let r = clampRow(rect.y); r <= clampRow(rect.y + rect.height); r++)
      for (let c = clampCol(rect.x); c <= clampCol(rect.x + rect.width); c++) for (const e of cells.get(r * cols + c) ?? []) f(e);
  };
  const segmentBox = ([, p, q]) => ({x: Math.min(p.x, q.x), y: Math.min(p.y, q.y), width: Math.abs(q.x - p.x), height: Math.abs(q.y - p.y)});
  return {
    items,
    /** The node item (leaf, container or bus bar) of an id. */
    node: id => byId.get(id),
    /** The ids of the nodes and lines that touch `rect` (world), each once. */
    query(rect) {
      const nodesIn = new Set(), linesIn = new Set();
      visit(rect, e => {
        if (e < nodeCount) { if (intersects(items[e].bounds, rect)) nodesIn.add(items[e].id); }
        else { const s = segments[e - nodeCount]; if (intersects(segmentBox(s), rect)) linesIn.add(items[s[0]].id); }
      });
      return {nodes: nodesIn, routes: linesIn};
    },
    pick(x, y, tolerance = 0) {
      const at = {x: x - tolerance, y: y - tolerance, width: 2 * tolerance, height: 2 * tolerance};
      let leaf = null, container = null, route = null, best = Infinity;
      visit(at, e => {
        if (e < nodeCount) {
          const it = items[e], r = it.bounds;
          if (x < r.x || x > r.x + r.width || y < r.y || y > r.y + r.height) return;
          if (it.container) { if (!container || it.depth > container.depth) container = it; }
          else if (!leaf || it.depth > leaf.depth) leaf = it;
        } else {
          const [owner, p, q] = segments[e - nodeCount];
          const d = segmentDistance(x, y, p, q), reach = Math.max(tolerance, items[owner].width / 2);
          if (d <= reach && d < best) { best = d; route = items[owner]; }
        }
      });
      const hit = leaf ?? route ?? container;
      return hit ? {kind: hit.kind, id: hit.id} : null;
    },
  };
}

function segmentDistance(x, y, p, q) {
  const dx = q.x - p.x, dy = q.y - p.y, len = dx * dx + dy * dy;
  const t = len ? Math.max(0, Math.min(1, ((x - p.x) * dx + (y - p.y) * dy) / len)) : 0;
  return Math.hypot(x - (p.x + t * dx), y - (p.y + t * dy));
}

/** The world rectangle a view shows on a canvas of `width` by `height` CSS pixels (view: centre and CSS pixels per unit). */
export function viewRect(view, width, height) {
  return {x: view.centreX - width / 2 / view.scale, y: view.centreY - height / 2 / view.scale, width: width / view.scale, height: height / view.scale};
}

/**
 * Which layer draws: 'near' (SVG) when at most `budget` elements are in view and the median box in view is at least
 * `minPixels` across on screen; back to 'far' only above `budget` times `hysteresis` elements or below `minPixels`
 * divided by it, so that a zoom at the threshold does not flicker. Provisional values until measured (ADR 0025).
 */
export function nearPolicy({elements, boxPixels, current = 'far', budget = 400, minPixels = 24, hysteresis = 1.25}) {
  if (current === 'near') return elements > budget * hysteresis || boxPixels < minPixels / hysteresis ? 'far' : 'near';
  return elements <= budget && boxPixels >= minPixels ? 'near' : 'far';
}

/** The median size on screen (the smaller side, in CSS pixels) of the leaves among `ids` at `scale`; 0 for none. */
export function medianBoxPixels(index, ids, scale) {
  const sizes = [];
  for (const id of ids) { const it = index.node(id); if (it && !it.container) sizes.push(Math.min(it.bounds.width, it.bounds.height) * scale); }
  if (!sizes.length) return 0;
  sizes.sort((a, b) => a - b);
  return sizes[Math.floor(sizes.length / 2)];
}

/**
 * The part of `scene` that touches `rect` (world), as a scene for `renderSvg` with `rect` as its bounds: the nodes in
 * it with their containers, the lines with a segment in it (whole), and the ports, labels, crossings, junctions, hulls,
 * bus bars and hub stubs in it. Coordinates are unchanged.
 */
export function sceneExcerpt(scene, index, rect) {
  const {nodes: nodeIds, routes: routeIds} = index.query(rect);
  const byId = new Map(scene.nodes.map(n => [n.id, n]));
  const keep = new Set();
  for (const id of nodeIds) { let n = byId.get(id); while (n && !keep.has(n.id)) { keep.add(n.id); n = byId.get(n.parentId); } }
  const inside = p => p.x >= rect.x && p.x <= rect.x + rect.width && p.y >= rect.y && p.y <= rect.y + rect.height;
  const out = {
    ...scene,
    bounds: {...rect},
    nodes: scene.nodes.filter(n => keep.has(n.id)),
    routes: scene.routes.filter(r => routeIds.has(r.id)),
    ports: (scene.ports ?? []).filter(p => inside(p.position)),
    labels: (scene.labels ?? []).filter(l => intersects(l.bounds, rect)),
    crossings: (scene.crossings ?? []).filter(c => inside(c.position)),
  };
  if (scene.junctions) out.junctions = scene.junctions.filter(inside);
  if (scene.membershipHulls) out.membershipHulls = scene.membershipHulls.filter(h => h.points?.some(inside) || intersects(boundsOf(h.points ?? []), rect));
  if (scene.busLines) out.busLines = scene.busLines.filter(b => nodeIds.has(b.id) || b.taps.some(t => routeIds.has(t.edge)));
  if (scene.hubRefs) out.hubRefs = scene.hubRefs.filter(h => routeIds.has(h.edge) || intersects(h.label, rect));
  return out;
}

function boundsOf(points) {
  if (!points.length) return {x: 0, y: 0, width: -1, height: -1};
  let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
  for (const p of points) { x0 = Math.min(x0, p.x); y0 = Math.min(y0, p.y); x1 = Math.max(x1, p.x); y1 = Math.max(y1, p.y); }
  return {x: x0, y: y0, width: x1 - x0, height: y1 - y0};
}
