// Usage: node benchmarks/eval/pack-components.mjs [--json]; needs target/release/layout-command.
// Experimental `packComponents` against the default, `componentPacking` and `isolatedGrid` on synthetic shapes that
// stand for the packing problems seen in an external evaluation (deterministic, seed in the generators):
//   mixed      – many small components of different sizes and lone nodes side by side (a mixed graph);
//   lone       – only nodes without edges;
//   leaves     – a container whose children are mostly leaves plus one connected part (what `reveal` opens);
//   wide       – one container holding many small trees (what opening a large container shows).
// Prints per shape and variant: width × height, aspect, area (M), crossings, bends, lines through nodes, overlaps, ms.
// The last variant is elk-rs layered (its default `separateConnectedComponents: true`), through the shared ELK adapter.
import {arqlet, normalizeArqlet, metrics, round} from './lib.mjs';
import {elkLayout} from './elk-adapter.mjs';
import ElkRs from '@openedges/elk-layout';

function random(seed) {
  let s = BigInt(seed);
  return () => {
    s = (s * 6364136223846793005n + 1442695040888963407n) & 0xffffffffffffffffn;
    return Number(s >> 33n) / 2 ** 31;
  };
}

/** Components as small trees: `sizes` nodes each, edges from a random earlier node of the same component. */
function trees(prefix, sizes, rand, parentId) {
  const nodes = [], edges = [];
  sizes.forEach((size, c) => {
    for (let k = 0; k < size; k++) {
      const id = `${prefix}${c}.${k}`;
      nodes.push({id, kind: 'fn', label: id, ...(parentId ? {parentId} : {})});
      if (k > 0) edges.push({id: `${id}-e`, kind: 'calls', source: `${prefix}${c}.${Math.floor(rand() * k)}`, target: id});
    }
  });
  return {nodes, edges};
}

function shapes() {
  const out = {};
  {
    const rand = random(1);
    const sizes = Array.from({length: 40}, () => 2 + Math.floor(rand() * rand() * 30));
    const g = trees('c', sizes, rand);
    for (let k = 0; k < 30; k++) g.nodes.push({id: `lone${k}`, kind: 'fn', label: `lone${k}`});
    out.mixed = g;
  }
  out.lone = {nodes: Array.from({length: 120}, (_, k) => ({id: `n${k}`, kind: 'fn', label: `n${k}`})), edges: []};
  {
    const rand = random(2);
    const g = trees('p', [25], rand, 'box');
    g.nodes.unshift({id: 'box', kind: 'package', label: 'box'});
    for (let k = 0; k < 80; k++) g.nodes.push({id: `leaf${k}`, kind: 'fn', label: `leaf${k}`, parentId: 'box'});
    out.leaves = g;
  }
  {
    const rand = random(3);
    const sizes = Array.from({length: 60}, () => 1 + Math.floor(rand() * 8));
    const g = trees('t', sizes, rand, 'big');
    g.nodes.unshift({id: 'big', kind: 'package', label: 'big'});
    // A few edges leave the container, as in a real view.
    g.nodes.push({id: 'outside', kind: 'fn', label: 'outside'});
    for (let c = 0; c < 60; c += 12) g.edges.push({id: `out${c}`, kind: 'calls', source: `t${c}.0`, target: 'outside'});
    out.wide = g;
  }
  return out;
}

const VARIANTS = {
  default: {},
  componentPacking: {componentPacking: true},
  isolatedGrid: {isolatedGrid: true},
  packComponents: {packComponents: {}},
  'packComponents 16:9': {packComponents: {aspect: 16 / 9}},
};

const rows = [];
for (const [name, graph] of Object.entries(shapes())) {
  for (const [variant, options] of Object.entries(VARIANTS)) {
    const {snapshot, ms} = arqlet({op: 'import', graph, limit: graph.nodes.length, options});
    const m = metrics(normalizeArqlet(snapshot));
    rows.push(row(name, variant, graph, m, ms));
  }
  const elk = await elkLayout({...graph, defaultDirection: 'lr'}, new ElkRs());
  rows.push(row(name, 'elk-rs layered', graph, metrics(elk), elk.ms));
  // Flat shapes: elk-rs with its components separated and packed (`separateConnectedComponents`), called directly.
  if (!graph.nodes.some(n => n.parentId)) {
    const started = performance.now();
    const laid = await new ElkRs().layout({
      id: 'root',
      layoutOptions: {'elk.algorithm': 'layered', 'elk.direction': 'RIGHT', 'elk.edgeRouting': 'ORTHOGONAL',
        'elk.separateConnectedComponents': 'true'},
      children: graph.nodes.map(n => ({id: n.id, width: 180, height: 54})),
      edges: graph.edges.map(e => ({id: e.id, sources: [e.source], targets: [e.target]})),
    });
    const ms = performance.now() - started;
    const at = new Map(laid.children.map(c => [c.id, c]));
    const out = {
      nodes: laid.children.map(c => ({id: c.id, parentId: null, container: false, x: c.x, y: c.y, w: c.width, h: c.height})),
      edges: (laid.edges ?? []).map(e => {
        const s = e.sections?.[0];
        const points = s ? [s.startPoint, ...(s.bendPoints ?? []), s.endPoint] : [];
        return {id: e.id, source: e.sources[0], target: e.targets[0], points};
      }),
    };
    if (out.nodes.every(n => at.has(n.id))) rows.push(row(name, 'elk-rs, components separated', graph, metrics(out), ms));
  }
}
function row(shape, variant, graph, m, ms) {
  return {shape, variant, nodes: graph.nodes.length, edges: graph.edges.length, width: round(m.width), height: round(m.height),
    aspect: round(m.width / m.height), areaM: round(m.area / 1e6), crossings: m.crossings, bends: m.bends,
    lineNodeHits: m.lineNodeHits, siblingOverlaps: m.siblingOverlaps, ms: Math.round(ms)};
}
if (process.argv.includes('--json')) {
  for (const r of rows) console.log(JSON.stringify(r));
} else {
  console.log('| shape | variant | width × height | aspect | area (M) | crossings | bends | line–node hits | overlaps | ms |');
  console.log('|---|---|---|---|---|---|---|---|---|---|');
  for (const r of rows) {
    console.log(`| ${r.shape} | ${r.variant} | ${r.width} × ${r.height} | ${r.aspect} | ${r.areaM} | ${r.crossings} | ${r.bends} | ${r.lineNodeHits} | ${r.siblingOverlaps} | ${r.ms} |`);
  }
}
