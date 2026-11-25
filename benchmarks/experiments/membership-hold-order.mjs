// Experiment, not merged into the engine (ADR 0022 M3b): members made neighbours in their ranks through orderHints (block first, middle, last).
// Only groups inside one container that the default view does not draw whole. No timing. Needs target/release/layout-command.
import {readFileSync, readdirSync} from 'node:fs';
import {spawnSync} from 'node:child_process';
const ROOT = new URL('../..', import.meta.url).pathname.replace(/\/$/, '');
const {normalizeArqlet, metrics, LAYOUT_COMMAND} = await import(`${ROOT}/benchmarks/eval/lib.mjs`);
const src = readFileSync(`${ROOT}/benchmarks/memberships.mjs`, 'utf8');
const part = src.slice(src.indexOf('/** mulberry32'), src.indexOf('const QUALITY'));
const plant = new Function('SEED', 'SIZE', part + ';return plant;')(22, 4);
const run = r => { const a = JSON.parse(spawnSync(LAYOUT_COMMAND, [], {input: JSON.stringify(r), encoding: 'utf8', maxBuffer: 1 << 28}).stdout); if (!a.ok) throw new Error(a.error); return a; };
const imp = (graph, options) => run({op: 'import', graph, limit: 200, options}).snapshot;
const measure = s => { const m = metrics(normalizeArqlet(s)); return {hits: m.lineNodeHits, x: m.crossings + m.sharedEndpointCrossings, b: m.bends, a: m.area}; };
function whole(s, members) {
  const ns = normalizeArqlet(s).nodes, by = new Map(ns.map(n => [n.id, n])), g = members.map(m => by.get(m));
  const x0 = Math.min(...g.map(n => n.x)) - 10, y0 = Math.min(...g.map(n => n.y)) - 10, x1 = Math.max(...g.map(n => n.x + n.w)) + 10, y1 = Math.max(...g.map(n => n.y + n.h)) + 10;
  return !ns.some(n => !n.container && !members.includes(n.id) && n.x + n.w / 2 >= x0 && n.x + n.w / 2 <= x1 && n.y + n.h / 2 >= y0 && n.y + n.h / 2 <= y1);
}
function hintsFor(s, area, members, where) {
  const plan = s.layeredPlans?.[area]; if (!plan) return null;
  const real = new Set(s.graph.nodes.map(n => n.id)), M = new Set(members), hints = {};
  const frac = [];
  for (const rank of plan.ranks) { const r = rank.filter(i => real.has(i)); r.forEach((i, k) => { if (M.has(i)) frac.push(r.length > 1 ? k / (r.length - 1) : .5); }); }
  const mean = frac.reduce((a, b) => a + b, 0) / frac.length;
  for (const rank of plan.ranks) {
    const r = rank.filter(i => real.has(i)), mine = r.filter(i => M.has(i)), rest = r.filter(i => !M.has(i));
    if (!mine.length || !rest.length) continue;
    const at = where === 'first' ? 0 : where === 'last' ? rest.length : Math.round(mean * rest.length);
    let prev = at ? rest[at - 1] : null;
    for (const m of mine) { hints[m] = {after: prev}; prev = m; }
  }
  return hints;
}
const tot = {};
function study(source, input, graph, dim) {
  const base = imp(graph, {}), b0 = measure(base), parent = new Map(base.graph.nodes.map(n => [n.id, n.parentId ?? null]));
  for (const g of graph.memberships.dimensions.find(d => d.id === dim).groups) {
    const members = g.members.filter(m => parent.has(m)), owners = new Set(members.map(m => parent.get(m)));
    if (members.length < 2 || owners.size !== 1 || whole(base, members)) continue;
    const t = tot[source] ??= {split: 0, noplan: 0, whole: 0, free: 0, cheap: 0}; t.split++;
    const area = [...owners][0] ?? '<root>'; let best = null;
    if (!base.layeredPlans?.[area]) { t.noplan++; console.log(source, input, dim, g.id, 'no layered plan in', area); continue; }
    const outcomes = [];
    for (const where of ['first', 'middle', 'last']) {
      const h = hintsFor(base, area, members, where); if (!h || !Object.keys(h).length) { outcomes.push(`${where}: nothing to reorder`); continue; }
      const s = imp(graph, {orderHints: h}), ok = whole(s, members), m = measure(s);
      const d = [m.hits - b0.hits, m.x - b0.x, m.b - b0.b];
      outcomes.push(`${where}: ${ok ? 'WHOLE' : 'no'} hits ${d[0]} x ${d[1]} b ${d[2]}`);
      if (ok && !d[0] && (!best || d[1] < best[0] || d[1] === best[0] && d[2] < best[1])) best = [d[1], d[2]];
    }
    if (best) { t.whole++; if (best[0] <= 0 && best[1] <= 0) t.free++; else if (best[0] <= 2 && best[1] <= 4) t.cheap++; }
    console.log(source, input, dim, g.id, `(${members.length})`, outcomes.join(' | '));
  }
}
for (const f of readdirSync(`${ROOT}/examples/fixtures/memberships`).filter(f => f.endsWith('.json'))) {
  const {groupingPriority: _, ...graph} = JSON.parse(readFileSync(`${ROOT}/examples/fixtures/memberships/${f}`, 'utf8'));
  for (const d of graph.memberships.dimensions) study('fixture', f, graph, d.id);
}
for (const f of readdirSync(`${ROOT}/benchmarks/inputs`).filter(f => f.endsWith('.json') && !/manifest|synthetic/.test(f))) {
  const graph = JSON.parse(readFileSync(`${ROOT}/benchmarks/inputs/${f}`, 'utf8'));
  for (const k of ['near', 'far']) { const memberships = plant(graph, k); if (memberships.dimensions[0].groups.length) study(k, f, {...graph, memberships}, k); }
}
console.log(JSON.stringify(tot));
