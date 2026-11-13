import {test} from 'node:test';
import assert from 'node:assert/strict';
import {existsSync} from 'node:fs';
import {fileURLToPath} from 'node:url';
import {overviewModel, overviewBuffers, pickNode, viewOf, subtreeOf, structuredExcerpt, representationsOf, flowDirectionOf, displayName, resolvedEdgeIds, clipToFrame, BORDER_PREFIX, ownersOf, placeScene, pickExpanded, labelsOf, nameOf, chainOf, SHAPE_STRIDE, LINE_STRIDE, OUTLINES} from './overview.js';

// Two clusters: c (with leaves a, b) and d (with leaf e); edges a→b, b→a (a cycle) and a→e.
const overview = {
  schemaVersion: 1,
  ids: ['c', 'a', 'b', 'd', 'e'],
  parent: [-1, 0, 0, -1, 3],
  x: [-20, -24, -16, 20, 20], y: [0, 0, 0, 0, 0], r: [10, 4, 4, 8, 4],
  rootRadius: 30, edges: [1, 2, 2, 1, 1, 4], links: [1, 2, 2, 0, 3, 1],
};
const graph = {
  nodes: [{id: 'c', parentId: null}, {id: 'a', parentId: 'c'}, {id: 'b', parentId: 'c'}, {id: 'd', parentId: null}, {id: 'e', parentId: 'd'}],
  edges: [{id: 'ab', source: 'a', target: 'b'}, {id: 'ba', source: 'b', target: 'a'}, {id: 'ae', source: 'a', target: 'e'}, {id: 'open', source: 'a', target: null}],
};

test('the model indexes the hierarchy', () => {
  const m = overviewModel(overview);
  assert.deepEqual(m.roots, [0, 3]);
  assert.deepEqual(m.children[0], [1, 2]);
  assert.deepEqual([...m.depth], [0, 1, 1, 0, 1]);
  assert.deepEqual([...m.cluster], [0, 0, 0, 3, 3]);
  assert.deepEqual([...m.leaves], [2, 1, 1, 1, 1]);
  assert.throws(() => overviewModel({...overview, schemaVersion: 2}), /schema/);
});

test('under a single root, its children give the colours', () => {
  // root → c (a, b) and d (e): one root, as a repository's `.` directory.
  const single = {...overview, ids: ['root', ...overview.ids], parent: [-1, 0, 1, 1, 0, 4],
    x: [0, ...overview.x], y: [0, ...overview.y], r: [40, ...overview.r]};
  const m = overviewModel(single);
  assert.deepEqual(m.tops, [0, 1, 4]);
  assert.deepEqual([...m.cluster], [0, 1, 1, 1, 4, 4]);
  // A chain of single big directories is passed through: root → src → (c, d).
  const chain = overviewModel({...single, ids: ['root', 'src', ...overview.ids], parent: [-1, 0, 1, 2, 2, 1, 5],
    x: [0, 0, ...overview.x], y: [0, 0, ...overview.y], r: [50, 40, ...overview.r]});
  assert.deepEqual([...chain.cluster].slice(2), [2, 2, 2, 5, 5]);
});

test('buffers hold one quad per disc, containers first, and one line per edge in its direction', () => {
  const b = overviewBuffers(overviewModel(overview));
  assert.equal(b.count.discs, 5 * 6);
  // The first quad is a container (kind 1, a ring), the last a leaf (kind 0).
  assert.equal(b.discs[SHAPE_STRIDE - 1], 1);
  assert.equal(b.discs[b.discs.length - 1], 0);
  assert.equal(b.count.edges, 6);
  // The second edge runs from b (x -16) to a (x -24): direction is kept.
  assert.deepEqual([b.edges[2 * LINE_STRIDE], b.edges[3 * LINE_STRIDE]], [-16, -24]);
  assert.equal(b.count.front + b.count.back + b.count.routes, 0);
});

// A scene of cluster c laid out structured: a and b as boxes, a route a → b and one b → a.
const scene = {
  bounds: {x: 0, y: 0, width: 100, height: 40},
  nodes: [
    {id: 'a', label: 'a', kind: 'fn', container: false, bounds: {x: 0, y: 10, width: 30, height: 20}},
    {id: 'b', label: 'b', kind: 'fn', container: false, bounds: {x: 70, y: 10, width: 30, height: 20}},
  ],
  routes: [
    {id: 'ab', source: 'a', target: 'b', points: [{x: 30, y: 15}, {x: 70, y: 15}]},
    {id: 'ba', source: 'b', target: 'a', points: [{x: 70, y: 25}, {x: 30, y: 25}]},
  ],
  labels: [{id: 'a', kind: 'node', text: 'a', bounds: {x: 5, y: 15, width: 20, height: 10}}],
};

test('a structured cluster replaces its discs by its scene, inside its disc', () => {
  const m = overviewModel(overview);
  const expanded = new Map([[0, {scene}]]);
  assert.deepEqual([...ownersOf(m, expanded)], [-1, 0, 0, -1, -1]);
  const b = overviewBuffers(m, expanded);
  // c keeps its ring; a and b are boxes now; d and e stay discs.
  assert.equal(b.count.discs, 3 * 6);
  assert.equal(b.count.front, 2 * 6);
  // Two routes of one segment, an arrow at each target, and one where the leaving edge a → e meets e's disc.
  assert.equal(b.count.routes, 4);
  assert.equal(b.count.arrows, 9);
  // The scene fits the disc: every box corner lies inside it.
  const place = placeScene(m, 0, scene);
  for (const n of scene.nodes) for (const [x, y] of [[n.bounds.x, n.bounds.y], [n.bounds.x + n.bounds.width, n.bounds.y + n.bounds.height]]) {
    const [wx, wy] = place.apply([x, y]);
    assert.ok(Math.hypot(wx - overview.x[0], wy - overview.y[0]) <= overview.r[0]);
  }
  // Only a → e leaves the cluster; it starts at a's box now.
  assert.equal(b.count.edges, 2);
  const [ax, ay] = place.apply([15, 20]);
  assert.ok(Math.abs(b.edges[0] - ax) < 1e-4 && Math.abs(b.edges[1] - ay) < 1e-4);
  // The arrow of b → a points at a (its tip is the route's last point).
  const tip = place.apply([30, 25]);
  assert.ok(Math.abs(b.arrows[LINE_STRIDE * 3] - tip[0]) < 1e-4);
});

test('picking inside a structured cluster finds its boxes, and labels show when large enough', () => {
  const m = overviewModel(overview);
  const expanded = new Map([[0, {scene}]]);
  const [x, y] = placeScene(m, 0, scene).apply([15, 20]);
  assert.equal(pickExpanded(m, expanded, x, y).node.id, 'a');
  assert.equal(pickExpanded(m, expanded, overview.x[0], overview.y[0] + 9.5).node, null);
  assert.equal(pickExpanded(m, expanded, 20, 0), null);
  const near = labelsOf(m, expanded, {centreX: -20, centreY: 0, scale: 40}, 800, 800);
  assert.ok(near.some(l => l.text === 'a'));
  // b has no placed label: its name is written in its box.
  assert.ok(near.some(l => l.text === 'b' && l.kind === 'node'));
  assert.ok(near.some(l => l.kind === 'cluster'));
  assert.equal(labelsOf(m, expanded, {centreX: 0, centreY: 0, scale: 0.5}, 800, 800).length, 0);
});

test('picking finds the deepest disc and nothing outside', () => {
  const m = overviewModel(overview);
  assert.equal(pickNode(m, -24, 0), 1);
  assert.equal(pickNode(m, -20, 8), 0);
  assert.equal(pickNode(m, 0, 25), -1);
  const v = viewOf(m, 3, 200, 100);
  assert.equal(v.centreX, 20);
  assert.ok(Math.abs(v.scale - 0.92 * 100 / 16) < 1e-9);
});

test('a structured excerpt keeps the cluster and routes leaving edges to stand-ins, direction kept', () => {
  const m = overviewModel(overview);
  assert.deepEqual(subtreeOf(m, 0), ['c', 'a', 'b']);
  const {graph: excerpt, leaving, inner, standIns} = structuredExcerpt(graph, m, 'c');
  assert.deepEqual(excerpt.nodes.map(n => [n.id, n.parentId]), [['c', null], ['a', 'c'], ['b', 'c'], [BORDER_PREFIX + 'd', null]]);
  // a → e leaves to cluster d: it now runs a → border:d, under its own id.
  assert.deepEqual(excerpt.edges.map(e => [e.id, e.source, e.target]), [['ab', 'a', 'b'], ['ba', 'b', 'a'], ['ae', 'a', BORDER_PREFIX + 'd']]);
  assert.deepEqual([leaving, inner, standIns], [1, 2, 1]);
  assert.deepEqual(resolvedEdgeIds(graph), ['ab', 'ba', 'ae']);
});

test('a route is cut at the frame: the border port is where it crosses', () => {
  const frame = {x: 0, y: 0, width: 10, height: 10};
  // Leaving to the right: the part from the start to x = 10.
  assert.deepEqual(clipToFrame([[5, 5], [8, 5], [20, 5]], frame, false), {port: [10, 5], points: [[5, 5], [8, 5], [10, 5]]});
  // Coming in from above: the part from y = 0 to the end.
  assert.deepEqual(clipToFrame([[3, -9], [3, 4]], frame, true), {port: [3, 0], points: [[3, 0], [3, 4]]});
  assert.equal(clipToFrame([[1, 1], [2, 2]], frame, false), null);
});

test('the representation follows the user, then the rules of the data, then the zoom', () => {
  const m = overviewModel({...overview, kinds: ['dir', 'fn'], kind: [0, 1, 1, 0, 1]});
  m.overview.ids.forEach((_, i) => assert.equal(m.size[i], [2, 0, 0, 1, 0][i]));
  const far = {centreX: 0, centreY: 0, scale: 0.5}, near = {centreX: -20, centreY: 0, scale: 20};
  // Zoomed out, both clusters are less than 14 pixels: collapsed. Zoomed into c, it fills the view: structured.
  assert.deepEqual([...representationsOf(m, far, 400, 400).collapsed].sort(), [0, 3]);
  assert.deepEqual([...representationsOf(m, near, 400, 400).structured], [0]);
  // Only the cluster under the focus switches: with the focus on d, c stays organic although it is large on screen.
  assert.equal(representationsOf(m, near, 400, 400, {focus: [20, 0]}).structured.size, 0);
  // A cluster too large for the limit is never structured by the zoom.
  assert.equal(representationsOf(m, near, 400, 400, {limit: 1}).structured.size, 0);
  // Hysteresis: at 80 pixels (between half of 140 and 140), c is structured only when it is already shown so.
  const half = {...near, scale: 8};
  assert.equal(representationsOf(m, half, 400, 400).structured.size, 0);
  assert.deepEqual([...representationsOf(m, half, 400, 400, {current: new Set([0])}).structured], [0]);
  // A rule by kind beats the zoom; the user's decision beats the rule.
  assert.equal(representationsOf(m, near, 400, 400, {rules: {dir: 'organic'}}).structured.size, 0);
  assert.deepEqual([...representationsOf(m, near, 400, 400, {rules: {dir: 'organic'}, decisions: new Map([['c', 'structured']])}).structured], [0]);
  assert.deepEqual([...representationsOf(m, near, 400, 400, {decisions: new Map([['c', 'collapsed']])}).collapsed], [0]);
  // A level of detail collapses the clusters from that depth on, before the zoom; the user's decision opens one again.
  assert.deepEqual([...representationsOf(m, near, 400, 400, {levelOfDetail: {depth: 0}}).collapsed].sort(), [0, 3]);
  assert.deepEqual([...representationsOf(m, near, 400, 400, {levelOfDetail: {depth: 0}, decisions: new Map([['c', 'organic']])}).collapsed], [3]);
  assert.equal(representationsOf(m, near, 400, 400, {levelOfDetail: {depth: 1}}).structured.size, 1, 'deeper levels leave the top to the zoom');
  // By kind: the clusters of a kind listed are collapsed, the others left to the zoom.
  assert.deepEqual([...representationsOf(m, near, 400, 400, {levelOfDetail: {kinds: ['dir']}}).collapsed].sort(), [0, 3]);
  assert.equal(representationsOf(m, near, 400, 400, {levelOfDetail: {kinds: ['fn']}}).collapsed.size, 0);
  // Without the automatic policy everything without a decision or rule stays organic.
  const off = representationsOf(m, far, 400, 400, {auto: false});
  assert.equal(off.collapsed.size + off.structured.size, 0);
});

test('a collapsed cluster is one filled disc and its edges are bundled per pair', () => {
  const m = overviewModel(overview);
  const b = overviewBuffers(m, new Map(), new Set([0]));
  // c is one disc (kind 0, filled), a and b are gone; d and e stay.
  assert.equal(b.count.discs, 3 * 6);
  const kinds = [];
  for (let v = 0; v < b.count.discs; v += 6) kinds.push(b.discs[v * SHAPE_STRIDE + SHAPE_STRIDE - 1]);
  assert.ok(kinds.includes(0) && kinds.filter(k => k === 1).length === 1);
  // a → b and b → a are inside c: not drawn. a → e runs from c's centre to e, once.
  assert.equal(b.count.edges, 2);
  assert.deepEqual([b.edges[0], b.edges[LINE_STRIDE]], [overview.x[0], overview.x[4]]);
  assert.equal(pickNode(m, -24, 0, new Set([0])), 0);
});

test('labels do not overlap: the larger one wins', () => {
  // Two clusters side by side, close enough that their names at the top of the discs would collide.
  const close = {...overview, x: [-6, -8, -4, 6, 6], r: [10, 2, 2, 10, 2], rootRadius: 20};
  const m = overviewModel(close);
  const view = {centreX: 0, centreY: 0, scale: 4};
  const names = labelsOf(m, new Map(), view, 400, 400).filter(l => l.kind === 'cluster');
  assert.equal(names.length, 2, 'apart: both names');
  const nearer = overviewModel({...close, x: [-1, -3, 1, 1, 1], r: [10, 2, 2, 9.5, 2]});
  const one = labelsOf(nearer, new Map(), view, 400, 400).filter(l => l.kind === 'cluster');
  assert.deepEqual(one.map(l => l.text), ['c']);
});

test('a structured cluster flows towards the clusters its edges go to', () => {
  const m = overviewModel(overview);
  // c (left) has one edge out, a → e, to d on the right: left to right.
  assert.equal(flowDirectionOf(m, 0), 'lr');
  // d receives that edge from the left: it flows from the left as well.
  assert.equal(flowDirectionOf(m, 3), 'lr');
  // Mirrored, c flows right to left; with the target below, top to bottom.
  assert.equal(flowDirectionOf(overviewModel({...overview, x: [20, 16, 24, -20, -20]}), 0), 'rl');
  assert.equal(flowDirectionOf(overviewModel({...overview, x: [0, 0, 0, 0, 0], y: [-20, -20, -20, 20, 20]}), 0), 'tb');
  // An isolated cluster has no preference.
  assert.equal(flowDirectionOf(overviewModel({...overview, edges: [1, 2]}), 0), null);
  // The excerpt carries the direction.
  assert.equal(structuredExcerpt(graph, m, 'c', {direction: 'lr'}).graph.defaultDirection, 'lr');
});

test('a community the overview added becomes a container of the excerpt', () => {
  // c holds a community k (added by the overview, after the graph's nodes) which holds a and b.
  const withCommunity = {
    ...overview,
    ids: [...overview.ids, 'community:c:1.0'],
    parent: [-1, 5, 5, -1, 3, 0],
    x: [...overview.x, -20], y: [...overview.y, 0], r: [...overview.r, 8],
  };
  const m = overviewModel(withCommunity);
  const {graph: excerpt} = structuredExcerpt(graph, m, 'c');
  assert.deepEqual(excerpt.nodes.map(n => [n.id, n.parentId, n.kind]).slice(0, 4), [
    ['c', null, undefined], ['a', 'community:c:1.0', undefined], ['b', 'community:c:1.0', undefined], ['community:c:1.0', 'c', 'community'],
  ]);
  assert.equal(displayName('community:c:1.0'), '≈ 1.0');
  assert.equal(displayName('src/lib/a.rs'), 'a.rs');
});

const wasm = new URL('../engine-client/arqlet.wasm', import.meta.url);
test('the engine’s overview feeds the model and the drill-down lays out', {skip: !existsSync(fileURLToPath(wasm))}, async () => {
  const {loadEngine} = await import('../engine-client/engine.js');
  const run = await loadEngine(wasm);
  const nodes = [], edges = [];
  for (let c = 0; c < 4; c++) {
    nodes.push({id: `c${c}`, kind: 'dir', label: `c${c}`, parentId: null});
    for (let l = 0; l < 20; l++) {
      nodes.push({id: `c${c}/n${l}`, kind: 'fn', label: `n${l}`, parentId: `c${c}`});
      edges.push({id: `c${c}/e${l}`, kind: 'call', source: `c${c}/n${l}`, target: `c${c}/n${(l + 1) % 20}`});
    }
    edges.push({id: `x${c}`, kind: 'call', source: `c${c}/n0`, target: `c${(c + 1) % 4}/n0`});
  }
  const big = {nodes, edges};
  const result = run({op: 'overview', graph: big});
  assert.equal(result.ok, true, result.error);
  const m = overviewModel(result.overview);
  assert.equal(m.n, 84);
  assert.equal(result.overview.stats.overlaps, 0);
  assert.equal(pickNode(m, result.overview.x[1], result.overview.y[1]), 1);
  const {graph: excerpt, leaving, standIns} = structuredExcerpt(big, m, 'c0');
  assert.equal(leaving, 2);
  assert.equal(standIns, 2);
  const imported = run({op: 'import', graph: excerpt, limit: excerpt.nodes.length});
  assert.equal(imported.ok, true, imported.error);
  const scene = run({op: 'scene', snapshot: imported.snapshot});
  assert.equal(scene.ok, true, scene.error);
  // Drawn structured, both leaving edges get a border port on the cluster's box, and the overview edge starts there.
  const withIds = overviewModel(result.overview, resolvedEdgeIds(big));
  const c0 = withIds.index.get('c0');
  const b = overviewBuffers(withIds, new Map([[c0, {scene: scene.scene}]]));
  const frame = scene.scene.nodes.find(n => n.id === 'c0').bounds;
  const place = placeScene(withIds, c0, scene.scene);
  const k = resolvedEdgeIds(big).indexOf('x0');
  const legs = [];
  for (let v = 0; v < b.count.edges; v += 2) legs.push([b.edges[v * LINE_STRIDE], b.edges[v * LINE_STRIDE + 1]]);
  const [fx, fy] = place.invert(legs.find(([x, y]) => {
    const [sx, sy] = place.invert([x, y]);
    return Math.abs(sx - frame.x) < 1e-3 || Math.abs(sx - frame.x - frame.width) < 1e-3 || Math.abs(sy - frame.y) < 1e-3 || Math.abs(sy - frame.y - frame.height) < 1e-3;
  }) ?? [NaN, NaN]);
  assert.ok(Number.isFinite(fx) && Number.isFinite(fy), `no leg starts on the border of c0 (edge ${k})`);
  assert.equal(scene.scene.nodes.filter(n => n.id.startsWith('c0/')).length, 20);
});

test('the fisheye lens enlarges its middle, leaves its rim and outside in place, and can be inverted', async () => {
  const {lensAt, lensInverse} = await import('./overview.js');
  const lens = {x: 100, y: 100, radius: 50, magnification: 3};
  assert.deepEqual(lensAt(lens, 100, 100), [100, 100, 3], 'the focus stays, enlarged M times');
  assert.deepEqual(lensAt(lens, 200, 100), [200, 100, 1], 'outside: unchanged');
  const [rimX, , rimF] = lensAt(lens, 149.999, 100);
  assert.ok(Math.abs(rimX - 149.999) < 1e-3 && Math.abs(rimF - 1) < 1e-3, 'continuous at the rim');
  let last = 100;
  for (let d = 1; d < 50; d++) {
    const [x, y, f] = lensAt(lens, 100 + d, 100);
    assert.equal(y, 100);
    assert.ok(x > last && x <= 150, `monotone and inside the lens at ${d}`);
    assert.ok(f >= 1 && f <= 3);
    last = x;
    const [bx, by] = lensInverse(lens, x, y);
    assert.ok(Math.abs(bx - (100 + d)) < 1e-9 && Math.abs(by - 100) < 1e-9, `inverse at ${d}`);
  }
  const [dx, dy] = lensAt(lens, 110, 120);
  const back = lensInverse(lens, dx, dy);
  assert.ok(Math.abs(back[0] - 110) < 1e-9 && Math.abs(back[1] - 120) < 1e-9, 'inverse off the axis');
  for (const none of [null, {...lens, radius: 0}, {...lens, magnification: 1}]) {
    assert.deepEqual(lensAt(none, 120, 90), [120, 90, 1]);
    assert.deepEqual(lensInverse(none, 120, 90), [120, 90]);
  }
});

test('under the lens, labels move with what they name and texts too small before appear', () => {
  const m = overviewModel(overview);
  const expanded = new Map([[0, {scene}]]);
  const near = {centreX: -20, centreY: 0, scale: 40};
  const plain = labelsOf(m, expanded, near, 800, 800);
  assert.deepEqual(labelsOf(m, expanded, near, 800, 800, {lens: {x: -5000, y: -5000, radius: 100, magnification: 4}}), plain,
    'a lens elsewhere changes nothing');
  // Farther out the texts of the structured cluster are too small; a lens over the cluster shows them.
  const names = ls => ls.filter(l => l.kind !== 'cluster').map(l => l.text);
  // The largest scale at which they are gone.
  const scale = [20, 15, 10, 8, 6, 5, 4, 3].find(k => !names(labelsOf(m, expanded, {centreX: -20, centreY: 0, scale: k}, 800, 800)).length);
  const far = {centreX: -20, centreY: 0, scale};
  const [cx, cy] = [400 + (overview.x[0] - far.centreX) * far.scale, 400 + (overview.y[0] - far.centreY) * far.scale];
  const lensed = labelsOf(m, expanded, far, 800, 800, {lens: {x: cx, y: cy, radius: 300, magnification: 6}});
  assert.ok(names(lensed).includes('a'), JSON.stringify(lensed));
});

test('a node is named by its label, else by the last part of its id', () => {
  const m = overviewModel({...overview, labels: overview.ids.map((id, i) => (i === 0 ? 'Cluster A' : ''))});
  assert.equal(nameOf(m, 0), 'Cluster A');
  assert.equal(nameOf(m, 1), overview.ids[1].split('/').at(-1));
  assert.equal(nameOf(overviewModel(overview), 0), overview.ids[0].split('/').at(-1), 'without labels: the id');
});


test('an outline per kind travels as the tenths of the shape kind', () => {
  const m = overviewModel({...overview, kinds: ['dir', 'fn'], kind: [0, 1, 1, 0, 1]});
  const kinds = buffers => [...Array(buffers.count.discs / 6).keys()].map(k => buffers.discs[k * 6 * SHAPE_STRIDE + 10]);
  const plain = kinds(overviewBuffers(m));
  const shaped = kinds(overviewBuffers(m, new Map(), new Set(), -1, {shapes: {dir: 'square', fn: 'octagon'}}));
  assert.deepEqual(OUTLINES, ['circle', 'square', 'octagon', 'diamond']);
  // Containers (rings, kind 1) become squares (+0.1), leaves (fill, kind 0) octagons (+0.2); circles add nothing.
  assert.deepEqual(shaped.map((k, i) => Math.round((k - plain[i]) * 10)), plain.map(k => (k >= 1 ? 1 : 2)));
});

test('a chain of single children is drawn, named, picked and decided as one', () => {
  // f holds only p (same disc), p holds q and r; g is a container with one child of a smaller disc: no chain.
  const o = {
    schemaVersion: 1,
    ids: ['f', 'p', 'q', 'r', 'g', 'h'],
    labels: ['Core', 'Core', 'q', 'r', 'G', 'h'],
    parent: [-1, 0, 1, 1, -1, 4],
    x: [0, 0, -3, 3, 30, 30], y: [0, 0, 0, 0, 0, 0], r: [8, 8, 2, 2, 8, 4],
    rootRadius: 40, edges: [], links: [],
    kinds: ['package', 'module', 'leaf'], kind: [0, 1, 2, 2, 0, 2],
  };
  const m = overviewModel(o);
  assert.deepEqual([...m.chainTop], [0, 0, 2, 3, 4, 5]);
  assert.deepEqual(chainOf(m, 1), [0, 1]);
  assert.equal(nameOf(m, 0), 'Core', 'a name repeated in the chain is written once');
  assert.equal(nameOf(m, 4), 'G');
  assert.equal(nameOf({...m, overview: {...o, labels: ['Core', 'Util', 'q', 'r', 'G', 'h']}}, 0), 'Core › Util');
  // Picking on the ring of the chain gives its top; a leaf inside stays itself.
  assert.equal(pickNode(m, 0, 6), 0);
  assert.equal(pickNode(m, -3, 0), 2);
  // One ring for the chain: f's ring, not p's (5 discs of 6 nodes).
  assert.equal(overviewBuffers(m).count.discs / 6, 5);
  // A rule on the inner kind applies to the chain; its top carries the decision.
  const far = {centreX: 0, centreY: 0, scale: 10};
  assert.deepEqual([...representationsOf(m, far, 400, 400, {rules: {module: 'collapsed'}}).collapsed], [0]);
  // Opened by the level of detail, the whole chain opens: p is not collapsed again one level deeper.
  const opened = representationsOf(m, far, 400, 400, {levelOfDetail: {depth: 0}, decisions: new Map([['f', 'organic']])});
  assert.deepEqual([...opened.collapsed], [4]);
});
