import {test} from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {createStyle, autoColour, themeFromCss, DEFAULT_THEME, PALETTE, STYLE_PRESETS} from './style.js';
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
