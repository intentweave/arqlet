import {test} from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {createStyle, autoColour, themeFromCss, glyphOf, orientGlyphs, DEFAULT_THEME, PALETTE, STYLE_PRESETS} from './style.js';
import {sceneBuffers, parseColour, BOX_STRIDE, SEGMENT_STRIDE, TRIANGLE_STRIDE} from './scene-buffers.js';
import {renderSvg} from './svg.js';
import {toReactFlow} from './react-flow.js';

const scene = name => JSON.parse(readFileSync(new URL(`./fixtures/${name}.scene.json`, import.meta.url)));

test('rules match by type, data and id; later rules win; tokens and auto resolve', () => {
  const style = createStyle({theme: {route: '#123456'}, rules: [
    {selector: 'edge', style: {stroke: '$route', width: 3}},
    {selector: 'edge[kind=import]', style: {stroke: 'auto', dash: '4 2'}},
    {selector: 'leaf[kind=module]', style: {fill: '#ff0000'}},
    {selector: 'container', style: {stroke: '#00ff00'}},
    {selector: 'node#a', style: {fill: '#0000ff'}},
  ]});
  assert.deepEqual(style.edge({id: 'e', kind: 'call'}), {stroke: '#123456', width: 3});
  assert.deepEqual(style.edge({id: 'e', kind: 'import'}), {stroke: autoColour('import'), width: 3, dash: '4 2'});
  assert.deepEqual(style.node({id: 'm', kind: 'module'}), {fill: '#ff0000'});
  assert.deepEqual(style.node({id: 'm', kind: 'module', container: true}), {stroke: '#00ff00'}, 'leaf does not match a container');
  assert.deepEqual(style.node({id: 'a', kind: 'module'}), {fill: '#0000ff'}, 'the later rule wins');
  assert.throws(() => createStyle({rules: [{selector: 'node > edge', style: {}}]}), /Unsupported selector/);
});

test('auto colours are stable per kind and from the palette', () => {
  assert.equal(autoColour('call'), autoColour('call'));
  assert.ok(PALETTE.includes(autoColour('anything')));
  assert.equal(new Set(['call', 'import', 'inherits', 'uses', 'contains'].map(k => autoColour(k))).size >= 3, true);
});

test('the default style draws exactly as before: no rules, the default theme', () => {
  const s = createStyle();
  assert.equal(s.isDefault, true);
  assert.deepEqual(s.theme, DEFAULT_THEME);
  assert.equal(createStyle(s), s, 'a resolved style is passed through');
  assert.equal(createStyle(STYLE_PRESETS.byKind).isDefault, false);
});

test('CSS colours parse: hex in every length, rgb(), hsl()', () => {
  assert.deepEqual(parseColour('#f00'), [1, 0, 0, 1]);
  assert.deepEqual(parseColour('#ff000080').map(v => +v.toFixed(3)), [1, 0, 0, 0.502]);
  assert.deepEqual(parseColour('rgb(255, 0, 0)'), [1, 0, 0, 1]);
  assert.deepEqual(parseColour('rgba(0 0 255 / 50%)'), [0, 0, 1, 0.5]);
  assert.deepEqual(parseColour('hsl(120 100% 50%)').map(v => +v.toFixed(3)), [0, 1, 0, 1]);
});

test('the theme comes from CSS custom properties', () => {
  const view = {getComputedStyle: () => ({getPropertyValue: name => ({'--arqlet-leaf-fill': ' #abcdef ', '--arqlet-route': 'rgb(1, 2, 3)'})[name] ?? ''})};
  assert.deepEqual(themeFromCss({}, view), {leafFill: '#abcdef', route: 'rgb(1, 2, 3)'});
});

// A minimal DOM for renderSvg.
class El { constructor(tag) { this.tag = tag; this.children = []; this.attrs = {}; } append(...c) { this.children.push(...c); } replaceChildren() { this.children = []; } setAttribute(k, v) { this.attrs[k] = v; } }
const walk = (e, f) => { f(e); for (const c of e.children ?? []) walk(c, f); };

test('one style, every renderer: renderSvg, the GPU buffers and React Flow draw the same colours', () => {
  const s = scene('bpmn-pools');
  const leaf = s.nodes.find(n => !n.container);
  const sheet = {theme: {route: '#112233'}, rules: [{selector: `node#${leaf.id}`, style: {fill: '#aa0000', stroke: '#00bb00'}}, {selector: 'edge', style: {arrow: '#ccdd00'}}]};
  const host = new El('svg');
  renderSvg(host, s, {document: {createElementNS: (_, t) => new El(t)}, style: sheet});
  const lines = [], shapes = [], markers = [];
  walk(host, e => { if (e.tag === 'polyline' && e.attrs['data-route-id'] && e.attrs.class !== 'route-hit') lines.push(e); if (e.tag === 'marker') markers.push(e); if (e.tag === 'g' && e.attrs['data-node-id'] === leaf.id) shapes.push(e.children.find(c => (c.tag === 'rect' || c.tag === 'circle') && c.attrs.class !== 'lw-hit')); });
  assert.ok(lines.every(l => l.attrs.stroke === '#112233'), 'the theme route colour in SVG');
  assert.deepEqual([shapes[0].attrs.fill, shapes[0].attrs.stroke], ['#aa0000', '#00bb00']);
  assert.equal(markers.length, 2, 'the default marker and one for the styled arrowheads');
  assert.equal(markers[1].children[0].attrs.fill, '#ccdd00');
  const b = sceneBuffers(s, {style: sheet});
  const seg = b.layers[2].segments, tri = b.layers[2].triangles;
  assert.deepEqual([...seg.slice(9, 12)].map(v => Math.round(v * 255)), [0x11, 0x22, 0x33], 'the same route colour on the GPU');
  assert.deepEqual([...tri.slice(3, 6)].map(v => Math.round(v * 255)), [0xcc, 0xdd, 0x00], 'the same arrowhead colour');
  const leaves = b.layers[3].boxes, i = s.nodes.filter(n => !n.container).indexOf(leaf);
  assert.deepEqual([...leaves.slice(i * BOX_STRIDE + 8, i * BOX_STRIDE + 11)].map(v => Math.round(v * 255)), [0xaa, 0, 0]);
  const flow = toReactFlow(s, {style: sheet});
  assert.deepEqual([flow.nodes.find(n => n.id === leaf.id).data.look.fill, flow.edges[0].data.look.stroke, flow.edges[0].data.look.arrow], ['#aa0000', '#112233', '#ccdd00']);
  assert.equal(toReactFlow(s).edges[0].data.look, undefined, 'no look without a style');
});

test('a scene\'s buses become data for the rules: a bus per route, stations and interchanges, auto by bus', () => {
  const sc = {buses: [['e1', 'e2'], ['e3']], routes: [{id: 'e1', source: 'a', target: 'g'}, {id: 'e2', source: 'b', target: 'g'}, {id: 'e3', source: 'c', target: 'g'}, {id: 'x', source: 'a', target: 'c'}]};
  const style = createStyle({palette: ['#100000', '#200000', '#300000'], rules: [
    {selector: 'edge[bus]', style: {stroke: 'auto(bus)', arrow: 'none'}},
    {selector: 'leaf[bus]', style: {stroke: 'auto(bus)', radius: 'pill'}},
    {selector: 'leaf[interchange]', style: {stroke: '#000000'}},
  ]});
  const b = style.bind(sc);
  assert.deepEqual(b.edge({id: 'e1'}), {stroke: '#100000', arrow: 'none'});
  assert.deepEqual(b.edge({id: 'e3'}), {stroke: '#200000', arrow: 'none'}, 'bus 1 takes the second colour');
  assert.deepEqual(b.edge({id: 'x'}), {}, 'a route on no bus matches no [bus] rule');
  assert.deepEqual(b.node({id: 'a'}), {stroke: '#100000', radius: 'pill'});
  assert.deepEqual(b.node({id: 'g'}), {stroke: '#000000'}, 'on two buses: an interchange, on no single bus');
  assert.deepEqual(b.node({id: 'z'}), {});
  assert.equal(style.bind(sc), b, 'bound once per scene');
});

// The metro sheet with dots for the stations on one line (as before ticks).
const METRO_DOTS = {...STYLE_PRESETS.metro, rules: STYLE_PRESETS.metro.rules.map(r => (r.selector === 'leaf[bus]' ? {...r, style: {...r.style, glyph: 'dot', glyphSize: 7, strokeWidth: 3}} : r))};

test('metro: no arrowheads, stations as dots with their names outside, lines to the dots, in every renderer', () => {
  const s = scene('bpmn-pools');
  const routes = s.routes.slice(0, 2).map(r => r.id);
  const withBus = {...s, buses: [routes]};
  const host = new El('svg');
  renderSvg(host, withBus, {document: {createElementNS: (_, t) => new El(t)}, style: METRO_DOTS});
  const lines = [];
  walk(host, e => { if (e.tag === 'polyline' && routes.includes(e.attrs['data-route-id']) && e.attrs.class !== 'route-hit') lines.push(e); });
  assert.ok(lines.length === 2 && lines.every(l => !l.attrs['marker-end'] && l.attrs['stroke-width'] === 5), 'bus routes: thick, no marker');
  const station = s.routes.find(r => r.id === routes[0]).source;
  const box = s.nodes.find(n => n.id === station).bounds, cx = box.x + box.width / 2, cy = box.y + box.height / 2;
  let dot = null, label = null;
  walk(host, e => { if (e.tag === 'g' && e.attrs['data-node-id'] === station) { dot = e.children.find(c => c.tag === 'circle'); label = e.children.find(c => c.tag === 'text'); } });
  assert.deepEqual([dot.attrs.cx, dot.attrs.cy, dot.attrs.r], [cx, cy, 7], 'a station is a dot in the middle of its box');
  assert.ok(label.attrs.x > cx + 7 && label.attrs.y > cy + 7, 'its name outside, right of and below the dot');
  // The route reaches the dot's middle.
  const first = lines[0].attrs.points.split(' ')[0].split(',').map(Number);
  const r0 = s.routes.find(r => r.id === lines[0].attrs['data-route-id']);
  const ends = [r0.source, r0.target].map(id => s.nodes.find(n => n.id === id).bounds).map(b => [b.x + b.width / 2, b.y + b.height / 2]);
  assert.ok(ends.some(([x, y]) => Math.abs(first[0] - x) < 1e-9 && Math.abs(first[1] - y) < 1e-9), 'a line starts in a dot');
  const plain = sceneBuffers(s, {style: METRO_DOTS}), metro = sceneBuffers(withBus, {style: METRO_DOTS});
  assert.equal(plain.layers[2].triangles.length - metro.layers[2].triangles.length, 2 * 3 * TRIANGLE_STRIDE, 'two arrowheads fewer on the GPU');
  const flow = toReactFlow(withBus, {style: METRO_DOTS});
  assert.equal(flow.edges.find(e => e.id === routes[0]).data.look.arrow, 'none');
  assert.equal(flow.nodes.find(n => n.id === station).data.look.glyph, 'dot');
  assert.ok(flow.edges.find(e => e.id === routes[0]).data.look.points, 'React Flow gets the points to the dots');
  // The GPU buffers draw the station as a dot of radius 7 in the middle.
  const leaves = metro.layers[3].boxes, ids = metro.layers[3].ids, k = ids.indexOf(station);
  assert.deepEqual([...leaves.slice(k * BOX_STRIDE, k * BOX_STRIDE + 6)], [cx - 7, cy - 7, 14, 14, 7, 1]);
});

test('a tick stands across its line, to the right or up, its name beyond the tip', () => {
  const tick = {glyph: 'tick'};
  const at = (x, y) => ({x: x - 10, y: y - 10, width: 20, height: 20});
  const glyphs = new Map(['a', 'b', 'c', 'd', 'e'].map((id, k) => [id, glyphOf(tick, [at(0, 0), at(100, 0), at(200, 0), at(200, 100), at(300, 300)][k])]));
  // a — b — c across, c — d down; e has no routes.
  const scene = {routes: [
    {id: 'ab', source: 'a', target: 'b', points: [{x: 10, y: 0}, {x: 90, y: 0}]},
    {id: 'bc', source: 'b', target: 'c', points: [{x: 110, y: 0}, {x: 190, y: 0}]},
    {id: 'cd', source: 'c', target: 'd', points: [{x: 200, y: 10}, {x: 200, y: 90}]},
  ], buses: [['ab', 'bc', 'cd']]};
  assert.equal(orientGlyphs(glyphs, scene), glyphs, 'in place');
  const b = glyphs.get('b');
  assert.deepEqual([b.nx, b.ny, b.x2, b.y2, b.align, b.baseline], [0, -1, 100, -9, 'center', 'bottom'], 'a line across: the tick points up, the name centred above');
  assert.ok(b.labelY < b.y2 && b.labelX === 100);
  const d = glyphs.get('d');
  assert.deepEqual([d.nx, d.ny, d.x2, d.align, d.baseline], [1, 0, 209, 'left', 'middle'], 'the end of a line down: the tick points right, the name left-aligned beside it');
  assert.ok(d.labelX > d.x2);
  // c turns the corner: its tick stands across the diagonal between its neighbours, to the right.
  const c = glyphs.get('c');
  assert.ok(Math.abs(Math.hypot(c.nx, c.ny) - 1) < 1e-12 && c.nx > 0 && c.ny < 0);
  assert.deepEqual([glyphs.get('e').nx, glyphs.get('e').ny], [0, -1], 'no routes: up');
  // A dot stays as it was.
  assert.deepEqual(glyphOf({glyph: 'dot'}, at(0, 0)), {kind: 'dot', cx: 0, cy: 0, r: 7, labelX: 10, labelY: 18});
});

test('metro: a station on one line is a tick in its line\'s colour in every renderer', () => {
  const s = scene('bpmn-pools');
  const routes = s.routes.slice(0, 2).map(r => r.id);
  const withBus = {...s, buses: [routes]};
  const ends = new Map();
  for (const r of s.routes.filter(r => routes.includes(r.id))) for (const id of [r.source, r.target]) ends.set(id, (ends.get(id) ?? 0) + 1);
  const station = [...ends.keys()][0];
  const box = s.nodes.find(n => n.id === station).bounds, cx = box.x + box.width / 2, cy = box.y + box.height / 2;
  const colour = autoColour(0, STYLE_PRESETS.metro.palette);
  const host = new El('svg');
  renderSvg(host, withBus, {document: {createElementNS: (_, t) => new El(t)}, style: STYLE_PRESETS.metro});
  let line = null, label = null;
  walk(host, e => { if (e.tag === 'g' && e.attrs['data-node-id'] === station) { line = e.children.find(c => c.tag === 'line'); label = e.children.find(c => c.tag === 'text'); } });
  assert.ok(line, 'a tick, not a dot');
  assert.deepEqual([line.attrs.x1, line.attrs.y1, line.attrs.stroke, line.attrs['stroke-width']], [cx, cy, colour, 4]);
  assert.ok(Math.abs(Math.hypot(line.attrs.x2 - cx, line.attrs.y2 - cy) - 9) < 1e-9, 'nine units long');
  // Across its line: perpendicular to where its route leaves the station.
  const glyphs = new Map([[station, glyphOf({glyph: 'tick'}, box)]]);
  orientGlyphs(glyphs, withBus);
  const g = glyphs.get(station);
  assert.deepEqual([line.attrs.x2, line.attrs.y2], [g.x2, g.y2]);
  assert.ok(label.attrs['text-anchor'] && label.attrs.x === g.labelX && label.attrs.y === g.labelY, 'its name beyond the tip');
  // The GPU layer: a segment from the middle to the tip, an empty box with the id.
  const metro = sceneBuffers(withBus, {style: STYLE_PRESETS.metro}), leaves = metro.layers[3];
  const k = leaves.ids.indexOf(station);
  assert.ok(k >= 0 && leaves.boxes[k * BOX_STRIDE + 6] === 0, 'an empty box carries the id');
  let found = false;
  for (let i = 0; i < leaves.segments.length; i += SEGMENT_STRIDE) {
    const seg = [...leaves.segments.slice(i, i + 5)].map(v => Math.round(v * 1000) / 1000);
    if (seg[0] === Math.round(cx * 1000) / 1000 && seg[1] === Math.round(cy * 1000) / 1000) { found = true; assert.deepEqual(seg.slice(2), [g.x2, g.y2, 4].map(v => Math.round(v * 1000) / 1000)); }
  }
  assert.ok(found, 'the tick as a segment on the GPU');
  const text = metro.texts.find(t => t.text === s.nodes.find(n => n.id === station).label.slice(0, 32) && t.x === g.labelX);
  assert.ok(text && text.align === g.align && text.baseline === g.baseline);
  // React Flow: the tick relative to the node's box.
  const look = toReactFlow(withBus, {style: STYLE_PRESETS.metro}).nodes.find(n => n.id === station).data.look;
  assert.equal(look.glyph, 'tick');
  assert.deepEqual([look.tick.x1, look.tick.y1, look.tick.x2, look.tick.y2], [cx - box.x, cy - box.y, g.x2 - box.x, g.y2 - box.y]);
});

test('a pill lies across the main axis of its lines; a branching station is marked `branch`', () => {
  const at = (x, y) => ({x: x - 10, y: y - 10, width: 20, height: 20});
  const route = (id, a, b, from, to) => ({id, source: a, target: b, points: [from, to]});
  // g where a line from the left goes on to the right and branches down: a T.
  const glyphs = new Map([['g', glyphOf({glyph: 'pill'}, at(100, 100))], ['x', glyphOf({glyph: 'pill'}, at(400, 100))]]);
  assert.deepEqual([glyphs.get('g').thickness, glyphs.get('g').length, glyphs.get('g').width, glyphs.get('g').r], [11, 26.4, 3, 5.5]);
  const scene = {routes: [
    route('l', 'a', 'g', {x: 0, y: 100}, {x: 90, y: 100}), route('r', 'g', 'b', {x: 110, y: 100}, {x: 200, y: 100}), route('d', 'g', 'c', {x: 100, y: 110}, {x: 100, y: 200}),
    // x: a crossing, four ways; balanced, so the first route's line decides.
    route('x1', 'p', 'x', {x: 300, y: 100}, {x: 390, y: 100}), route('x2', 'x', 'q', {x: 410, y: 100}, {x: 500, y: 100}),
    route('x3', 's', 'x', {x: 400, y: 0}, {x: 400, y: 90}), route('x4', 'x', 't', {x: 400, y: 110}, {x: 400, y: 200}),
  ], buses: [['l', 'r', 'd'], ['x1', 'x2', 'x3', 'x4']]};
  orientGlyphs(glyphs, scene);
  const g = glyphs.get('g');
  assert.deepEqual([g.nx, g.ny, g.x2, g.y2, g.align, g.baseline], [0, -1, 100, 100 - 13.2, 'center', 'bottom'], 'the through line runs across: the pill stands up, its name above');
  const x = glyphs.get('x');
  assert.deepEqual([x.nx, x.ny], [0, -1]);
  // Lines leave the crossing up and down too: its name goes between them, to the upper right, beyond the pill's corner.
  assert.deepEqual([x.align, x.baseline], ['left', 'bottom']);
  assert.ok(x.labelX > 400 && x.labelY < 100);
  // A branch whose lines leave up, down and diagonally: the main axis turns to the nearest 45° (down), the pill lies flat, its name to the left, where no line leaves.
  const branch = new Map([['y', glyphOf({glyph: 'pill'}, at(100, 400))]]);
  orientGlyphs(branch, {routes: [route('u', 'n', 'y', {x: 100, y: 300}, {x: 100, y: 390}), route('v', 'y', 's', {x: 100, y: 410}, {x: 100, y: 500}), route('w', 'y', 'e', {x: 107, y: 407}, {x: 200, y: 500})]});
  assert.deepEqual([branch.get('y').nx, branch.get('y').ny, branch.get('y').align], [-1, 0, 'right']);
  // The derived data: g has three routes of one bus.
  const looks = createStyle({rules: [{selector: 'leaf[branch]', style: {glyph: 'pill'}}]}).bind({...scene, nodes: []});
  assert.equal(looks.node({id: 'g'}).glyph, 'pill');
  assert.equal(looks.node({id: 'a'}).glyph, undefined);
});

test('metro: an interchange is a white pill across its lines in every renderer', () => {
  const s = scene('bpmn-pools');
  // Two routes that share a node, on two buses: that node is an interchange.
  let pair = null;
  for (const a of s.routes) { for (const b of s.routes) { if (a !== b && !pair && [b.source, b.target].includes(a.source)) pair = [a, b]; } }
  assert.ok(pair, 'two routes meeting at a node');
  const station = pair[0].source, withBus = {...s, buses: [[pair[0].id], [pair[1].id]]};
  const box = s.nodes.find(n => n.id === station).bounds, cx = box.x + box.width / 2, cy = box.y + box.height / 2;
  const glyphs = new Map([[station, glyphOf({glyph: 'pill'}, box)]]);
  orientGlyphs(glyphs, withBus);
  const g = glyphs.get(station);
  const host = new El('svg');
  renderSvg(host, withBus, {document: {createElementNS: (_, t) => new El(t)}, style: STYLE_PRESETS.metro});
  let pill = null;
  walk(host, e => { if (e.tag === 'g' && e.attrs['data-node-id'] === station) pill = e.children.find(c => c.tag === 'rect' && c.attrs.rx); });
  assert.ok(pill, 'a pill');
  assert.deepEqual([pill.attrs.width, pill.attrs.height, pill.attrs.rx, pill.attrs.fill, pill.attrs.stroke, pill.attrs['stroke-width']], [26.4, 11, 5.5, '#ffffff', '#1b1b1b', 3]);
  assert.ok(pill.attrs.transform.startsWith('rotate(') && pill.attrs.transform.endsWith(` ${cx} ${cy})`));
  // The GPU layer: triangles around the middle in the border's colour, then in white; an empty box with the id.
  const leaves = sceneBuffers(withBus, {style: STYLE_PRESETS.metro}).layers[3];
  const k = leaves.ids.indexOf(station);
  assert.ok(k >= 0 && leaves.boxes[k * BOX_STRIDE + 6] === 0);
  const fans = [];
  for (let i = 0; i < leaves.triangles.length; i += TRIANGLE_STRIDE) if (leaves.triangles[i] === Math.fround(cx) && leaves.triangles[i + 1] === Math.fround(cy)) fans.push([...leaves.triangles.slice(i + 3, i + 7)].map(v => Math.round(v * 100) / 100));
  assert.equal(fans.length, 2 * 26, 'two fans of 26 triangles');
  assert.deepEqual(fans[0], [0.11, 0.11, 0.11, 1]);
  assert.deepEqual(fans.at(-1), [1, 1, 1, 1]);
  // Its outline reaches half the length along the normal.
  let far = 0;
  for (let i = 0; i < leaves.triangles.length; i += TRIANGLE_STRIDE) if (leaves.triangles[i + 3] < 0.5) far = Math.max(far, (leaves.triangles[i] - cx) * g.nx + (leaves.triangles[i + 1] - cy) * g.ny);
  assert.ok(Math.abs(far - 13.2) < 1e-3, String(far));
  const look = toReactFlow(withBus, {style: STYLE_PRESETS.metro}).nodes.find(n => n.id === station).data.look;
  assert.equal(look.glyph, 'pill');
  assert.deepEqual([look.pill.cx, look.pill.cy, look.pill.length, look.pill.thickness], [cx - box.x, cy - box.y, 26.4, 11]);
});
