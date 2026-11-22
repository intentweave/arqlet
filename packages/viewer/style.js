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

const NODE_PROPERTIES = ['fill', 'stroke', 'strokeWidth', 'text', 'dash', 'radius'];
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
        node: n => { const b = nodeBuses.get(n.id); return node(b ? {...n, bus: b.size === 1 ? [...b][0] : undefined, buses: b.size, interchange: b.size > 1} : n); },
        edge: r => (routeBus.has(r.id) ? edge({...r, bus: routeBus.get(r.id)}) : edge(r)),
      };
      bound.set(scene, out);
      return out;
    },
  };
}

// The explicit buses of a scene: the bus of each route, and the buses each node is an end of.
function busData(scene) {
  const routeBus = new Map(), nodeBuses = new Map();
  (scene.buses ?? []).forEach((edges, b) => { for (const id of edges) if (!routeBus.has(id)) routeBus.set(id, b); });
  for (const r of scene.routes ?? []) {
    const b = routeBus.get(r.id);
    if (b === undefined) continue;
    for (const end of [r.source, r.target]) { if (!nodeBuses.has(end)) nodeBuses.set(end, new Set()); nodeBuses.get(end).add(b); }
  }
  return {routeBus, nodeBuses};
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
  // bus stations in its colour, a node on several buses an interchange; on a light background.
  metro: {
    palette: ['#e32017', '#0098d4', '#00782a', '#00a4a7', '#9b0056', '#ffd300', '#894e24', '#ee7c0e', '#6950a1', '#95cdba'],
    theme: {frameFill: '#f7f7f5', frameStroke: '#d0d0cc', title: '#55554f', leafFill: '#ffffff', leafStroke: '#9a9a94', text: '#1b1b1b', row: '#55554f',
      selectedFill: '#fff6d6', selectedStroke: '#1b1b1b', route: '#9a9a94', arrow: '#9a9a94', labelFill: '#ffffff', labelText: '#1b1b1b',
      junctionFill: '#ffffff', junctionStroke: '#1b1b1b', hover: '#0019a8', trace: '#e32017', pin: '#ee7c0e', crossing: '#ee7c0e'},
    rules: [
      {selector: 'edge[bus]', style: {stroke: 'auto(bus)', arrow: 'none', width: 5, opacity: 1}},
      {selector: 'leaf[bus]', style: {fill: '#ffffff', stroke: 'auto(bus)', strokeWidth: 3, text: '#1b1b1b', radius: 'pill'}},
      {selector: 'leaf[interchange]', style: {fill: '#ffffff', stroke: '#1b1b1b', strokeWidth: 4, text: '#1b1b1b', radius: 'pill'}},
    ],
  },
  // A light theme (the host's background must be light too).
  light: {theme: {
    frameFill: '#f3f6fa', frameStroke: '#b8c4d2', title: '#3d5470', leafFill: '#ffffff', leafStroke: '#8a9bb0', text: '#1b2a3a', row: '#3d5470',
    selectedFill: '#dff3ea', selectedStroke: '#2f8f6b', invalid: '#c9473a', route: '#4f8f7e', activeRoute: '#1f5f4e', arrow: '#4f8f7e',
    bus: '#6f93b8', busText: '#3d5470', hubFill: '#ffffff', hubStroke: '#8a9bb0', labelFill: '#ffffff', labelText: '#1b2a3a',
    crossing: '#c9922a', pin: '#c9922a', control: '#6b7f96', controlIcon: '#3d5470', hover: '#2f6fd0', trace: '#d98a00'}},
});
