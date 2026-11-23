// Usage: node benchmarks/eval/simplex-guard.mjs [--pose uniform|native] [--limit 150] [--json OUT] [DATASET ...];
// needs target/release/layout-command.
//
// ADR 0027, step 0: the guard of step 1, checked offline before any engine change. Step 1 would rank every layered area
// both ways (longest paths, network simplex) and keep the simplex ranks for an area only when its plan is not worse in the
// order the plan can see: first the plan's crossings, then its long-edge dummies; ties keep the longest-path ranks.
//
// Offline, the two rankings come from two whole layouts — the default and `simplexRanks` — and every area's stored plan is
// read from each. Per area this records:
//   ranks changed    whether the rank of any child differs between the two (the order alone does not count)
//   plan crossings   between neighbouring ranks, as `plan-crossings.mjs` counts them; long-edge dummies
//   guard            the choice step 1 would make ("simplex", "longest", or "same" when the ranks do not differ)
//   drawn Δ          the crossings and bends of the drawing that lie in this area and not in a child container
//                    (simplex − default; every crossing point and bend is given to the innermost container around it, so
//                    the areas add up to the view's totals, which is checked against `metrics`)
//   box Δ            the change of the area's box (the root: the drawing's extent)
// A view worse than the default in objective order (crossings, then bends; overlaps and line–node hits are 0 in both arms
// on this corpus and checked) has "loss areas": areas whose own drawn crossings, then bends, got worse. The check asks:
//   - does the guard reject simplex on every loss area whose ranks changed?
//   - loss areas whose ranks did not change lie outside the changed areas (their box changed through a child): the local
//     guard cannot see them (ADR 0027: the guard is local, the acceptance global)
//   - on the views that carry the area gain, does the guard keep simplex on the areas whose ranks changed and whose box
//     shrank, weighted by how much?
// Limits of the offline reading: in the simplex layout every area uses simplex ranks, so an area's plan is ordered around
// children laid out with simplex ranks too; step 1 would order it around the children it chose. The comparison is the
// closest one without an engine change.
import {readFileSync, writeFileSync, existsSync} from 'node:fs';
import {join} from 'node:path';
import {arqlet, normalizeArqlet, metrics, ROOT} from './lib.mjs';

const args = process.argv.slice(2);
const option = (name, fallback) => { const i = args.indexOf(name); return i >= 0 ? args.splice(i, 2)[1] : fallback; };
const LIMIT = Number(option('--limit', 150));
const POSE = option('--pose', 'uniform');
const JSON_OUT = option('--json', null);
const LAYERED = ['activity-order', 'arqlet-pipeline', 'bpmn-pools', 'can-bus', 'cloud-architecture', 'deployment-cluster',
  'er-diagram', 'layerweave-code', 'mixed-large', 'network-vlan', 'org-chart', 'petri-net', 'state-order', 'state-tcp',
  'synthetic-2k', 'uml-class', 'workflow-dag'];
const selected = args.length ? args : LAYERED;
const GAIN_VIEWS = new Set(['org-chart', 'mixed-large', 'synthetic-2k']);
const ROOT_AREA = '<root>';
const DUMMY = '\u0001dummy:';
const STRUCTURAL = ['\u0001dummy:border:', '\u0001dummy:side-in:', '\u0001dummy:side-out:'];
const isLongDummy = id => id.startsWith(DUMMY) && !STRUCTURAL.some(p => id.startsWith(p));
const EPS = 1e-6;

const load = name => {
  const raw = JSON.parse(readFileSync(join(ROOT, 'benchmarks/datasets', `${name}.json`), 'utf8'));
  const graph = raw.graph ?? raw;
  if (POSE === 'uniform') {
    delete graph.buses; delete graph.defaultRouting; delete graph.containerRouting;
    graph.containerDirections = {}; graph.containerStrategies = {}; graph.defaultStrategy = 'none';
    if (!['lr', 'tb'].includes(graph.defaultDirection)) graph.defaultDirection = 'lr';
  }
  return graph;
};

// The plan's crossings between neighbouring ranks (sections sharing an end do not count) and its long-edge dummies.
function planCounts(plan) {
  const place = new Map();
  plan.ranks.forEach((r, ri) => r.forEach((id, ci) => place.set(id, [ri, ci])));
  const byRank = new Map();
  for (const chain of plan.chains ?? []) {
    const path = [chain.source, ...(chain.dummies ?? []), chain.target];
    for (let i = 0; i + 1 < path.length; i++) {
      const a = place.get(path[i]), b = place.get(path[i + 1]);
      if (!a || !b || b[0] !== a[0] + 1) continue;
      if (!byRank.has(a[0])) byRank.set(a[0], []);
      byRank.get(a[0]).push([a[1], b[1], path[i], path[i + 1]]);
    }
  }
  let crossings = 0;
  for (const list of byRank.values()) {
    for (let i = 0; i < list.length; i++) {
      for (let j = i + 1; j < list.length; j++) {
        const [u1, l1, a1, b1] = list[i], [u2, l2, a2, b2] = list[j];
        if (a1 === a2 || b1 === b2) continue;
        if ((u1 - u2) * (l1 - l2) < 0) crossings++;
      }
    }
  }
  return {crossings, dummies: plan.ranks.flat().filter(isLongDummy).length};
}

// The rank of every child (not dummies) of an area.
const childRanks = plan => {
  const out = {};
  plan.ranks.forEach((r, ri) => r.forEach(id => { if (!id.startsWith(DUMMY)) out[id] = ri; }));
  return out;
};
const sameRanks = (a, b) => {
  const ka = Object.keys(a), kb = Object.keys(b);
  return ka.length === kb.length && ka.every(k => a[k] === b[k]);
};

// Every crossing point and bend of a drawing, given to the innermost container whose box holds it (else the root).
function drawnByArea(d) {
  const containers = d.nodes.filter(n => n.container).sort((p, q) => p.w * p.h - q.w * q.h);
  const areaOf = p => containers.find(c => p.x > c.x + EPS && p.x < c.x + c.w - EPS && p.y > c.y + EPS && p.y < c.y + c.h - EPS)?.id ?? ROOT_AREA;
  const out = {};
  const add = (area, k) => { out[area] ??= {crossings: 0, bends: 0}; out[area][k]++; };
  for (const e of d.edges) {
    const p = e.points;
    for (let i = 1; i + 1 < p.length; i++) {
      const ax = p[i].x - p[i - 1].x, ay = p[i].y - p[i - 1].y, bx = p[i + 1].x - p[i].x, by = p[i + 1].y - p[i].y;
      if (Math.abs(ax * by - ay * bx) > EPS) add(areaOf(p[i]), 'bends');
    }
  }
  const orient = (p, q, r) => (q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x);
  const segs = d.edges.map(e => e.points.slice(1).map((q, i) => {
    const a = e.points[i];
    return {a, b: q, x0: Math.min(a.x, q.x), x1: Math.max(a.x, q.x), y0: Math.min(a.y, q.y), y1: Math.max(a.y, q.y)};
  }));
  for (let i = 0; i < segs.length; i++) {
    for (let j = i + 1; j < segs.length; j++) {
      for (const s of segs[i]) {
        for (const t of segs[j]) {
          if (s.x1 < t.x0 || t.x1 < s.x0 || s.y1 < t.y0 || t.y1 < s.y0) continue;
          const o1 = orient(s.a, s.b, t.a), o2 = orient(s.a, s.b, t.b), o3 = orient(t.a, t.b, s.a), o4 = orient(t.a, t.b, s.b);
          if (!(((o1 > EPS && o2 < -EPS) || (o1 < -EPS && o2 > EPS)) && ((o3 > EPS && o4 < -EPS) || (o3 < -EPS && o4 > EPS)))) continue;
          const u = o3 / (o3 - o4);
          add(areaOf({x: s.a.x + (s.b.x - s.a.x) * u, y: s.a.y + (s.b.y - s.a.y) * u}), 'crossings');
        }
      }
    }
  }
  return out;
}

const boxDims = (d, id) => {
  if (id === ROOT_AREA) {
    const xs = d.nodes.flatMap(n => [n.x, n.x + n.w]), ys = d.nodes.flatMap(n => [n.y, n.y + n.h]);
    return {w: Math.max(...xs) - Math.min(...xs), h: Math.max(...ys) - Math.min(...ys)};
  }
  const n = d.nodes.find(m => m.id === id);
  return n ? {w: n.w, h: n.h} : {w: 0, h: 0};
};
const boxArea = (d, id) => { const b = boxDims(d, id); return b.w * b.h; };
const worse = (a, b) => a.crossings !== b.crossings ? a.crossings > b.crossings : a.bends > b.bends;

const views = [];
for (const name of selected) {
  if (!existsSync(join(ROOT, 'benchmarks/datasets', `${name}.json`))) continue;
  const first = arqlet({op: 'import', graph: load(name), limit: LIMIT});
  const view = first.snapshot.graph;
  const base = POSE === 'native' ? first.snapshot.options ?? {} : {};
  const run = options => arqlet({op: 'import', graph: {...view}, limit: view.nodes.length, options: {...base, ...options}}).snapshot;
  const sl = run({}), ss = run({simplexRanks: true});
  const dl = normalizeArqlet(sl), ds = normalizeArqlet(ss);
  const ml = metrics(dl), ms = metrics(ds);
  const drawnL = drawnByArea(dl), drawnS = drawnByArea(ds);
  const tot = o => Object.values(o).reduce((a, v) => ({crossings: a.crossings + v.crossings, bends: a.bends + v.bends}), {crossings: 0, bends: 0});
  const tl = tot(drawnL), ts = tot(drawnS);
  const check = {
    default: tl.crossings === ml.crossings + ml.sharedEndpointCrossings && tl.bends === ml.bends,
    simplex: ts.crossings === ms.crossings + ms.sharedEndpointCrossings && ts.bends === ms.bends,
  };
  const gate = m => ({overlaps: m.siblingOverlaps, hits: m.lineNodeHits, crossings: m.crossings + m.sharedEndpointCrossings, bends: m.bends});
  const gl = gate(ml), gs = gate(ms);
  const viewWorse = gs.overlaps > gl.overlaps || gs.hits > gl.hits || (gs.overlaps === gl.overlaps && gs.hits === gl.hits && worse(gs, gl));
  const parent = new Map(view.nodes.map(n => [n.id, n.parentId ?? ROOT_AREA]));
  const ancestors = id => { const out = []; for (let p = parent.get(id); p; p = parent.get(p)) { out.push(p); if (p === ROOT_AREA) break; } return out; };
  const areas = [];
  const names = new Set([...Object.keys(sl.layeredPlans ?? {}), ...Object.keys(ss.layeredPlans ?? {})]);
  for (const area of names) {
    const pl = sl.layeredPlans?.[area], ps = ss.layeredPlans?.[area];
    if (!pl || !ps) continue;
    const changed = !sameRanks(childRanks(pl), childRanks(ps));
    const cl = planCounts(pl), cs = planCounts(ps);
    const guard = !changed ? 'same'
      : (cs.crossings < cl.crossings || (cs.crossings === cl.crossings && cs.dummies < cl.dummies)) ? 'simplex' : 'longest';
    const a = drawnL[area] ?? {crossings: 0, bends: 0}, b = drawnS[area] ?? {crossings: 0, bends: 0};
    const bl = boxDims(dl, area), bs = boxDims(ds, area);
    areas.push({area, changed, plan: {longest: cl, simplex: cs}, guard,
      drawn: {crossings: b.crossings - a.crossings, bends: b.bends - a.bends}, loss: worse(b, a),
      box: bs.w * bs.h - bl.w * bl.h, dims: {default: bl, simplex: bs}});
  }
  // Where a loss outside the changed areas lies, seen from them: inside one (a child), around one (an ancestor), or apart.
  const changedSet = new Set(areas.filter(a => a.changed).map(a => a.area));
  for (const a of areas) {
    if (!a.loss || a.changed) continue;
    const up = a.area === ROOT_AREA ? [] : ancestors(a.area);
    const inside = up.some(x => changedSet.has(x));
    const around = [...changedSet].some(c => c !== a.area && (a.area === ROOT_AREA || ancestors(c).includes(a.area)));
    a.relation = inside && around ? 'child and ancestor' : inside ? 'child of a changed area' : around ? 'ancestor of a changed area' : 'apart';
  }
  views.push({name, singlePass: [sl, ss].map(s => (s.layoutDiagnostics ?? []).some(d => d.startsWith('Single pass: '))),
    gates: {default: gl, simplex: gs}, viewWorse, check, areas,
    area: {default: boxArea(dl, ROOT_AREA), simplex: boxArea(ds, ROOT_AREA)}});
}

// Report.
const short = id => id === ROOT_AREA ? '<root>' : id.length > 34 ? '…' + id.slice(-33) : id;
console.log(`pose ${POSE}, limit ${LIMIT}\n`);
const bad = views.filter(v => !v.check.default || !v.check.simplex).map(v => v.name);
console.log(bad.length ? `**Attribution does not add up to metrics on: ${bad.join(', ')}**\n` : 'Attribution adds up to the `metrics` totals on every view (crossings and bends, both arms).\n');
console.log('| view | worse in objective order | areas | ranks changed | guard keeps simplex | guard keeps longest |');
console.log('|---|---|---:|---:|---:|---:|');
for (const v of views) {
  const ch = v.areas.filter(a => a.changed);
  console.log(`| ${v.name} | ${v.viewWorse ? '**yes**' : 'no'} | ${v.areas.length} | ${ch.length} | ${ch.filter(a => a.guard === 'simplex').length} | ${ch.filter(a => a.guard === 'longest').length} |`);
}
const verdict = [];
for (const v of views.filter(v => v.viewWorse)) {
  console.log(`\n**${v.name}** (worse: crossings ${v.gates.default.crossings} → ${v.gates.simplex.crossings}, bends ${v.gates.default.bends} → ${v.gates.simplex.bends})\n`);
  console.log('| area | ranks changed | plan crossings L → S | dummies L → S | guard | drawn Δ crossings | drawn Δ bends | loss |');
  console.log('|---|---|---|---|---|---:|---:|---|');
  for (const a of v.areas.filter(a => a.changed || a.drawn.crossings || a.drawn.bends)) {
    console.log(`| ${short(a.area)} | ${a.changed ? 'yes' : 'no'} | ${a.plan.longest.crossings} → ${a.plan.simplex.crossings} | ${a.plan.longest.dummies} → ${a.plan.simplex.dummies} | ${a.guard} | ${a.drawn.crossings} | ${a.drawn.bends} | ${a.loss ? (a.changed ? '**yes**' : `**yes** (${a.relation})`) : ''} |`);
  }
  const losses = v.areas.filter(a => a.loss);
  const inChanged = losses.filter(a => a.changed), outside = losses.filter(a => !a.changed);
  const caught = inChanged.filter(a => a.guard === 'longest');
  verdict.push({view: v.name, losses: losses.length, inChanged: inChanged.length, caught: caught.length, outside: outside.length,
    relations: outside.map(a => a.relation)});
  console.log(`\nloss areas ${losses.length}: in areas whose ranks changed ${inChanged.length} (guard rejects simplex on ${caught.length}), outside the changed areas ${outside.length}.`);
}
for (const v of views.filter(v => GAIN_VIEWS.has(v.name) || v.areas.some(a => a.changed && a.guard === 'longest'))) {
  // Every area whose ranks changed, with its box before and after: a change of shape can shrink the parent even where the
  // area's own box does not shrink, so the box area alone does not say which area carries a view's gain.
  console.log(`\n**${v.name}** — every area whose ranks changed (view area ${Math.round(v.area.default / 1000)}k → ${Math.round(v.area.simplex / 1000)}k)\n`);
  console.log('| area | plan crossings L → S | dummies L → S | guard | box w × h default → simplex | drawn Δ crossings | drawn Δ bends |');
  console.log('|---|---|---|---|---|---:|---:|');
  for (const a of v.areas.filter(a => a.changed)) {
    const d0 = a.dims.default, d1 = a.dims.simplex, r = x => Math.round(x);
    console.log(`| ${short(a.area)} | ${a.plan.longest.crossings} → ${a.plan.simplex.crossings} | ${a.plan.longest.dummies} → ${a.plan.simplex.dummies} | ${a.guard} | ${r(d0.w)}×${r(d0.h)} → ${r(d1.w)}×${r(d1.h)} | ${a.drawn.crossings} | ${a.drawn.bends} |`);
  }
}
for (const v of views.filter(v => GAIN_VIEWS.has(v.name))) {
  const gain = v.areas.filter(a => a.changed && a.box < 0);
  const total = gain.reduce((s, a) => s - a.box, 0), kept = gain.filter(a => a.guard === 'simplex').reduce((s, a) => s - a.box, 0);
  console.log(`\n**${v.name}** (gain view: area ${Math.round(v.area.default / 1000)}k → ${Math.round(v.area.simplex / 1000)}k${v.viewWorse ? ', also worse in objective order' : ''})\n`);
  console.log('| area | plan crossings L → S | dummies L → S | guard | box Δ (k) | drawn Δ crossings | drawn Δ bends |');
  console.log('|---|---|---|---|---:|---:|---:|');
  for (const a of gain.sort((p, q) => p.box - q.box).slice(0, 12)) {
    console.log(`| ${short(a.area)} | ${a.plan.longest.crossings} → ${a.plan.simplex.crossings} | ${a.plan.longest.dummies} → ${a.plan.simplex.dummies} | ${a.guard} | ${Math.round(a.box / 1000)} | ${a.drawn.crossings} | ${a.drawn.bends} |`);
  }
  console.log(`\nareas with changed ranks and a smaller box: ${gain.length}; the guard keeps simplex on ${gain.filter(a => a.guard === 'simplex').length}, ` +
    `holding ${total ? Math.round(100 * kept / total) : 0} % of their summed box reduction.`);
}
if (JSON_OUT) writeFileSync(JSON_OUT, JSON.stringify({kind: 'simplex-guard', pose: POSE, limit: LIMIT, views, verdict}, null, 1) + '\n');
