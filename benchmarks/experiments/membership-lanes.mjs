// Experiment, not merged into the engine (ADR 0022 M4): lanes with what the engine has. No timing. Needs
// target/release/layout-command.
//
//   flat        the graph without containers, no lanes: what lanes cost is measured against it
//   containers  every group of the first dimension as a container (M1), the nearest thing today
//   lanes       every node pinned across the flow into the band of its group (`pinsInSinglePass`), bands in the declared
//               order; the second dimension, where there is one, as bands along the flow: helper edges from every node of a
//               band to every node of the next, taken out before anything is measured; an edge that runs back between bands is turned
//               for the layout only, so that the cycle break does not turn a helper edge instead
//   sorted      `lanes`, with the nodes of a rank and band sorted by the mean place of their neighbours, eight rounds, the best
//               round kept (violations, hits, crossings, bends): what ordering inside the bands is worth
//   engine      `as: lanes` of the engine (M4a), one dimension only
//   best        `sorted` with the best order of the bands (all orders, at most four bands): an upper bound
//
// Bands: each as high as its fullest rank, nodes in the order of the flat drawing, GAP apart. `violations`: nodes outside their
// band across the flow, and along it nodes that overlap the stretch of another phase.
// Inputs: the fixtures with a dimension that has every node in exactly one group, and graphs of the corpus whose top-level
// containers become the lanes (edges between leaves only, all containers taken away).
import {readFileSync, readdirSync} from 'node:fs';
import {spawnSync} from 'node:child_process';
import {ROOT, LAYOUT_COMMAND, normalizeArqlet, metrics} from '../eval/lib.mjs';
const GAP = 40, TOP = 48;
const run = r => { const a = JSON.parse(spawnSync(LAYOUT_COMMAND, [], {input: JSON.stringify(r), encoding: 'utf8', maxBuffer: 1 << 28}).stdout); if (!a.ok) throw new Error(a.error); return a; };
const imp = (graph, options = {}) => run({op: 'import', graph, limit: 400, options}).snapshot;
const score = d => { const m = metrics(d); return {hits: m.lineNodeHits, shared: m.sharedLinePairs, over: m.siblingOverlaps + m.crossBranchOverlaps, x: m.crossings + m.sharedEndpointCrossings, b: m.bends, l: Math.round(m.length), a: Math.round(m.area)}; };
const strip = s => { const d = normalizeArqlet(s); return {nodes: d.nodes, edges: d.edges.filter(e => !e.id.startsWith('__'))}; };
const show = q => `hits ${q.hits} overlaps ${q.over} shared ${q.shared} x ${q.x} b ${q.b} len ${q.l} area ${(q.a / 1e6).toFixed(2)}M`;
const permutations = list => list.length < 2 ? [list] : list.flatMap((x, i) => permutations([...list.slice(0, i), ...list.slice(i + 1)]).map(p => [x, ...p]));

function lanes(graph, laneOf, order, phaseOf, phases, rounds = 0) {
  const extra = [];
  if (phaseOf) for (let k = 1; k < phases.length; k++) for (const a of graph.nodes) for (const b of graph.nodes)
    if (phaseOf.get(a.id) === phases[k - 1] && phaseOf.get(b.id) === phases[k]) extra.push({id: `__${extra.length}`, kind: 'help', source: a.id, target: b.id});
  // An edge from a later phase to an earlier one is laid out the other way round, and drawn in its own direction again.
  const back = new Set(phaseOf ? graph.edges.filter(e => phases.indexOf(phaseOf.get(e.source)) > phases.indexOf(phaseOf.get(e.target))).map(e => e.id) : []);
  const g = {...graph, edges: [...graph.edges.map(e => back.has(e.id) ? {...e, source: e.target, target: e.source} : e), ...extra]};
  const first = imp(g), at = new Map(normalizeArqlet(first).nodes.map(n => [n.id, n]));
  const plan = first.layeredPlans?.['<root>'];
  if (!plan) throw new Error('no layered plan');
  const ranks = plan.ranks.map(r => r.filter(id => at.has(id)));
  const [across, size] = plan.vertical ? ['x', 'w'] : ['y', 'h'];
  const stack = list => list.reduce((t, id) => t + at.get(id)[size] + GAP, 0);
  const bands = new Map();
  let start = TOP;
  for (const lane of order) {
    const height = Math.max(0, ...ranks.map(r => stack(r.filter(id => laneOf.get(id) === lane))));
    bands.set(lane, [start, start + height]);
    start += height + GAP;
  }
  const place = (pos) => {
    const pins = {};
    for (const r of ranks) for (const lane of order) {
      const mine = r.filter(id => laneOf.get(id) === lane).sort((p, q) => pos(p) - pos(q) || (p < q ? -1 : 1));
      const [lo, hi] = bands.get(lane);
      let c = lo + (hi - lo - stack(mine)) / 2 + GAP / 2;
      for (const id of mine) { pins[id] = {x: at.get(id).x, y: at.get(id).y, [across]: c}; c += at.get(id)[size] + GAP; }
    }
    return pins;
  };
  const near = new Map(graph.nodes.map(n => [n.id, []]));
  for (const e of graph.edges) if (e.source !== e.target) { near.get(e.source).push(e.target); near.get(e.target).push(e.source); }
  let pins = place(id => at.get(id)[across]), best = null;
  for (let round = 0; round <= rounds; round++) {
    const q = judge(pins);
    const key = [q.lanes + q.phases, q.hits, q.x, q.b];
    if (!best || key.findIndex((v, i) => v !== best.key[i]) >= 0 && key.find((v, i) => v !== best.key[i]) < best.key[key.findIndex((v, i) => v !== best.key[i])]) best = {...q, key};
    const now = pins;
    pins = place(id => { const n = near.get(id); return n.length ? n.reduce((t, m) => t + now[m][across], 0) / n.length : now[id][across]; });
  }
  return best;
  function judge(pins) {
  const s = imp(g, {pinsInSinglePass: true, pins});
  const restored = {...s, graph: {...s.graph, edges: s.graph.edges.map(e => back.has(e.id) ? {...e, source: e.target, target: e.source} : e)}, routes: s.routes.map(x => back.has(x.id) ? {...x, points: [...x.points].reverse()} : x)};
  const d = strip(restored), now = new Map(d.nodes.map(n => [n.id, n]));
  // The drawing may be shifted as a whole: bands are compared by their own extents, in order.
  const extent = (key, axis, side) => { const of = d.nodes.filter(n => key.get(n.id) !== undefined); const m = new Map(); for (const n of of) { const k = key.get(n.id), e = m.get(k) ?? [Infinity, -Infinity]; m.set(k, [Math.min(e[0], n[axis]), Math.max(e[1], n[axis] + n[side])]); } return m; };
  const broken = (key, list, axis, side) => { const e = extent(key, axis, side); let bad = 0; for (let i = 0; i < list.length; i++) for (let j = i + 1; j < list.length; j++) { const a = e.get(list[i]), b = e.get(list[j]); if (a && b && a[1] > b[0] + 1e-6) bad += d.nodes.filter(n => (key.get(n.id) === list[j] && n[axis] < a[1]) || (key.get(n.id) === list[i] && n[axis] + n[side] > b[0])).length; } return bad; };
  const [along, long] = plan.vertical ? ['y', 'h'] : ['x', 'w'];
  let wrong = 0; for (const id of Object.keys(pins)) if (Math.abs(now.get(id)[across] - pins[id][across]) > 1) wrong++;
  return {...score(d), lanes: broken(laneOf, order, across, size), phases: phaseOf ? broken(phaseOf, phases, along, long) : 0, off: wrong, n: d.nodes.length, single: (s.layoutDiagnostics ?? []).some(x => x.startsWith('Single pass:'))};
  }
}

const totals = {views: 0, laneViolations: 0, phaseViolations: 0, withoutViolation: 0, hard: 0, refused: 0, pins: 0, pinsOff: 0, notSingle: 0, sortedClean: 0, engineBad: 0, engineIssues: 0, engine: [], x: [], b: []};
function study(name, graph, laneOf, order, second) {
  totals.views++;
  const out = [];
  const flat = score(strip(imp(graph)));
  out.push(`flat ${show(flat)}`);
  const boxes = {...graph, nodes: [...order.map(o => ({id: `__c${o}`, kind: 'lane', label: String(o), parentId: null})), ...graph.nodes.map(n => ({...n, parentId: `__c${laneOf.get(n.id)}`}))]};
  try { out.push(`containers ${show(score(strip(imp(boxes))))}`); } catch (e) { out.push(`containers refused ${e.message.slice(0, 60)}`); }
  if (!second) try {
    const memberships = {dimensions: [{id: 'lane', groups: order.map(o => ({id: String(o), members: graph.nodes.filter(n => laneOf.get(n.id) === o).map(n => n.id)}))}]};
    const s = imp({...graph, memberships}, {groupingPriority: [{by: 'lane', as: 'lanes'}]}), d = strip(s), q = score(d), v = s.layeredPlans?.['<root>']?.vertical;
    const [axis, side] = v ? ['x', 'w'] : ['y', 'h'], e = order.map(o => { const of = d.nodes.filter(n => laneOf.get(n.id) === o); return [Math.min(...of.map(n => n[axis])), Math.max(...of.map(n => n[axis] + n[side]))]; });
    let bad = 0; for (let i = 1; i < e.length; i++) if (e[i][0] < e[i - 1][1] - 1e-6) bad++;
    totals.engineBad += bad; totals.engineIssues += s.geometryIssues.length; totals.engine.push([flat.x, q.x, flat.b, q.b, flat.hits, q.hits, flat.shared, q.shared]);
    out.push(`engine ${show(q)} bands out of order ${bad} issues ${s.geometryIssues.length}${(s.layoutDiagnostics ?? []).some(x => x.startsWith('Single pass:')) ? '' : ' NOT single pass: ' + (s.layoutDiagnostics ?? [])[0]}`);
  } catch (e) { out.push(`engine refused: ${e.message.slice(0, 100)}`); }
  if (!process.argv.includes('--engine')) try {
    const q = lanes(graph, laneOf, order, second?.of, second?.order);
    totals.laneViolations += q.lanes; totals.phaseViolations += q.phases; totals.pins += q.n; totals.pinsOff += q.off; if (!q.single) totals.notSingle++;
    if (!q.lanes && !q.phases) totals.withoutViolation++;
    if (q.hits > flat.hits || q.over > flat.over || q.shared > flat.shared) totals.hard++;
    out.push(`lanes ${show(q)} violations ${q.lanes}/${q.phases} pins off ${q.off}${q.single ? '' : ' NOT single pass'}`);
    const sorted = lanes(graph, laneOf, order, second?.of, second?.order, 8);
    out.push(`sorted ${show(sorted)} violations ${sorted.lanes}/${sorted.phases}`);
    totals.sortedClean += !sorted.lanes && !sorted.phases; totals.x.push([flat.x, q.x, sorted.x]); totals.b.push([flat.b, q.b, sorted.b]);
    if (order.length <= 4) {
      let best = null;
      for (const p of permutations(order)) { const r = lanes(graph, laneOf, p, second?.of, second?.order, 8); if (r.lanes || r.phases) continue; if (!best || r.hits < best.hits || (r.hits === best.hits && (r.x < best.x || (r.x === best.x && r.b < best.b)))) best = {...r, p}; }
      out.push(best ? `best ${show(best)} order ${best.p.join(',').slice(0, 50)}` : 'best: none without violation');
    }
  } catch (e) { totals.refused++; out.push(`lanes refused: ${e.message.slice(0, 80)}`); }
  if (!second || !process.argv.includes('--engine')) console.log(`${name} (${graph.nodes.length} nodes, ${graph.edges.length} edges, ${order.length} lanes${second ? `, ${second.order.length} phases` : ''})\n  ${out.join('\n  ')}`);
}
/** The graph without containers: leaves, and edges between them. */
function leaves(graph) {
  const inside = new Set(graph.nodes.map(n => n.parentId).filter(Boolean)), ids = new Set(graph.nodes.filter(n => !inside.has(n.id)).map(n => n.id));
  return {defaultDirection: 'lr', nodes: graph.nodes.filter(n => ids.has(n.id)).map(n => ({id: n.id, kind: n.kind, label: n.label, parentId: null})), edges: graph.edges.filter(e => ids.has(e.source) && ids.has(e.target)).map(e => ({id: e.id, kind: e.kind, source: e.source, target: e.target}))};
}
const key = (d, flat) => { const of = new Map(); for (const g of d.groups) for (const m of g.members) { if (of.has(m)) return null; of.set(m, g.id); } return flat.nodes.every(n => of.has(n.id)) ? {of, order: d.groups.map(g => g.id)} : null; };
for (const f of readdirSync(`${ROOT}examples/fixtures/memberships`).filter(f => f.endsWith('.json')).sort()) {
  const graph = JSON.parse(readFileSync(`${ROOT}examples/fixtures/memberships/${f}`, 'utf8')), flat = leaves(graph);
  const keys = graph.memberships.dimensions.map(d => [d.id, key(d, flat)]).filter(([, k]) => k && k.order.length > 1);
  for (const [id, k] of keys) study(`${f} ${id}`, flat, k.of, k.order, null);
  for (const [a, ka] of keys) for (const [b, kb] of keys) if (a !== b) study(`${f} ${a} x ${b}`, flat, ka.of, ka.order, kb);
}
for (const f of ['bpmn-pools', 'network-vlan', 'workflow-dag', 'org-chart', 'uml-class', 'layerweave-code']) {
  const graph = JSON.parse(readFileSync(`${ROOT}benchmarks/inputs/${f}.json`, 'utf8')), flat = leaves(graph);
  const parent = new Map(graph.nodes.map(n => [n.id, n.parentId ?? null]));
  const chain = id => { const c = []; for (let p = parent.get(id); p; p = parent.get(p)) c.unshift(p); return c; };
  // The first level of containers on which the leaves are in more than one.
  for (const level of [0, 1]) {
    const of = new Map(flat.nodes.map(n => [n.id, chain(n.id)[level] ?? chain(n.id).at(-1) ?? 'none'])), order = [...new Set(of.values())];
    if (order.length > 1 && order.length <= 12) { study(`${f} level ${level}`, flat, of, order, null); break; }
  }
}
console.log(JSON.stringify(totals));
