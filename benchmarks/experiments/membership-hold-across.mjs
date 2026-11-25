// Experiment, not merged into the engine (ADR 0022 M3c): groups across containers that the default view does not draw whole,
// each tried alone. No timing. Needs target/release/layout-command.
//
//   pieces  the members inside each owner as a container nested there (M3b, per owner)
//   border  as pieces, and helper edges that push each piece to the border of its owner that faces the other pieces:
//           after everything else in the owner that is first in the flow, before everything in the last one; for an owner in
//           between, only the nested container. Every order of the owners by their place in the default view.
//   pull    as border, with a helper edge from piece to piece as well, so that the owners become neighbours
//
// The helper edges and containers are taken out before anything is measured, so the space of their channels stays: areas are
// an upper bound. `pieces` counts the hulls a group needs (M2b rules, rectangle): one per owner at best while not whole.
import {readFileSync, readdirSync} from 'node:fs';
import {spawnSync} from 'node:child_process';
import {ROOT, LAYOUT_COMMAND, normalizeArqlet, metrics} from '../eval/lib.mjs';
const src = readFileSync(`${ROOT}benchmarks/memberships.mjs`, 'utf8');
const part = src.slice(src.indexOf('/** mulberry32'), src.indexOf('const QUALITY'));
const plant = new Function('SEED', 'SIZE', part + ';return plant;')(22, 4);
const run = r => { const a = JSON.parse(spawnSync(LAYOUT_COMMAND, [], {input: JSON.stringify(r), encoding: 'utf8', maxBuffer: 1 << 28}).stdout); if (!a.ok) throw new Error(a.error); return a; };
const imp = graph => run({op: 'import', graph, limit: 200, options: {}}).snapshot;
const help = id => id.startsWith('__');

/** The drawing without the helper edges and containers, children back at their owners. */
function drawing(s, parent) {
  const d = normalizeArqlet(s);
  const inside = new Set([...parent.values()].filter(Boolean));
  return {nodes: d.nodes.filter(n => !help(n.id)).map(n => ({...n, parentId: parent.get(n.id), container: inside.has(n.id)})), edges: d.edges.filter(e => !help(e.id))};
}
function score(d) { const m = metrics(d); return {hits: m.lineNodeHits, x: m.crossings + m.sharedEndpointCrossings, b: m.bends, a: m.area}; }
const box = (g) => [Math.min(...g.map(n => n.x)) - 10, Math.min(...g.map(n => n.y)) - 10, Math.max(...g.map(n => n.x + n.w)) + 10, Math.max(...g.map(n => n.y + n.h)) + 10];
/** Foreign leaves in the group's rectangle: of an owner of the group, and of other containers (or the top level). */
function intruders(d, members) {
  const [x0, y0, x1, y1] = box(d.nodes.filter(n => members.includes(n.id)));
  const owners = new Set(d.nodes.filter(n => members.includes(n.id)).map(n => n.parentId));
  const inside = d.nodes.filter(n => !n.container && !members.includes(n.id) && n.x + n.w / 2 >= x0 && n.x + n.w / 2 <= x1 && n.y + n.h / 2 >= y0 && n.y + n.h / 2 <= y1);
  return {own: inside.filter(n => owners.has(n.parentId)).length, other: inside.filter(n => !owners.has(n.parentId)).length};
}
function clean(d, members) {
  const [x0, y0, x1, y1] = box(d.nodes.filter(n => members.includes(n.id)));
  return !d.nodes.some(n => !n.container && !members.includes(n.id) && n.x + n.w / 2 >= x0 && n.x + n.w / 2 <= x1 && n.y + n.h / 2 >= y0 && n.y + n.h / 2 <= y1);
}
function variant(graph, base, members, parent, how) {
  const owners = [...new Set(members.map(m => parent.get(m)))];
  const at = new Map(normalizeArqlet(base).nodes.map(n => [n.id, n]));
  const mid = o => { const g = o ? [at.get(o)] : members.filter(m => parent.get(m) === null).map(m => at.get(m)); return g.reduce((t, n) => t + n.x + n.w / 2 + n.y + n.h / 2, 0) / g.length; };
  owners.sort((a, b) => mid(a) - mid(b));
  const nodes = graph.nodes.map(n => ({...n})), edges = [...graph.edges];
  const piece = new Map();
  owners.forEach((o, k) => {
    const mine = members.filter(m => parent.get(m) === o);
    if (mine.length === 1) { piece.set(o, mine[0]); return; }
    nodes.push({id: `__p${k}`, kind: 'group', label: '', parentId: o});
    for (const n of nodes) if (mine.includes(n.id)) n.parentId = `__p${k}`;
    piece.set(o, `__p${k}`);
  });
  const link = (a, b, i) => edges.push({id: `__e${i}`, kind: 'help', source: a, target: b});
  let i = 0;
  if (how !== 'pieces') owners.forEach((o, k) => {
    if (o === null) return;
    const others = graph.nodes.filter(n => (n.parentId ?? null) === o && !members.includes(n.id));
    if (k === 0) for (const n of others) link(n.id, piece.get(o), i++);
    else if (k === owners.length - 1) for (const n of others) link(piece.get(o), n.id, i++);
  });
  if (how === 'pull') for (let k = 1; k < owners.length; k++) link(piece.get(owners[k - 1]), piece.get(owners[k]), i++);
  return drawing(imp({...graph, nodes, edges}), parent);
}
const totals = {};
function study(source, input, graph, dim) {
  const {memberships: _, ...plain} = graph;
  const base = imp(plain), parent = new Map(base.graph.nodes.map(n => [n.id, n.parentId ?? null]));
  const d0 = drawing(base, parent), s0 = score(d0);
  for (const g of graph.memberships.dimensions.find(d => d.id === dim).groups) {
    const members = g.members.filter(m => parent.has(m)), owners = new Set(members.map(m => parent.get(m)));
    if (members.length < 2 || owners.size < 2 || clean(d0, members)) continue;
    const t = totals[source] ??= {groups: 0, two: 0, pieces: 0, border: 0, pull: 0, any: 0, cheap: 0, refused: 0, pullFailsOwnOnly: 0, pullFailsOtherOnly: 0, pullFailsBoth: 0};
    t.groups++; if (owners.size === 2) t.two++;
    const out = []; let best = null, any = false;
    for (const how of ['pieces', 'border', 'pull']) {
      try {
        const d = variant(plain, base, members, parent, how), s = score(d), ok = clean(d, members);
        const i = intruders(d, members);
        if (!ok && how === 'pull') t[i.own && i.other ? 'pullFailsBoth' : i.own ? 'pullFailsOwnOnly' : 'pullFailsOtherOnly']++;
        if (ok) { any = true; t[how]++; if (s.hits <= s0.hits && s.x - s0.x <= Math.max(2, s0.x / 20) && s.b - s0.b <= Math.max(4, s0.b / 20)) best = how; }
        out.push(`${how} ${ok ? 'WHOLE' : 'no'} hits ${s.hits - s0.hits} x ${s0.x}${s.x - s0.x < 0 ? '' : '+'}${s.x - s0.x} b ${s0.b}${s.b - s0.b < 0 ? '' : '+'}${s.b - s0.b} area ${(s.a / s0.a).toFixed(2)}`);
      } catch (e) { t.refused++; out.push(`${how} refused: ${e.message.slice(0, 60)}`); }
    }
    if (any) t.any++;
    if (best) t.cheap++;
    console.log(source, input, dim, g.id, `${members.length} members, ${owners.size} owners:`, out.join(' | '));
  }
}
for (const f of readdirSync(`${ROOT}examples/fixtures/memberships`).filter(f => f.endsWith('.json')).sort()) {
  const {groupingPriority: _, ...graph} = JSON.parse(readFileSync(`${ROOT}examples/fixtures/memberships/${f}`, 'utf8'));
  for (const d of graph.memberships.dimensions) study('fixture', f, graph, d.id);
}
for (const f of readdirSync(`${ROOT}benchmarks/inputs`).filter(f => f.endsWith('.json') && !/manifest|synthetic/.test(f)).sort()) {
  const graph = JSON.parse(readFileSync(`${ROOT}benchmarks/inputs/${f}`, 'utf8'));
  const memberships = plant(graph, 'near');
  if (memberships.dimensions[0].groups.length) study('near', f, {...graph, memberships}, 'near');
}
console.log(JSON.stringify(totals));
