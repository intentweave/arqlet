// Usage: node benchmarks/eval/area-budget.mjs [--limit 150] [--json OUT] [DATASET ...];
// needs target/release/layout-command and `npm ci` in benchmarks/eval (elk-rs, for the reference extents).
//
// Issue #119: where the default pipeline (single pass) spends the drawing's extent. The drawing's extent along the flow
// and across it are each broken down exactly — the parts add up to the extent — from the stored layered plans and the
// boxes, so nothing is estimated:
//
// Along the flow, an area's extent is its ranks one after another and the channels between them, inside its frame. A
// rank is as long as its longest element; that element is followed down (a child area is broken down the same way), so
// the parts are those of the longest chain through the hierarchy:
//   leaf            the length of leaves
//   channel         the free space between two ranks (the tracks of the edges that change lane there)
//   channel@border  the same, where one of the two ranks holds only border or side nodes of a container
//   border rank     the thickness of a rank that holds only border or side nodes (one per container side the edges cross)
//   dummy rank      the thickness of a rank that holds only long-edge dummies
//   rank align      a rank longer than its longest element (its elements stand at different places along the flow)
//   frame start/end a container's box beyond its content: padding, and the header on the side it is drawn
// Across the flow, an area's extent is the rank with the most content (the sum of its elements' extents), inside the hull
// of all its ranks, inside its frame. That rank's elements are followed down:
//   leaf, frame start/end   as above
//   gap node–node   free space between two neighbours of which at most one is a container
//   gap area–area   free space between two neighbouring containers (sibling areas stacked across the flow)
//   gap node–dummy, gap dummy–dummy, dummy track, border track   the long edges and border nodes in that rank
//   across slack    the hull of all ranks beyond that rank's span (ranks offset against each other, or a rank with
//                   less content that the alignment stretches wider)
// Uniform task, as `elk-spacing.mjs` and `hierarchy-mode.mjs` pose it, so elk-rs's extents are the reference.
import {readFileSync, writeFileSync, existsSync} from 'node:fs';
import {join} from 'node:path';
import ElkRs from '@openedges/elk-layout';
import {arqlet, ROOT} from './lib.mjs';
import {elkLayout} from './elk-adapter.mjs';

const args = process.argv.slice(2);
const option = (name, fallback) => { const i = args.indexOf(name); return i >= 0 ? args.splice(i, 2)[1] : fallback; };
const LIMIT = Number(option('--limit', 150));
const JSON_OUT = option('--json', null);
const LAYERED = ['activity-order', 'arqlet-pipeline', 'bpmn-pools', 'can-bus', 'cloud-architecture', 'deployment-cluster',
  'er-diagram', 'layerweave-code', 'mixed-large', 'network-vlan', 'org-chart', 'petri-net', 'state-order', 'state-tcp',
  'synthetic-2k', 'uml-class', 'workflow-dag'];
const selected = args.length ? args : LAYERED;
const DUMMY = '\u0001dummy:';
const BORDERISH = ['\u0001dummy:border:', '\u0001dummy:side-in:', '\u0001dummy:side-out:'];
const isDummy = id => id.startsWith(DUMMY);
const isBorderish = id => BORDERISH.some(p => id.startsWith(p));
const ROOT_AREA = '<root>';

function budget(s) {
  const plans = s.layeredPlans ?? {};
  const children = new Map();
  for (const n of s.graph.nodes) {
    const p = n.parentId ?? ROOT_AREA;
    if (!children.has(p)) children.set(p, []);
    children.get(p).push(n.id);
  }
  const lo = (r, axis) => axis === 'x' ? r.x : r.y;
  const len = (r, axis) => axis === 'x' ? r.width : r.height;
  const rectIn = (plan, id) => plan.dummies?.[id] ?? s.boxes[id];
  let missing = 0;
  const add = (b, k, v) => { b[k] = (b[k] ?? 0) + v; };

  // Breaks `size` (the element's extent on `axis`) into `b`; the parts sum to `size`.
  function element(id, axis, size, b) {
    if (isDummy(id)) { add(b, isBorderish(id) ? 'border track' : 'dummy track', size); return; }
    if (!children.has(id)) { add(b, 'leaf', size); return; }
    const plan = plans[id];
    if (!plan) { add(b, 'unplanned area', size); return; }
    area(plan, axis, size, b);
  }

  // Breaks an area's extent on `axis` into `b`. `size` is the area's box extent (null for the root: its hull).
  function area(plan, axis, size, b) {
    const flow = plan.vertical ? 'y' : 'x';
    const ranks = plan.ranks.map(ids => ids
      .map(id => ({id, r: rectIn(plan, id)}))
      .filter(e => { if (!e.r) missing++; return e.r; }));
    const all = ranks.flat();
    if (!all.length) { add(b, 'empty area', size ?? 0); return; }
    const hullLo = Math.min(...all.map(e => lo(e.r, axis)));
    const hullHi = Math.max(...all.map(e => lo(e.r, axis) + len(e.r, axis)));
    if (size !== null) {
      add(b, 'frame start', hullLo);
      add(b, 'frame end', size - hullHi);
    }
    const inner = hullHi - hullLo;
    let used = 0;
    if (axis === flow) {
      const spans = ranks.filter(r => r.length).map(r => {
        const s0 = Math.min(...r.map(e => lo(e.r, axis)));
        const s1 = Math.max(...r.map(e => lo(e.r, axis) + len(e.r, axis)));
        const kind = r.every(e => isBorderish(e.id)) ? 'border' : r.every(e => isDummy(e.id)) ? 'dummy' : 'nodes';
        return {s0, s1, kind, r};
      }).sort((a, c) => a.s0 - c.s0);
      spans.forEach((sp, i) => {
        const rankLen = sp.s1 - sp.s0;
        if (sp.kind === 'border') add(b, 'border rank', rankLen);
        else if (sp.kind === 'dummy') add(b, 'dummy rank', rankLen);
        else {
          const longest = sp.r.filter(e => !isDummy(e.id)).sort((p, q) => len(q.r, axis) - len(p.r, axis))[0];
          const l = len(longest.r, axis);
          element(longest.id, axis, l, b);
          add(b, 'rank align', rankLen - l);
        }
        used += rankLen;
        const next = spans[i + 1];
        if (next) {
          const gap = next.s0 - sp.s1;
          add(b, sp.kind === 'border' || next.kind === 'border' ? 'channel@border' : 'channel', gap);
          used += gap;
        }
      });
    } else {
      // The rank that needs the most room: the most content (the sum of its elements' extents), not the widest span —
      // a rank of two long-edge dummies at the far ends spans everything but is stretched by the others, not their cause.
      let widest = null;
      for (const r of ranks) {
        if (!r.length) continue;
        const s0 = Math.min(...r.map(e => lo(e.r, axis)));
        const s1 = Math.max(...r.map(e => lo(e.r, axis) + len(e.r, axis)));
        const content = r.reduce((a, e) => a + len(e.r, axis), 0);
        if (!widest || content > widest.content || (content === widest.content && s1 - s0 > widest.s1 - widest.s0)) {
          widest = {s0, s1, r, content};
        }
      }
      add(b, 'across slack', inner - (widest.s1 - widest.s0));
      used += inner - (widest.s1 - widest.s0);
      const elems = widest.r.slice().sort((p, q) => lo(p.r, axis) - lo(q.r, axis));
      elems.forEach((e, i) => {
        element(e.id, axis, len(e.r, axis), b);
        used += len(e.r, axis);
        const next = elems[i + 1];
        if (!next) return;
        const gap = lo(next.r, axis) - (lo(e.r, axis) + len(e.r, axis));
        const da = isDummy(e.id), dn = isDummy(next.id);
        const areaA = !da && children.has(e.id), areaN = !dn && children.has(next.id);
        add(b, da && dn ? 'gap dummy–dummy' : da || dn ? 'gap node–dummy' : areaA && areaN ? 'gap area–area' : 'gap node–node', gap);
        used += gap;
      });
    }
    // Whatever the ranks leave unexplained (ranks that overlap along the flow, or a widest rank whose elements overlap).
    add(b, 'residual', inner - used);
  }

  const root = plans[ROOT_AREA];
  if (!root) return null;
  const out = {};
  for (const axis of ['x', 'y']) {
    const b = {};
    area(root, axis, null, b);
    out[axis] = b;
  }
  // The root's widest rank across the flow: how many of its elements are areas.
  const across = root.vertical ? 'x' : 'y';
  let widest = null;
  for (const r of root.ranks) {
    const es = r.map(id => ({id, r: rectIn(root, id)})).filter(e => e.r);
    if (!es.length) continue;
    const content = es.reduce((a, e) => a + len(e.r, across), 0);
    if (!widest || content > widest.content) widest = {content, n: es.length, areas: es.filter(e => children.has(e.id)).length};
  }
  return {buckets: out, vertical: root.vertical, missing, rootWidest: widest};
}

const extentOf = d => {
  const xs = d.nodes.flatMap(n => [n.x, n.x + n.w]), ys = d.nodes.flatMap(n => [n.y, n.y + n.h]);
  return {x: Math.max(...xs) - Math.min(...xs), y: Math.max(...ys) - Math.min(...ys)};
};
const pose = name => {
  const graph = JSON.parse(readFileSync(join(ROOT, 'benchmarks/datasets', `${name}.json`), 'utf8'));
  delete graph.buses; delete graph.defaultRouting; delete graph.containerRouting;
  graph.containerDirections = {}; graph.containerStrategies = {}; graph.defaultStrategy = 'none';
  if (!['lr', 'tb'].includes(graph.defaultDirection)) graph.defaultDirection = 'lr';
  return graph;
};

const rows = [];
for (const name of selected) {
  if (!existsSync(join(ROOT, 'benchmarks/datasets', `${name}.json`))) continue;
  const view = arqlet({op: 'import', graph: pose(name), limit: LIMIT}).snapshot.graph;
  const s = arqlet({op: 'import', graph: {...view}, limit: view.nodes.length}).snapshot;
  const singlePass = !(s.layoutDiagnostics ?? []).some(d => d.startsWith('Single pass was not used'));
  const b = budget(s);
  let elk = null;
  try { elk = extentOf(await elkLayout(structuredClone(view), new ElkRs())); } catch { elk = null; }
  if (!b) { console.error(`${name}: no root plan`); continue; }
  const along = b.vertical ? 'y' : 'x', across = b.vertical ? 'x' : 'y';
  const sum = o => Object.values(o).reduce((a, v) => a + v, 0);
  rows.push({name, singlePass, vertical: b.vertical, missing: b.missing, rootWidest: b.rootWidest,
    along: b.buckets[along], across: b.buckets[across], alongExtent: sum(b.buckets[along]), acrossExtent: sum(b.buckets[across]),
    elkAlong: elk?.[along] ?? null, elkAcross: elk?.[across] ?? null});
}

const KEYS_ALONG = ['leaf', 'channel', 'channel@border', 'border rank', 'dummy rank', 'rank align', 'frame start', 'frame end'];
const KEYS_ACROSS = ['leaf', 'gap node–node', 'gap area–area', 'gap node–dummy', 'gap dummy–dummy', 'dummy track', 'border track',
  'across slack', 'frame start', 'frame end'];
const r0 = v => Math.round(v);
const table = (title, side, keys) => {
  const others = r => Object.entries(r[side]).filter(([k]) => !keys.includes(k)).reduce((a, [, v]) => a + v, 0);
  console.log(`\n**${title}** (units; the parts add up to the extent)\n`);
  console.log(`| dataset | single pass | extent | elk-rs | ${keys.join(' | ')} | other |`);
  console.log(`|---|---|---:|---:|${keys.map(() => '---:').join('|')}|---:|`);
  const tot = {extent: 0, elk: 0, other: 0};
  for (const r of rows) {
    const ext = side === 'along' ? r.alongExtent : r.acrossExtent, elk = side === 'along' ? r.elkAlong : r.elkAcross;
    tot.extent += ext; tot.elk += elk ?? 0; tot.other += others(r);
    for (const k of keys) tot[k] = (tot[k] ?? 0) + (r[side][k] ?? 0);
    console.log(`| ${r.name} | ${r.singlePass ? 'yes' : '**no**'} | ${r0(ext)} | ${elk === null ? '–' : r0(elk)} | ${keys.map(k => r0(r[side][k] ?? 0)).join(' | ')} | ${r0(others(r))} |`);
  }
  const pct = v => `${r0(v)} (${Math.round(100 * v / tot.extent)} %)`;
  console.log(`| **sum** | | ${r0(tot.extent)} | ${r0(tot.elk)} | ${keys.map(k => pct(tot[k] ?? 0)).join(' | ')} | ${pct(tot.other)} |`);
};
table('Along the flow', 'along', KEYS_ALONG);
table('Across the flow', 'across', KEYS_ACROSS);
console.log('\n**The root\'s fullest rank across the flow**\n');
console.log('| dataset | elements | of them areas |');
console.log('|---|---:|---:|');
for (const r of rows) console.log(`| ${r.name} | ${r.rootWidest?.n ?? '–'} | ${r.rootWidest?.areas ?? '–'} |`);
const off = rows.filter(r => !r.singlePass).map(r => r.name), miss = rows.filter(r => r.missing).map(r => `${r.name} (${r.missing})`);
if (off.length) console.log(`\nSingle pass not used on: ${off.join(', ')}`);
if (miss.length) console.log(`Plan elements without a box: ${miss.join(', ')}`);
if (JSON_OUT) writeFileSync(JSON_OUT, JSON.stringify({kind: 'area-budget', limit: LIMIT, rows}, null, 1) + '\n');
