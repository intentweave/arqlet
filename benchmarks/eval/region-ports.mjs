// Usage: node benchmarks/eval/region-ports.mjs [--json] [--variants a,b]; needs target/release/layout-command.
// Regions of other strategies (organic, tree, radial) inside a layered parent (ADR 0020, step S1-2): synthetic, deterministic
// graphs (seeded generator below) whose root flows left to right through layered stages and region containers, with many
// edges across the regions' borders. Measured per variant: lines through nodes (shape-aware), crossings, bends, area,
// whether single pass ran, and the edges that cross a region's border on a side other than the one the parent's flow
// enters (targets) or leaves (sources) — the border contract of ADR 0017.
import {arqlet, normalizeArqlet, metrics} from './lib.mjs';

function random(seed) {
  let s = BigInt(seed);
  return () => {
    s = (s * 6364136223846793005n + 1442695040888963407n) & 0xffffffffffffffffn;
    return Number(s >> 33n) / 2 ** 31;
  };
}

/** A layered root (lr) of `stages` stages; between them region containers of the given strategies, each with `size`
 *  children wired by its own shape; edges from every stage into the next region and from the region on. */
export function regions({seed, strategies, size, stageWidth}) {
  const rand = random(seed);
  const nodes = [], edges = [], containerStrategies = {}, containerDirections = {}, containerRouting = {};
  let k = 0;
  const edge = (source, target) => edges.push({id: `e${k++}`, kind: 'calls', source, target});
  const stage = i => Array.from({length: stageWidth}, (_, j) => {
    const id = `s${i}.${j}`;
    nodes.push({id, kind: 'svc', label: id});
    return id;
  });
  let previous = stage(0);
  strategies.forEach((strategy, r) => {
    const box = `r${r}`;
    nodes.push({id: box, kind: 'group', label: `${strategy} ${r}`});
    containerStrategies[box] = strategy;
    if (strategy !== 'tree') containerDirections[box] = 'auto';
    if (strategy !== 'tree') containerRouting[box] = 'straight';
    const inner = Array.from({length: size}, (_, j) => {
      const id = `${box}.${j}`;
      nodes.push({id, kind: 'fn', label: id, parentId: box, ...(strategy === 'radial' && j === 0 ? {shape: 'circle'} : {})});
      return id;
    });
    // The region's own edges: a tree (tree, radial) or a sparse mesh (organic).
    for (let j = 1; j < size; j++) edge(inner[strategy === 'organic' ? Math.floor(rand() * j) : Math.floor((j - 1) / 2)], inner[j]);
    if (strategy === 'organic') for (let j = 0; j < size / 2; j++) {
      const a = Math.floor(rand() * size), b = Math.floor(rand() * size);
      if (a !== b) edge(inner[a], inner[b]);
    }
    // Into the region from the stage before, and out of it into the next stage.
    for (const s of previous) edge(s, inner[Math.floor(rand() * size)]);
    const next = stage(r + 1);
    for (const t of next) edge(inner[Math.floor(rand() * size)], t);
    // A back edge (a cycle through the region) and a stage edge that passes it.
    edge(next[0], inner[Math.floor(rand() * size)]);
    edge(previous[0], next[next.length - 1]);
    previous = next;
  });
  return {nodes, edges, defaultDirection: 'lr', containerStrategies, containerDirections, containerRouting};
}

export const SHAPES = {
  'organic-only': {seed: 1, strategies: ['organic', 'organic'], size: 8, stageWidth: 3},
  'tree-only': {seed: 2, strategies: ['tree', 'tree'], size: 7, stageWidth: 3},
  'radial-only': {seed: 3, strategies: ['radial', 'radial'], size: 7, stageWidth: 3},
  'mixed-chain': {seed: 4, strategies: ['organic', 'tree', 'radial', 'organic'], size: 9, stageWidth: 4},
};

const ALL = {default: {}, singlePassMixed: {singlePassMixed: true}, regionPorts: {regionPorts: true},
  'singlePassMixed + regionPorts': {singlePassMixed: true, regionPorts: true}};

/** Edges crossing a region's border, and how many of them do it on a side other than the parent's flow expects. */
export function borderSides(graph, out) {
  const region = new Map(graph.nodes.filter(n => graph.containerStrategies?.[n.parentId]).map(n => [n.id, n.parentId]));
  const at = new Map(out.nodes.map(n => [n.id, n]));
  let crossing = 0, wrong = 0;
  for (const e of out.edges) {
    for (const [end, enters] of [[e.target, true], [e.source, false]]) {
      const box = region.get(end);
      if (!box || region.get(enters ? e.source : e.target) === box) continue;
      crossing++;
      const b = at.get(box), pts = enters ? e.points : [...e.points].reverse();
      // The last point outside the box before the route stays inside: its side is where the route crosses.
      let side = null;
      for (let i = pts.length - 1; i > 0; i--) {
        const p = pts[i - 1];
        const inside = p.x > b.x + 0.5 && p.x < b.x + b.w - 0.5 && p.y > b.y + 0.5 && p.y < b.y + b.h - 0.5;
        if (!inside) { side = p.x <= b.x + 0.5 ? 'left' : p.x >= b.x + b.w - 0.5 ? 'right' : p.y <= b.y + 0.5 ? 'top' : 'bottom'; break; }
      }
      // With the root flowing left to right, an edge enters a region from the left and leaves it to the right (back edges
      // the other way round).
      const back = e.source.startsWith('s') && e.target.startsWith('r') && Number(e.source.slice(1).split('.')[0]) > Number(e.target.slice(1).split('.')[0]);
      const expected = enters !== back ? 'left' : 'right';
      if (side !== expected) wrong++;
    }
  }
  return {crossing, wrong};
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const pick = process.argv.find(a => a.startsWith('--variants='))?.slice(11).split(',');
  const variants = Object.entries(ALL).filter(([name]) => !pick || pick.includes(name));
  const rows = [];
  for (const [shape, spec] of Object.entries(SHAPES)) {
    const graph = regions(spec);
    for (const [variant, options] of variants) {
      const {snapshot, ms} = arqlet({op: 'import', graph, limit: graph.nodes.length, options});
      const out = normalizeArqlet(snapshot);
      const m = metrics(out);
      const sides = borderSides(graph, out);
      rows.push({shape, variant, nodes: graph.nodes.length, edges: graph.edges.length, lineNodeHits: m.lineNodeHits,
        crossings: m.crossings, bends: m.bends, areaK: Math.round(m.area / 1e3), borderEdges: sides.crossing, offSide: sides.wrong,
        singlePass: (snapshot.layoutDiagnostics ?? []).some(d => d.startsWith('Single pass:')), ms: Math.round(ms),
        geometry: snapshot.geometryIssues.length});
    }
  }
  if (process.argv.includes('--json')) for (const r of rows) console.log(JSON.stringify(r));
  else {
    console.log('| shape | variant | nodes/edges | line–node hits | crossings | bends | area (k) | border ends off side | single pass | geometry issues | ms |');
    console.log('|---|---|---|---|---|---|---|---|---|---|---|');
    for (const r of rows) console.log(`| ${r.shape} | ${r.variant} | ${r.nodes}/${r.edges} | ${r.lineNodeHits} | ${r.crossings} | ${r.bends} | ${r.areaK} | ${r.offSide}/${r.borderEdges} | ${r.singlePass ? 'yes' : 'no'} | ${r.geometry} | ${r.ms} |`);
  }
}
