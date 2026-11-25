// Pins in single pass (ADR 0016, amendment, step P1): pins leaves of the example datasets by hand and measures the
// drawing with `pinsInSinglePass` against the same pin without the option, the unpinned drawing and a control.
//
// Usage: node benchmarks/eval/pins-single-pass.mjs LAYOUT_COMMAND [--json OUT] [DATASET ...]
// LAYOUT_COMMAND is a release build of `layout-command`. The datasets are `benchmarks/datasets/*.json` (generate them
// first with `python3 benchmarks/generate.py --no-manifest`). Per dataset up to `NODES` leaves of areas with a ranked
// plan are pinned, each in three places: further across the flow, in the band of another rank, and both.
import {execFileSync} from 'node:child_process';
import {readFileSync, writeFileSync} from 'node:fs';

const NODES = 4;
const DATASETS = ['can-bus', 'activity-order', 'bpmn-pools', 'cloud-architecture', 'deployment-cluster', 'er-diagram',
  'fixed-ports', 'layerweave-code', 'metro-map', 'org-chart', 'petri-net', 'self-loops', 'state-order', 'state-tcp',
  'uml-class', 'vehicle-buses', 'vehicle-trunk', 'vehicle-zonal', 'workflow-dag'];

const args = process.argv.slice(2);
const command = args.shift();
const out = args[0] === '--json' ? args.splice(0, 2)[1] : null;
const datasets = args.length ? args : DATASETS;

function run(request) {
  const started = process.hrtime.bigint();
  const text = execFileSync(command, {input: JSON.stringify(request), maxBuffer: 1 << 30, encoding: 'utf8'});
  const reply = JSON.parse(text);
  if (!reply.ok) throw new Error(reply.error);
  return {...reply, ms: Number(process.hrtime.bigint() - started) / 1e6};
}

function absolute(snapshot) {
  const parents = new Map(snapshot.graph.nodes.map(n => [n.id, n.parentId ?? null]));
  return id => {
    const box = {...snapshot.boxes[id]};
    for (let p = parents.get(id); p; p = parents.get(p)) {
      box.x += snapshot.boxes[p].x;
      box.y += snapshot.boxes[p].y;
    }
    return box;
  };
}

function drawingOf(snapshot) {
  const at = absolute(snapshot);
  const containers = new Set(snapshot.graph.nodes.map(n => n.parentId).filter(Boolean));
  const routes = new Map(snapshot.routes.map(r => [r.id, r]));
  const ports = new Map((snapshot.resolvedPorts ?? []).map(p => [`${p.edgeId}\0${p.endpoint}`, p.id]));
  const nodes = snapshot.graph.nodes.map(n => ({id: n.id, parentId: n.parentId ?? null, container: containers.has(n.id), ...at(n.id)}));
  const edges = [];
  let missing = 0;
  for (const e of snapshot.graph.edges) {
    if (e.target == null) continue;
    const route = routes.get(e.id);
    if (route?.bundle_of) continue;
    if (!route || route.points.length < 2) { missing++; continue; }
    edges.push({id: e.id, source: e.source, target: e.target, points: route.points,
      sourcePort: ports.get(`${e.id}\0source`) ?? null, targetPort: ports.get(`${e.id}\0target`) ?? null});
  }
  const buses = (snapshot.graph.buses ?? []).map(b => b.edges).filter(e => e?.length);
  return {drawing: {nodes, edges, buses}, missing};
}

// Whether a route starts nearer its source than its target and ends nearer its target: logical direction.
function reversedRoutes(snapshot) {
  const at = absolute(snapshot);
  const distance = (p, r) => Math.hypot(Math.max(r.x - p.x, 0, p.x - r.x - r.width), Math.max(r.y - p.y, 0, p.y - r.y - r.height));
  let count = 0;
  for (const route of snapshot.routes) {
    const e = snapshot.graph.edges.find(x => x.id === route.id);
    if (!e || e.target == null || e.target === e.source || route.points.length < 2) continue;
    const [a, b] = [route.points[0], route.points[route.points.length - 1]];
    const [s, t] = [at(e.source), at(e.target)];
    if (distance(a, s) > distance(a, t) + 1e-6 && distance(b, t) > distance(b, s) + 1e-6) count++;
  }
  return count;
}

function measure(snapshot, node, pin) {
  const {drawing, missing} = drawingOf(snapshot);
  const m = run({op: 'metrics', drawing}).metrics;
  const box = snapshot.boxes[node];
  pin ??= box;
  const parent = snapshot.graph.nodes.find(n => n.id === node).parentId ?? '<root>';
  const vertical = snapshot.layeredPlans?.[parent]?.vertical;
  const [dx, dy] = [box.x - pin.x, box.y - pin.y];
  const said = snapshot.layoutDiagnostics ?? [];
  return {
    singlePass: said.some(d => d.startsWith('Single pass:')),
    // The pipeline before the hierarchical one, which a leaf pin selects without the option.
    previousPipeline: said.some(d => d.includes('pinned leaf placement')),
    geometryIssues: snapshot.geometryIssues?.length ?? 0,
    routingIssues: snapshot.routingIssues?.length ?? 0,
    // Explicit buses, or edges of them, that are not drawn as a bus (a routing issue or a diagnostic).
    busIssues: [...(snapshot.routingIssues ?? []), ...said].filter(i => i.startsWith('Bus ')).length,
    missing, reversed: reversedRoutes(snapshot),
    lineNodeHits: m.lineNodeHits, containerTraversals: m.containerTraversals, crossings: m.crossings, bends: m.bends,
    sharedLinePairs: m.sharedLinePairs, area: Math.round(m.area),
    across: Math.round(Math.abs(vertical ? dx : dy) * 10) / 10,
    along: Math.round(Math.abs(vertical ? dy : dx) * 10) / 10,
  };
}

function cases(snapshot) {
  const containers = new Set(snapshot.graph.nodes.map(n => n.parentId).filter(Boolean));
  const eligible = [];
  for (const n of [...snapshot.graph.nodes].sort((a, b) => (a.id < b.id ? -1 : 1))) {
    if (containers.has(n.id)) continue;
    const plan = snapshot.layeredPlans?.[n.parentId ?? '<root>'];
    if (!plan) continue;
    const ranks = plan.ranks.map(r => r.filter(id => snapshot.boxes[id])).filter(r => r.length);
    const rank = ranks.findIndex(r => r.includes(n.id));
    if (rank >= 0 && ranks.length > 1) eligible.push({node: n.id, plan, ranks, rank});
  }
  const step = Math.max(1, Math.floor(eligible.length / NODES));
  return eligible.filter((_, i) => i % step === 0).slice(0, NODES).flatMap(({node, plan, ranks, rank}) => {
    const box = snapshot.boxes[node];
    const other = snapshot.boxes[ranks[rank + 1 < ranks.length ? rank + 1 : rank - 1][0]];
    const across = plan.vertical ? {x: box.x + 150, y: box.y} : {x: box.x, y: box.y + 150};
    const along = plan.vertical ? {x: box.x, y: other.y} : {x: other.x, y: box.y};
    const both = plan.vertical ? {x: across.x, y: along.y} : {x: along.x, y: across.y};
    return [{node, kind: 'across', pin: across}, {node, kind: 'rank', pin: along}, {node, kind: 'both', pin: both}];
  });
}

const rows = [];
for (const name of datasets) {
  const graph = JSON.parse(readFileSync(new URL(`../datasets/${name}.json`, import.meta.url)));
  const imported = run({op: 'import', graph, limit: 10000});
  const base = imported.snapshot;
  const chosen = cases(base);
  // The same view with the option already on, as a host has it when a node is pinned: the pin is then the only change, and
  // the engine places again only what holds the pinned node, on both sides.
  const opted = chosen.length ? run({op: 'import', graph, limit: 10000, options: {pinsInSinglePass: true}}).snapshot : null;
  if (!chosen.length) { console.error(`${name}: no leaf in a ranked plan, skipped`); continue; }
  for (const c of chosen) {
    const row = {dataset: name, ...c, unpinned: {...measure(base, c.node, null), ms: Math.round(imported.ms)}};
    // The control: the same local placement without any pin (the same node asked one unit taller).
    const own = base.boxes[c.node];
    const resized = run({op: 'configure', snapshot: base, expectedRevision: base.revision, expectedLayoutVersion: base.layoutVersion ?? 0,
      options: {nodeSizes: {[c.node]: {width: own.width, height: own.height + 1}}}});
    row.control = {...measure(resized.snapshot, c.node, null), ms: Math.round(resized.ms)};
    for (const [key, from] of [['before', base], ['option', opted]]) {
      try {
        const reply = run({op: 'configure', snapshot: from, expectedRevision: from.revision,
          expectedLayoutVersion: from.layoutVersion ?? 0, options: {pins: {[c.node]: c.pin}}});
        row[key] = {...measure(reply.snapshot, c.node, c.pin), ms: Math.round(reply.ms)};
      } catch (error) {
        row[key] = {error: String(error.message ?? error)};
      }
    }
    rows.push(row);
    console.error(`${name} ${c.node} ${c.kind}`);
  }
}
if (out) writeFileSync(out, rows.map(r => JSON.stringify(r)).join('\n') + '\n');

const keys = ['geometryIssues', 'routingIssues', 'busIssues', 'missing', 'reversed', 'lineNodeHits',
  'containerTraversals', 'sharedLinePairs', 'crossings', 'bends'];
const sides = ['unpinned', 'control', 'before', 'option'];
console.log('Each cell: the drawing as imported / after the same local change without a pin (control) / with the pin, option off / with it on, summed over the cases.\n');
console.log(`| dataset | cases | single pass | previous pipeline | ${keys.join(' | ')} | area (mean, 1000 units²) | along from the pin with the option, mean / max | across with the option, max |`);
console.log(`|---|---|---|---|${keys.map(() => '---').join('|')}|---|---|---|`);
for (const name of [...new Set(rows.map(r => r.dataset)), 'total']) {
  const of = rows.filter(r => (name === 'total' || r.dataset === name) && !r.before.error && !r.option.error);
  const sum = (side, key) => of.reduce((a, r) => a + Number(r[side][key]), 0);
  const area = side => Math.round(sum(side, 'area') / of.length / 1000);
  console.log(`| ${name} | ${of.length} | ` + ['singlePass', 'previousPipeline', ...keys].map(k => sides.map(s => sum(s, k)).join(' / ')).join(' | ') +
    ` | ${sides.map(area).join(' / ')} | ${(sum('option', 'along') / of.length).toFixed(0)} / ${Math.max(...of.map(r => r.option.along)).toFixed(0)}` +
    ` | ${Math.max(...of.map(r => r.option.across)).toFixed(0)} |`);
}
console.log('\nTime of one request (a process per request), milliseconds, median over the cases of a dataset:\n');
console.log('| dataset | import | control | configure with the pin, option off | option on |\n|---|---|---|---|---|');
for (const name of new Set(rows.map(r => r.dataset))) {
  const of = rows.filter(r => r.dataset === name && !r.before.error && !r.option.error);
  const median = side => of.map(r => r[side].ms).sort((a, b) => a - b)[Math.floor(of.length / 2)];
  console.log(`| ${name} | ${sides.map(median).join(' | ')} |`);
}
for (const r of rows) for (const side of ['before', 'option']) if (r[side].error) console.log(`error: ${r.dataset} ${r.node} ${r.kind} ${side}: ${r[side].error}`);
