// Experiment, not merged into the engine (ADR 0022 M3c): as membership-hold-across.mjs, only groups with two owners, aligned
// across the flow. Helper edges put a piece alone in its rank, so order hints have nothing to order there; here every member is
// pinned instead (`pinsInSinglePass`: a pin chooses rank, order and place across the flow). The members of the owner that comes
// first in the flow are pinned at its end, those of the other at its start, next to each other across the flow, both owners at
// the same absolute place: the start, middle or end of the stretch the two owners share, where they share one, else of the
// first owner's. The boxes of one layout give the next one's pins, three rounds. `bound` is the best of every round and place:
// an upper bound of what a rule could choose, not a rule. No timing. Needs target/release/layout-command.
import {readFileSync, readdirSync} from 'node:fs';
import {spawnSync} from 'node:child_process';
import {ROOT, LAYOUT_COMMAND, normalizeArqlet, metrics} from '../eval/lib.mjs';
const src = readFileSync(`${ROOT}benchmarks/memberships.mjs`, 'utf8');
const part = src.slice(src.indexOf('/** mulberry32'), src.indexOf('const QUALITY'));
const plant = new Function('SEED', 'SIZE', part + ';return plant;')(22, 4);
const run = r => { const a = JSON.parse(spawnSync(LAYOUT_COMMAND, [], {input: JSON.stringify(r), encoding: 'utf8', maxBuffer: 1 << 28}).stdout); if (!a.ok) throw new Error(a.error); return a; };
const imp = (graph, options = {}) => run({op: 'import', graph, limit: 200, options}).snapshot;
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
/** Pins for the members, from the boxes of `s`. `where`: 0 start, 0.5 middle, 1 end of the shared stretch. */
function pinsFor(s, members, owners, parent, where) {
  const d = normalizeArqlet(s), at = new Map(d.nodes.map(n => [n.id, n]));
  const all = d.nodes, rect = o => o ? at.get(o) : {x: Math.min(...all.map(n => n.x)), y: Math.min(...all.map(n => n.y)), w: Math.max(...all.map(n => n.x + n.w)) - Math.min(...all.map(n => n.x)), h: Math.max(...all.map(n => n.y + n.h)) - Math.min(...all.map(n => n.y))};
  const vertical = s.layeredPlans?.[owners[0] ?? '<root>']?.vertical ?? false;
  const [along, across, long, wide] = vertical ? ['y', 'x', 'h', 'w'] : ['x', 'y', 'w', 'h'];
  const R = owners.map(rect), order = R[0][along] + R[0][long] / 2 <= R[1][along] + R[1][long] / 2 ? [0, 1] : [1, 0];
  const inner = 60, gap = 40;
  const lo = Math.max(R[0][across], R[1][across]) + inner, hi = Math.min(R[0][across] + R[0][wide], R[1][across] + R[1][wide]) - 20;
  const pins = {};
  order.forEach((k, turn) => {
    const mine = members.filter(m => parent.get(m) === owners[k]).sort((a, b) => at.get(a)[across] - at.get(b)[across]);
    const need = mine.reduce((t, m) => t + at.get(m)[wide] + gap, -gap);
    const own = [R[k][across] + inner, R[k][across] + R[k][wide] - 20];
    const [a, z] = hi - lo >= need ? [lo, hi] : [R[order[0]][across] + inner, R[order[0]][across] + R[order[0]][wide] - 20];
    let c = a + Math.max(0, z - a - need) * where;
    if (owners[k]) c = Math.max(own[0], Math.min(c, Math.max(own[0], own[1] - need)));
    for (const m of mine) {
      const n = at.get(m), base = owners[k] ? R[k] : {x: 0, y: 0};
      const l = turn === 0 ? R[k][along] + R[k][long] - 24 - n[long] : R[k][along] + (vertical ? 48 : 24);
      pins[m] = {[along]: l - base[along], [across]: c - base[across]};
      c += n[wide] + gap;
    }
  });
  return pins;
}
const totals = {};
function study(source, input, graph, dim) {
  const {memberships: _, ...plain} = graph;
  const base = imp(plain), parent = new Map(base.graph.nodes.map(n => [n.id, n.parentId ?? null]));
  const d0 = drawing(base, parent), s0 = score(d0);
  for (const g of graph.memberships.dimensions.find(d => d.id === dim).groups) {
    const members = g.members.filter(m => parent.has(m)), owners = new Set(members.map(m => parent.get(m)));
    if (owners.size !== 2 || clean(d0, members)) continue;
    const t = totals[source] ??= {groups: 0, bound: 0, noNewHits: 0, cheap: 0, layouts: 0, refused: 0};
    t.groups++;
    let whole = 0, cheap = false, best = null;
    const list = [...owners];
    for (const where of [0, .5, 1]) {
      let s = base;
      for (let round = 1; round <= 3; round++) {
        try { s = imp(plain, {pinsInSinglePass: true, pins: pinsFor(s, members, list, parent, where)}); } catch (e) { t.refused++; t.why ??= e.message.slice(0, 80); break; }
        t.layouts++;
        const d = drawing(s, parent); if (!clean(d, members)) continue;
        const q = score(d), inBudget = q.hits <= s0.hits && q.x - s0.x <= Math.max(2, s0.x / 20) && q.b - s0.b <= Math.max(4, s0.b / 20);
        whole++; cheap ||= inBudget;
        if (!best || q.hits < best.hits || (q.hits === best.hits && (q.x < best.x || (q.x === best.x && q.b < best.b)))) best = {...q, how: `place ${where} round ${round}`};
      }
    }
    if (whole) t.bound++; if (cheap) t.cheap++; if (best && best.hits <= s0.hits) t.noNewHits++;
    console.log(source, input, dim, g.id, best ? `WHOLE in ${whole} layouts, best ${best.how} hits ${best.hits - s0.hits} x ${s0.x} to ${best.x}, b ${s0.b} to ${best.b}, area ${(best.a / s0.a).toFixed(2)}${cheap ? ', in budget' : ''}` : 'never whole');
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
