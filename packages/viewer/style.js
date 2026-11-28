// Provisional: the appearance of a scene, one model for every renderer (renderSvg, the GPU far layer, the React Flow
// mapping). Pure, no DOM except `themeFromCss`.
//
// Two parts, as graph viewers commonly split them (Cytoscape.js: a stylesheet of selectors; yFiles and React Flow:
// themes and CSS variables):
// - a theme: named colours (tokens) for every part a renderer draws and for the states (selected, invalid, trace,
//   hover). A host may set them from CSS custom properties (`--arqlet-leaf-fill: …`, `themeFromCss`), so a page's CSS
//   (dark and light, a brand) reaches the GPU layer too, which cannot read CSS per element;
// - rules: a selector over an element's data (`node`, `container`, `leaf`, `edge`, with `[kind=…]`, `[shape=…]`,
//   `#id`; `[key]` alone tests that the field is set) and properties. Later rules win over earlier ones, as in CSS. A
//   colour may be a token (`$route`), `auto` (a categorical colour from the element's kind, stable for a kind across
//   views) or `auto(field)` (from another field, for example `auto(bus)`).
// - data a scene derives for the rules (`bind(scene)`): an edge's `bus` (the index of the explicit bus it belongs to,
//   `scene.buses`), a node's `bus` (the bus it is on, when on one) and `interchange` (on more than one bus) and `buses`
//   (how many). So a bus can be drawn as a metro line and a node on several buses as an interchange.
//
// Rules decide by what an element is; states (selection, trace, hover) take the theme's colours in every renderer.
// The defaults are the colours renderSvg drew before, so a scene without a style draws exactly as before.

/** The default theme: the colours of the dark viewer. */
export const DEFAULT_THEME = Object.freeze({
  frameFill: '#142133', frameStroke: '#344961', title: '#9eb8d1',
  leafFill: '#24364b', leafStroke: '#4c6682', text: '#edf5ff', row: '#c4d6e8',
  selectedFill: '#245e50', selectedStroke: '#90e4c3', invalid: '#f08f80',
  route: '#73bba7', activeRoute: '#c8f5e4', arrow: '#73bba7',
  bus: '#6f93b8', busText: '#9fc3e6', hubFill: '#142133', hubStroke: '#3b5068',
  portSource: '#7ee1bc', portTarget: '#9dbaff', crossing: '#edc477', labelFill: '#142133', labelText: '#edf5ff',
  junctionFill: '#73bba7', junctionStroke: '#142133',
  pin: '#edc477', control: '#8aa2bb', controlIcon: '#c4d6e8', hover: '#7fb4ff', trace: '#ffc861',
});

/** Categorical colours for `auto`, readable on the dark theme (hue steps of a golden angle, fixed lightness). */
export const PALETTE = Object.freeze(['#73bba7', '#e6a560', '#8fb4f0', '#d98cb3', '#b5c96a', '#c99af0', '#6fc7d9', '#e88a7d', '#d4c06a', '#9aa7b8']);

// FNV-1a over the text, so that a kind keeps its colour whatever else the view holds.
function hash(text) {
  let h = 0x811c9dc5;
  for (let i = 0; i < text.length; i++) { h ^= text.charCodeAt(i); h = Math.imul(h, 0x01000193) >>> 0; }
  return h >>> 0;
}
/** The categorical colour of a kind (a number picks by index, so buses 0, 1, 2 … take distinct colours). */
export const autoColour = (kind, palette = PALETTE) => palette[(typeof kind === 'number' && Number.isInteger(kind) && kind >= 0 ? kind : hash(String(kind ?? ''))) % palette.length];

// A selector: `node`, `container`, `leaf`, `edge` (or `*`), then any of `[key=value]`, `[key]` and `#id`.
function parseSelector(text) {
  const m = /^\s*(node|container|leaf|edge|\*)?((?:\[[^\]=]+(?:=[^\]]*)?\])*)(#[^\s[]+)?\s*$/.exec(text);
  if (!m) throw Error(`Unsupported selector: ${text}`);
  const tests = [...(m[2] ?? '').matchAll(/\[([^\]=]+)(?:=([^\]]*))?\]/g)].map(([, k, v]) => [k.trim(), v === undefined ? undefined : v.trim().replace(/^["']|["']$/g, '')]);
  return {type: m[1] ?? '*', tests, id: m[3]?.slice(1) ?? null};
}
function matches(selector, type, item) {
  if (selector.type !== '*') {
    if (type === 'edge' ? selector.type !== 'edge' : selector.type === 'edge') return false;
    if (selector.type === 'container' && !item.container) return false;
    if (selector.type === 'leaf' && item.container) return false;
  }
  if (selector.id !== null && item.id !== selector.id) return false;
  return selector.tests.every(([k, v]) => (v === undefined ? item[k] !== undefined && item[k] !== null && item[k] !== false : String(item[k] ?? '') === v));
}

const NODE_PROPERTIES = ['fill', 'stroke', 'strokeWidth', 'text', 'dash', 'radius', 'glyph', 'glyphSize'];
const EDGE_PROPERTIES = ['stroke', 'arrow', 'width', 'opacity', 'dash'];

/**
 * A resolved style from a sheet `{theme?, rules?}` (theme: tokens over the defaults; rules: `[{selector, style}]`), or
 * from an already resolved style (returned as it is). `node(n)` and `edge(r)` give the properties the rules set for an
 * element, with tokens and `auto` resolved; renderers fall back to the theme for what is not set.
 */
export function createStyle(sheet = {}) {
  if (sheet && sheet.resolved === true) return sheet;
  const theme = {...DEFAULT_THEME, ...(sheet?.theme ?? {})};
  const palette = sheet?.palette ?? PALETTE;
  const rules = (sheet?.rules ?? []).map(r => ({selector: parseSelector(r.selector), style: r.style ?? {}}));
  const colour = (value, item) => {
    if (typeof value !== 'string') return value;
    if (value === 'auto') return autoColour(item.kind, palette);
    const by = /^auto\((\w+)\)$/.exec(value);
    if (by) return autoColour(item[by[1]], palette);
    if (value.startsWith('$')) return theme[value.slice(1)] ?? value;
    return value;
  };
  const resolve = (type, item, allowed) => {
    const out = {};
    for (const r of rules) if (matches(r.selector, type, item)) for (const k of allowed) if (r.style[k] !== undefined) out[k] = colour(r.style[k], item);
    return out;
  };
  const plain = !rules.length;
  const empty = Object.freeze({});
  const node = n => (plain ? empty : resolve('node', n, NODE_PROPERTIES)), edge = r => (plain ? empty : resolve('edge', r, EDGE_PROPERTIES));
  const bound = new WeakMap();
  return {
    resolved: true,
    theme,
    palette,
    /** Whether the sheet brings its own palette (the cluster overview then colours its clusters from it). */
    hasOwnPalette: Array.isArray(sheet?.palette),
    /** Whether the sheet only has the default theme and no rules (renderers may take their fast paths). */
    isDefault: plain && Object.keys(theme).every(k => theme[k] === DEFAULT_THEME[k]),
    node,
    edge,
    /** `node` and `edge` for the elements of `scene`, with the data the scene derives (bus, interchange). */
    bind(scene) {
      if (plain) return {node, edge};
      if (bound.has(scene)) return bound.get(scene);
      const {routeBus, nodeBuses} = busData(scene);
      const out = {
        node: n => {
          const b = nodeBuses.get(n.id);
          if (!b) return node(n);
          // `branch`: a line branches at the node (one bus's routes lead from it to three or more other nodes; parallel
          // routes and loops count once), as at a gateway.
          const branch = [...b.values()].some(others => others.size >= 3);
          return node({...n, bus: b.size === 1 ? [...b.keys()][0] : undefined, buses: b.size, interchange: b.size > 1, ...(branch ? {branch} : {})});
        },
        edge: r => (routeBus.has(r.id) ? edge({...r, bus: routeBus.get(r.id)}) : edge(r)),
      };
      bound.set(scene, out);
      return out;
    },
  };
}

// The explicit buses of a scene: the bus of each route, and the buses each node is an end of (with the other nodes each
// bus's routes lead to from it).
function busData(scene) {
  const routeBus = new Map(), nodeBuses = new Map();
  (scene.buses ?? []).forEach((edges, b) => { for (const id of edges) if (!routeBus.has(id)) routeBus.set(id, b); });
  for (const r of scene.routes ?? []) {
    const b = routeBus.get(r.id);
    if (b === undefined) continue;
    for (const [end, far] of [[r.source, r.target], [r.target, r.source]]) {
      if (!nodeBuses.has(end)) nodeBuses.set(end, new Map());
      const m = nodeBuses.get(end);
      if (!m.has(b)) m.set(b, new Set());
      if (far !== end) m.get(b).add(far);
    }
  }
  return {routeBus, nodeBuses};
}

/**
 * Provisional: a node drawn as a glyph (`glyph: 'dot'`): a dot of radius `glyphSize` (default 7) in the middle of its box
 * instead of the box, its name outside, right of and below the dot (where lines running across or along do not cross
 * it). `null` for a node drawn as its box.
 */
export function glyphOf(look, bounds) {
  if (look?.glyph !== 'dot' && look?.glyph !== 'tick' && look?.glyph !== 'pill') return null;
  const cx = bounds.x + bounds.width / 2, cy = bounds.y + bounds.height / 2;
  if (look.glyph === 'tick') return placeGlyph({kind: 'tick', cx, cy, r: 0, length: look.glyphSize ?? 9, width: look.strokeWidth ?? 4}, 0, -1);
  if (look.glyph === 'pill') {
    const thickness = look.glyphSize ?? 11;
    return placeGlyph({kind: 'pill', cx, cy, r: thickness / 2, thickness, length: thickness * 2.4, width: look.strokeWidth ?? 3}, 0, -1);
  }
  const r = look.glyphSize ?? 7;
  return {kind: 'dot', cx, cy, r, labelX: cx + r + 3, labelY: cy + r + 11};
}

// A tick standing out from its middle along the unit normal (nx, ny), or a pill lying along it across its middle (x2,
// y2: the tip, or the pill's end). Its name goes in the unit direction (lx, ly), by default the normal, just beyond the
// glyph's outline: beside it (left- or right-aligned) when that points sideways, centred above or below when it points
// up or down, at a corner when it points diagonally.
function placeGlyph(g, nx, ny, lx = nx, ly = ny) {
  const pill = g.kind === 'pill', reach = pill ? g.length / 2 : g.length;
  const x2 = g.cx + nx * reach, y2 = g.cy + ny * reach, gap = 4;
  // How far the outline reaches in the name's direction: a pill's half length along its axis, half its thickness across.
  const along = lx * nx + ly * ny, out = (pill ? Math.abs(along) * g.length / 2 + Math.abs(lx * ny - ly * nx) * g.thickness / 2 : Math.max(0, along) * g.length) + gap;
  const sideways = Math.abs(lx) > 0.38, vertical = Math.abs(ly) > 0.38;
  return {...g, nx, ny, x2, y2, labelX: g.cx + lx * out, labelY: g.cy + ly * out,
    align: sideways ? (lx > 0 ? 'left' : 'right') : 'center', baseline: vertical ? (ly < 0 ? 'bottom' : 'top') : 'middle'};
}

// The eight compass directions in the order a name prefers them (right of and above first, as the glyphs point).
const R2 = Math.SQRT1_2;
const NAME_DIRECTIONS = [[R2, -R2], [1, 0], [0, -1], [R2, R2], [0, 1], [-R2, -R2], [-1, 0], [-R2, R2]];
const OCTANTS = [[1, 0], [R2, R2], [0, 1], [-R2, R2], [-1, 0], [-R2, -R2], [0, -1], [R2, -R2]];

/**
 * Provisional: turns every tick (`glyph: 'tick'`, a station on one line drawn as a short stub, as metro maps draw
 * stations without changes) and every pill (`glyph: 'pill'`, an interchange or a branching station lying across its
 * lines) across the lines at the station. Their direction comes from the station's routes as drawn (`routeToGlyphs`),
 * those of an explicit bus first: with two, the direction from one neighbour to the other; with one, that route; with
 * more, their main axis (a line running through counts once, whichever way). The glyph points to the side of the line
 * where fewer lines leave (on a tie, to the right, or up when the line runs across); a pill's name goes between its
 * lines when both sides have one. A station without routes keeps its glyph pointing up. Replaces them in `glyphs` (a map
 * of node id to glyph) and returns it.
 */
export function orientGlyphs(glyphs, scene) {
  const turned = g => g?.kind === 'tick' || g?.kind === 'pill';
  if (![...glyphs.values()].some(turned)) return glyphs;
  const {routeBus} = busData(scene);
  const away = new Map();
  for (const r of scene.routes ?? []) {
    const gs = glyphs.get(r.source), gt = glyphs.get(r.target);
    if (!turned(gs) && !turned(gt)) continue;
    const p = routeToGlyphs(r.points, gs, gt);
    if (!p?.length) continue;
    const onBus = routeBus.has(r.id);
    for (const [id, g, ordered] of [[r.source, gs, p], [r.target, gt, [...p].reverse()]]) {
      if (!turned(g)) continue;
      // The first point of the route that is not the station's middle: where the line leaves it.
      const next = ordered.find(q => Math.hypot(q.x - g.cx, q.y - g.cy) > 1e-6);
      if (!next) continue;
      const l = Math.hypot(next.x - g.cx, next.y - g.cy);
      if (!away.has(id)) away.set(id, {bus: [], other: []});
      away.get(id)[onBus ? 'bus' : 'other'].push([(next.x - g.cx) / l, (next.y - g.cy) / l]);
    }
  }
  for (const [id, {bus, other}] of away) {
    const v = bus.length ? bus : other, g = glyphs.get(id);
    // Along the line: from the second neighbour to the first, or the one route's direction; with more, the main axis of
    // their directions (summed with doubled angles, so opposite directions agree: (x² - y², 2xy) for a unit vector),
    // or the first one's when they balance out (a crossing), turned to the nearest multiple of 45°, as a metro map's
    // lines run.
    let [dx, dy] = v.length === 2 ? [v[0][0] - v[1][0], v[0][1] - v[1][1]] : v[0];
    if (v.length > 2) {
      const c = v.reduce((sum, [x, y]) => sum + x * x - y * y, 0), s2 = v.reduce((sum, [x, y]) => sum + 2 * x * y, 0), m = Math.hypot(c, s2);
      if (m > 1e-6) [dx, dy] = [Math.sqrt(Math.max(0, (1 + c / m) / 2)), (s2 < 0 ? -1 : 1) * Math.sqrt(Math.max(0, (1 - c / m) / 2))];
      [dx, dy] = OCTANTS[((Math.round(Math.atan2(dy, dx) / (Math.PI / 4)) % 8) + 8) % 8];
    }
    let l = Math.hypot(dx, dy);
    // Two neighbours on the same side (a line that turns back): across their middle instead.
    if (l < 1e-6) { [dx, dy] = [-v[0][1], v[0][0]]; l = 1; }
    let nx = -dy / l, ny = dx / l;
    if (Math.abs(nx) > 0.38 ? nx < 0 : ny > 0) { nx = -nx; ny = -ny; }
    // Where no line runs: how far a direction stays from every line leaving the station (0 along one, 2 opposite it).
    const all = [...bus, ...other], clear = (x, y) => Math.min(...all.map(([a, b]) => 1 - (a * x + b * y)));
    // The glyph points to the freer side (on a tie, as above); a pill's name, when even that side has a line within
    // 30°, goes to the freest of the eight compass directions instead (a crossing: between its lines).
    if (clear(-nx, -ny) > clear(nx, ny) + 1e-9) { nx = -nx; ny = -ny; }
    let [lx, ly] = [nx, ny];
    if (g.kind === 'pill' && clear(lx, ly) < 1 - Math.cos(Math.PI / 6)) {
      let best = -1;
      for (const [x, y] of NAME_DIRECTIONS) { const c = clear(x, y); if (c > best + 1e-9) { best = c; [lx, ly] = [x, y]; } }
    }
    glyphs.set(id, placeGlyph(g, nx + 0, ny + 0, lx + 0, ly + 0));
  }
  return glyphs;
}

/**
 * The points a route is drawn with when an end is a glyph: a straight route (two points) runs from the middle of one
 * dot to the middle of the other, so lines placed on a lattice stay octilinear; a longer route gets a connector from its
 * end to the dot's middle. Unchanged when neither end is a glyph.
 */
export function routeToGlyphs(points, sourceGlyph, targetGlyph) {
  if (!points?.length || (!sourceGlyph && !targetGlyph)) return points;
  const centre = g => ({x: g.cx, y: g.cy});
  if (points.length === 2) return [sourceGlyph ? centre(sourceGlyph) : points[0], targetGlyph ? centre(targetGlyph) : points[1]];
  return [...(sourceGlyph ? [centre(sourceGlyph)] : []), ...points, ...(targetGlyph ? [centre(targetGlyph)] : [])];
}

const kebab = name => name.replace(/[A-Z]/g, c => '-' + c.toLowerCase());
/**
 * The theme tokens a page sets as CSS custom properties on `element` (or inherits): `--arqlet-<token in kebab case>`,
 * for example `--arqlet-leaf-fill` or `--arqlet-route`. Returns only the tokens that are set, for `{theme}` of a sheet.
 */
export function themeFromCss(element, view = globalThis) {
  const computed = view.getComputedStyle?.(element);
  if (!computed) return {};
  const out = {};
  for (const token of Object.keys(DEFAULT_THEME)) {
    const value = computed.getPropertyValue(`--arqlet-${kebab(token)}`).trim();
    if (value) out[token] = value;
  }
  return out;
}

/** Built-in sheets for hosts and the playground. */
export const STYLE_PRESETS = Object.freeze({
  default: {},
  // Routes and arrowheads coloured by the edge's kind, leaf outlines by the node's kind.
  byKind: {rules: [{selector: 'edge', style: {stroke: 'auto', arrow: 'auto'}}, {selector: 'leaf', style: {stroke: 'auto'}}]},
  // Metro map (with explicit buses, busStyle 'lines'): every bus a coloured line without arrowheads, the nodes on one
  // line ticks across it in its colour, interchanges (on several buses) and branching stations white pills across their
  // lines; on a light background.
  metro: {
    palette: ['#e32017', '#0098d4', '#00782a', '#00a4a7', '#9b0056', '#ffd300', '#894e24', '#ee7c0e', '#6950a1', '#95cdba'],
    theme: {frameFill: '#f7f7f5', frameStroke: '#d0d0cc', title: '#55554f', leafFill: '#ffffff', leafStroke: '#9a9a94', text: '#1b1b1b', row: '#55554f',
      selectedFill: '#fff6d6', selectedStroke: '#1b1b1b', route: '#9a9a94', arrow: '#9a9a94', labelFill: '#ffffff', labelText: '#1b1b1b',
      junctionFill: '#ffffff', junctionStroke: '#1b1b1b', hover: '#0019a8', trace: '#e32017', pin: '#ee7c0e', crossing: '#ee7c0e'},
    rules: [
      {selector: 'edge[bus]', style: {stroke: 'auto(bus)', arrow: 'none', width: 5, opacity: 1}},
      {selector: 'leaf[bus]', style: {fill: '#ffffff', stroke: 'auto(bus)', strokeWidth: 4, text: '#1b1b1b', radius: 'pill', glyph: 'tick', glyphSize: 9}},
      // A station where its line branches, or a gateway, and an interchange: a white pill across the lines.
      {selector: 'leaf[branch]', style: {fill: '#ffffff', stroke: '#1b1b1b', strokeWidth: 3, text: '#1b1b1b', glyph: 'pill', glyphSize: 11}},
      {selector: 'leaf[bus][kind=gateway]', style: {fill: '#ffffff', stroke: '#1b1b1b', strokeWidth: 3, text: '#1b1b1b', glyph: 'pill', glyphSize: 11}},
      {selector: 'leaf[interchange]', style: {fill: '#ffffff', stroke: '#1b1b1b', strokeWidth: 3, text: '#1b1b1b', glyph: 'pill', glyphSize: 11}},
    ],
  },
  // A light theme (the host's background must be light too).
  light: {theme: {
    frameFill: '#f3f6fa', frameStroke: '#b8c4d2', title: '#3d5470', leafFill: '#ffffff', leafStroke: '#8a9bb0', text: '#1b2a3a', row: '#3d5470',
    selectedFill: '#dff3ea', selectedStroke: '#2f8f6b', invalid: '#c9473a', route: '#4f8f7e', activeRoute: '#1f5f4e', arrow: '#4f8f7e',
    bus: '#6f93b8', busText: '#3d5470', hubFill: '#ffffff', hubStroke: '#8a9bb0', labelFill: '#ffffff', labelText: '#1b2a3a',
    crossing: '#c9922a', pin: '#c9922a', control: '#6b7f96', controlIcon: '#3d5470', hover: '#2f6fd0', trace: '#d98a00'}},
});
