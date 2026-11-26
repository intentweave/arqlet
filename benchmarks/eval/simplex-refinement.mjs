// Usage: node benchmarks/eval/simplex-refinement.mjs [--pose uniform|native] [--limit 150] [--baseline LAYOUT_COMMAND]
//        [--edits 12] [--json OUT] [DATASET ...]; needs target/release/layout-command.
//
// ADR 0027, step 1 (`simplexRanksCandidate`), the checks `option-gates.mjs --refine` does not make, per view:
//   off = baseline   with the option off, the import and the refinement answer byte for byte as `--baseline` (a build
//                    without the change) does
//   default ranks    where the refinement keeps the default ranks, its snapshot equals the one without the option, but for
//                    the option itself and the count of evaluated candidates
//   simplex ranks    where it chooses the simplex ranks, boxes and routes equal an import with `simplexRanks`
//   threads          the answer is byte for byte the same with 1 and 4 threads
//   kept             after a chosen refinement, every container is collapsed (and, apart from that, a node added), up to
//                    `--edits` of each: the edit keeps `simplexRanks`; the edited view is compared with the same edit of
//                    the default drawing in objective order (overlaps, line–node hits, crossings, bends) and area
import {readFileSync, writeFileSync, existsSync} from 'node:fs';
import {spawnSync} from 'node:child_process';
import {join} from 'node:path';
import {isDeepStrictEqual} from 'node:util';
import {LAYOUT_COMMAND, normalizeArqlet, metrics, ROOT} from './lib.mjs';

const args = process.argv.slice(2);
const option = (name, fallback) => { const i = args.indexOf(name); return i >= 0 ? args.splice(i, 2)[1] : fallback; };
const LIMIT = Number(option('--limit', 150));
const POSE = option('--pose', 'uniform');
const BASELINE = option('--baseline', null);
const EDITS = Number(option('--edits', 12));
const JSON_OUT = option('--json', null);
const LAYERED = ['activity-order', 'arqlet-pipeline', 'bpmn-pools', 'can-bus', 'cloud-architecture', 'deployment-cluster',
  'er-diagram', 'layerweave-code', 'mixed-large', 'network-vlan', 'org-chart', 'petri-net', 'state-order', 'state-tcp',
  'synthetic-2k', 'uml-class', 'workflow-dag'];
const selected = args.length ? args : LAYERED;

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

// One request to a build: the answer as text, checked to be a success.
const call = (request, command = LAYOUT_COMMAND) => {
  const r = spawnSync(command, [], {input: JSON.stringify(request), encoding: 'utf8', maxBuffer: 256 * 1024 * 1024,
    timeout: Number(process.env.ARQLET_TIMEOUT_MS ?? 600000), killSignal: 'SIGKILL'});
  if (r.error) throw new Error(`${command}: ${r.error.message}`);
  if (r.status !== 0) throw new Error(r.stderr);
  const response = JSON.parse(r.stdout);
  if (!response.ok) throw new Error(response.error);
  return {text: r.stdout, snapshot: response.snapshot};
};
const attempt = f => { try { return f(); } catch (error) { return {error: String(error.message ?? error).split('\n')[0].slice(0, 160)}; } };
const at = s => ({snapshot: s, expectedRevision: s.revision, expectedLayoutVersion: s.layoutVersion});
const refine = (s, threads = 1, command = LAYOUT_COMMAND) => call({op: 'refine', ...at(s), threads}, command);
const geometry = s => ({boxes: s.boxes, routes: s.routes, resolvedPorts: s.resolvedPorts, labels: s.labels});
const GATES = ['overlaps', 'hits', 'crossings', 'bends'];
const score = s => {
  const d = normalizeArqlet(s), m = metrics(d);
  const xs = d.nodes.flatMap(n => [n.x, n.x + n.w]), ys = d.nodes.flatMap(n => [n.y, n.y + n.h]);
  return {overlaps: m.siblingOverlaps, hits: m.lineNodeHits, crossings: m.crossings + m.sharedEndpointCrossings,
    bends: m.bends, area: (Math.max(...xs) - Math.min(...xs)) * (Math.max(...ys) - Math.min(...ys))};
};
const compare = (a, b) => { for (const k of GATES) if (a[k] !== b[k]) return a[k] < b[k] ? 'better' : 'worse'; return 'equal'; };

const rows = [];
console.log(`pose ${POSE}, limit ${LIMIT}\n`);
console.log('| dataset | single pass | strategy | off = baseline | default ranks | simplex = import | threads | edits kept | better / equal / worse | area of edits |');
console.log('|---|---|---|---|---|---|---|---:|---|---:|');
for (const name of selected) {
  if (!existsSync(join(ROOT, 'benchmarks/datasets', `${name}.json`))) continue;
  const first = call({op: 'import', graph: load(name), limit: LIMIT});
  const view = first.snapshot.graph;
  const base = POSE === 'native' ? first.snapshot.options ?? {} : {};
  const importing = (options, command) => call({op: 'import', graph: view, limit: view.nodes.length, options: {...base, ...options}}, command);
  const row = {name};
  const off = importing({});
  const offRefined = attempt(() => refine(off.snapshot));
  if (BASELINE) {
    const was = importing({}, BASELINE);
    const wasRefined = attempt(() => refine(was.snapshot, 1, BASELINE));
    row.baseline = was.text === off.text && wasRefined.text === offRefined.text && wasRefined.error === offRefined.error;
  }
  const on = importing({simplexRanksCandidate: true});
  const onRefined = attempt(() => refine(on.snapshot));
  if (offRefined.error || onRefined.error) {
    // A view whose refinement fails: it must fail the same with the option.
    row.error = offRefined.error ?? onRefined.error;
    row.sameError = offRefined.error === onRefined.error;
    rows.push(row);
    console.log(`| ${name} | – | refinement fails ${row.sameError ? 'with and without the option' : 'with one arm only'}: ${row.error} | ${row.baseline === undefined ? '–' : row.baseline ? 'yes' : '**no**'} | – | – | – | – | – | – |`);
    continue;
  }
  row.threads = onRefined.text === refine(on.snapshot, 4).text;
  const result = onRefined.snapshot;
  row.strategy = result.optimization.strategy;
  if (row.strategy === 'simplex-ranks') {
    const ranked = importing({simplexRanks: true}).snapshot;
    row.simplexIsImport = result.options.simplexRanks === true && isDeepStrictEqual(geometry(result), geometry(ranked));
    const parents = new Set(view.nodes.map(n => n.parentId).filter(Boolean));
    const containers = result.graph.nodes.filter(n => parents.has(n.id)).slice(0, EDITS).map(n => n.id);
    const leaves = result.graph.nodes.filter(n => !parents.has(n.id)).slice(0, EDITS).map(n => n.id);
    const edits = [...containers.map(id => ({op: 'toggle', nodeId: id})), ...leaves.map(id => ({op: 'add', sourceId: id, label: 'added'}))];
    const tally = {made: 0, kept: 0, better: 0, equal: 0, worse: 0, areaKept: 0, areaDefault: 0, failed: 0};
    for (const edit of edits) {
      try {
        const kept = call({...edit, ...at(result)}).snapshot;
        const plain = call({...edit, ...at(offRefined.snapshot)}).snapshot;
        const a = score(kept), b = score(plain);
        tally.made++;
        if (kept.options?.simplexRanks === true) tally.kept++;
        tally[compare(a, b)]++;
        tally.areaKept += a.area; tally.areaDefault += b.area;
      } catch (error) { tally.failed++; tally.error = String(error.message ?? error).split('\n')[0].slice(0, 160); }
    }
    row.edits = tally;
  } else {
    // But for the options (their input's, with the option) and the count of evaluated candidates, the snapshot is the one
    // without the option.
    const strip = s => { const c = structuredClone(s); delete c.options; delete c.optimization.candidatesEvaluated; return c; };
    row.defaultRanks = result.options.simplexRanks === undefined && isDeepStrictEqual(result.options, on.snapshot.options) &&
      isDeepStrictEqual(strip(result), strip(offRefined.snapshot));
  }
  rows.push(row);
  const yes = v => v === undefined ? '–' : v ? 'yes' : '**no**';
  const e = row.edits;
  row.singlePass = Boolean(result.layoutDiagnostics?.some(d => d.startsWith('Single pass: ')));
  console.log(`| ${name} | ${row.singlePass ? 'yes' : 'no'} | ${row.strategy} | ${yes(row.baseline)} | ${yes(row.defaultRanks)} | ${yes(row.simplexIsImport)} | ${yes(row.threads)} | ` +
    (e ? `${e.kept} of ${e.made}${e.failed ? ` (${e.failed} failed)` : ''} | ${e.better} / ${e.equal} / ${e.worse} | ${e.areaDefault ? ((e.areaKept / e.areaDefault - 1) * 100).toFixed(1) : '–'} % |` : '– | – | – |'));
}
const total = k => rows.reduce((s, r) => s + (r.edits?.[k] ?? 0), 0);
console.log(`\nViews: ${rows.length}; simplex ranks chosen on ${rows.filter(r => r.strategy === 'simplex-ranks').length}.`);
console.log(`Edits after a chosen refinement: ${total('made')} made, ${total('kept')} keep the ranks, ${total('failed')} failed; against the same edit of the default drawing ${total('better')} better, ${total('equal')} equal, ${total('worse')} worse; area ${total('areaDefault') ? ((total('areaKept') / total('areaDefault') - 1) * 100).toFixed(1) : '–'} %.`);
const failed = rows.filter(r => [r.baseline, r.sameError, r.defaultRanks, r.simplexIsImport, r.threads].includes(false) || (r.edits && (r.edits.failed || r.edits.kept !== r.edits.made)));
console.log(`Failed checks: ${failed.length ? failed.map(r => r.name).join(', ') : 'none'}.`);
if (JSON_OUT) writeFileSync(JSON_OUT, JSON.stringify({kind: 'simplex-refinement', pose: POSE, limit: LIMIT, baseline: Boolean(BASELINE), rows}, null, 1) + '\n');
process.exit(failed.length ? 1 : 0);
