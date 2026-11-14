// Usage: node benchmarks/eval/region-child-ports.mjs [--json]; needs target/release/layout-command.
// Layered containers inside an organic root (ADR 0020, step S1-3, `regionChildPorts`): synthetic, deterministic graphs
// (generator and seeds below: containers of both axes, optionally with a nested container of the other axis, loose nodes,
// random edges inside the containers and between everything) and the mixed playground examples with an organic root.
// Measured per variant: lines through nodes (shape-aware), crossings, bends, the geometry issues by kind, and the ends of
// edges that leave a layered child of the root on a side across its flow's axis.
import {readFileSync, existsSync} from 'node:fs';
import {arqlet, normalizeArqlet, metrics, ROOT} from './lib.mjs';

export function organicRoot({containers = 3, size = 5, nested = false, seed = 7, loose = 4}) {
  let s = seed;
  const rand = () => (s = (s * 1103515245 + 12345) % 2147483648) / 2147483648;
  const nodes = [], edges = [], containerDirections = {};
  let k = 0;
  const groups = [];
  for (let c = 0; c < containers; c++) {
    const box = `c${c}`;
    nodes.push({id: box, kind: 'group', label: box});
    containerDirections[box] = c % 2 ? 'tb' : 'lr';
    if (nested) {
      nodes.push({id: `${box}.sub`, kind: 'group', label: `${box}.sub`, parentId: box});
      containerDirections[`${box}.sub`] = c % 2 ? 'lr' : 'tb';
    }
    const inner = [];
    for (let j = 0; j < size; j++) {
      const id = `${box}.n${j}`;
      nodes.push({id, kind: 'fn', label: id, parentId: nested && j >= size - 2 ? `${box}.sub` : box});
      if (j) edges.push({id: `e${k++}`, kind: 'calls', source: inner[Math.floor(rand() * j)], target: id});
      inner.push(id);
    }
    groups.push(inner);
  }
  for (let j = 0; j < loose; j++) {
    nodes.push({id: `free${j}`, kind: 'fn', label: `free${j}`});
    groups.push([`free${j}`]);
  }
  for (let j = 0; j < containers * 3; j++) {
    const a = groups[Math.floor(rand() * groups.length)], b = groups[Math.floor(rand() * groups.length)];
    if (a === b) continue;
    edges.push({id: `x${k++}`, kind: 'calls', source: a[Math.floor(rand() * a.length)], target: b[Math.floor(rand() * b.length)]});
  }
  return {nodes, edges, defaultDirection: 'auto', defaultStrategy: 'organic', defaultRouting: 'straight', containerDirections};
}

/** Ends of edges leaving a layered child of the root (the outermost container of an end) across its flow's axis. */
export function offAxis(graph, out) {
  const parent = new Map(graph.nodes.map(n => [n.id, n.parentId ?? null]));
  const top = id => { let c = null; for (let a = parent.get(id); a; a = parent.get(a)) if (parent.get(a) === null) c = a; return c; };
  const axis = c => ({lr: 'x', rl: 'x', tb: 'y', bt: 'y'})[graph.containerDirections?.[c]];
  const at = new Map(out.nodes.map(n => [n.id, n]));
  let ends = 0, off = 0;
  for (const e of out.edges) for (const [end, other, leaving] of [[e.source, e.target, true], [e.target, e.source, false]]) {
    const c = top(end);
    if (!c || !axis(c) || top(other) === c) continue;
    ends++;
    const b = at.get(c), pts = leaving ? e.points : [...e.points].reverse();
    for (let i = 1; i < pts.length; i++) {
      const p = pts[i], q = pts[i - 1];
      const inside = p.x > b.x + 0.5 && p.x < b.x + b.w - 0.5 && p.y > b.y + 0.5 && p.y < b.y + b.h - 0.5;
      if (inside) continue;
      const hits = [[(b.x - q.x) / (p.x - q.x), 'x'], [(b.x + b.w - q.x) / (p.x - q.x), 'x'], [(b.y - q.y) / (p.y - q.y), 'y'], [(b.y + b.h - q.y) / (p.y - q.y), 'y']]
        .filter(([v]) => v >= -1e-6 && v <= 1 + 1e-6 && Number.isFinite(v)).sort((u, v) => u[0] - v[0]);
      if (!hits.length || hits[0][1] !== axis(c)) off++;
      break;
    }
  }
  return {ends, off};
}

const VARIANTS = {
  'straightDetours': {straightDetours: true},
  'straightDetours + regionChildPorts': {straightDetours: true, regionChildPorts: true},
  'singlePassMixed + straightDetours': {singlePassMixed: true, straightDetours: true},
  'singlePassMixed + straightDetours + regionChildPorts': {singlePassMixed: true, straightDetours: true, regionChildPorts: true},
};

if (import.meta.url === `file://${process.argv[1]}`) {
  const graphs = [];
  for (const nested of [false, true]) for (const seed of [1, 2, 3, 4, 5, 6]) graphs.push([`organic-root ${nested ? 'nested' : 'flat'} ${seed}`, organicRoot({containers: 4, size: 6, nested, seed, loose: 5})]);
  for (const name of ['mixed-layouts', 'mixed-large']) {
    const file = `${ROOT}apps/playground/generated/examples/${name}.json`;
    if (existsSync(file)) { const raw = JSON.parse(readFileSync(file, 'utf8')); graphs.push([name, raw.graph ?? raw]); }
  }
  const rows = [];
  for (const [name, graph] of graphs) for (const [variant, options] of Object.entries(VARIANTS)) {
    const {snapshot, ms} = arqlet({op: 'import', graph, limit: graph.nodes.length, options});
    const out = normalizeArqlet(snapshot);
    const m = metrics(out);
    const sides = offAxis(graph, out);
    const issues = snapshot.geometryIssues.filter(i => i.kind !== 'straight-through-node');
    rows.push({graph: name, variant, lineNodeHits: m.lineNodeHits, crossings: m.crossings, bends: m.bends,
      overlaps: issues.filter(i => /overlap/.test(i.kind)).length, otherIssues: issues.filter(i => !/overlap/.test(i.kind)).length,
      borderEnds: sides.ends, offAxis: sides.off, ms: Math.round(ms)});
  }
  if (process.argv.includes('--json')) for (const r of rows) console.log(JSON.stringify(r));
  else {
    console.log('| graph | variant | line–node hits | crossings | bends | overlaps | other geometry issues | ends off the axis | ms |');
    console.log('|---|---|---|---|---|---|---|---|---|');
    for (const r of rows) console.log(`| ${r.graph} | ${r.variant} | ${r.lineNodeHits} | ${r.crossings} | ${r.bends} | ${r.overlaps} | ${r.otherIssues} | ${r.offAxis}/${r.borderEnds} | ${r.ms} |`);
    // Sums over the synthetic graphs per variant.
    const sums = {};
    for (const r of rows.filter(r => r.graph.startsWith('organic-root'))) {
      const s = sums[r.variant] ??= {hits: 0, crossings: 0, bends: 0, overlaps: 0, off: 0, ends: 0};
      s.hits += r.lineNodeHits; s.crossings += r.crossings; s.bends += r.bends; s.overlaps += r.overlaps; s.off += r.offAxis; s.ends += r.borderEnds;
    }
    console.log('\nSums over the 12 synthetic graphs:');
    for (const [v, s] of Object.entries(sums)) console.log(`- ${v}: hits ${s.hits}, crossings ${s.crossings}, bends ${s.bends}, overlaps ${s.overlaps}, ends off the axis ${s.off}/${s.ends}`);
  }
}
