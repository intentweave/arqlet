// Where the time of a patch into a bus view goes (#212). Public and deterministic.
//
// For each case, `bus-mesh.mjs` gives a bus view with a drill-down (CHILDREN under members of the first bus, EDGES from
// them). The overview without the children and their edges is imported; then one `patch` adds the children and the
// edges; for comparison the whole graph is imported fresh. Each request runs in its own `layout-command` process with
// `timings: true`; a binary built with `bus-patch-phases.patch` also prints `PHASE name ms [detail]` lines, which are kept.
//
// Usage: node benchmarks/experiments/bus-patch.mjs [options]
//   --binary PATH   layout-command (default target/release/layout-command)
//   --cases LIST    UNITS/BUSES/STRATEGY/STYLE/CHILDREN/EDGES, comma-separated
//                   (default 280/107/organic/lines/480/2100,100/30/organic/lines/240/1000)
//   --reps N        runs of each request; the median total is reported (default 3)
//   --out FILE      appends one JSON line per case and request; rows already in FILE (same label, case, rep count) are skipped
//   --label TEXT    recorded in each line (which build)
//   --timeout S     time limit of one process in seconds (default 900); a run that hits it is tried once more
//   --options JSON  layout options merged into every request (for example '{"sparseRepairGrid":true}')
//   --metrics 1     also scores the median patch and fresh drawings with the `metrics` operation (routes; the bus taps are
//                   checked here for boxes they run through, deeper than half a unit)
//   --partial 1     the overview keeps every child but those of one container (the first child's parent), and the patch adds
//                   those with their edges: one more drill-down into a view that already has most of its routes
//   --env K=V       sets an environment variable for the patch requests only (a probe build reads it)
import {spawnSync} from 'node:child_process';
import {appendFileSync, existsSync, readFileSync} from 'node:fs';
import os from 'node:os';
import {createHash} from 'node:crypto';
import {fileURLToPath} from 'node:url';
import path from 'node:path';

const args = Object.fromEntries(
  process.argv.slice(2).reduce((pairs, a, i, all) => (a.startsWith('--') ? [...pairs, [a.slice(2), all[i + 1]]] : pairs), []),
);
const binary = args.binary ?? 'target/release/layout-command';
const cases = (args.cases ?? '280/107/organic/lines/480/2100,100/30/organic/lines/240/1000').split(',');
const reps = Number(args.reps ?? 3);
const timeout = Number(args.timeout ?? 900) * 1000;
const extra = args.options ? JSON.parse(args.options) : {};
const patchEnv = args.env ? {...process.env, [args.env.split('=')[0]]: args.env.split('=').slice(1).join('=')} : process.env;
const mesh = path.join(path.dirname(fileURLToPath(import.meta.url)), 'bus-mesh.mjs');

function generated(units, buses, strategy, style, children, edges) {
  const out = spawnSync(process.execPath, [mesh, units, buses, strategy, style, children, edges], {encoding: 'utf8', maxBuffer: 1 << 28});
  if (out.status !== 0) throw new Error(out.stderr);
  const request = JSON.parse(out.stdout);
  Object.assign(request.options, extra);
  return request;
}

function run(body, env = process.env) {
  const call = () =>
    spawnSync(binary, [], {input: JSON.stringify({...body, timings: true}), encoding: 'utf8', maxBuffer: 1 << 30, timeout, killSignal: 'SIGKILL', env});
  let result = call();
  // Once more after the time limit: a freshly started process has been seen to hang before it runs.
  if (result.error?.code === 'ETIMEDOUT' || result.signal === 'SIGKILL') result = call();
  if (result.error?.code === 'ETIMEDOUT' || result.signal) return {timedOut: true};
  if (result.status !== 0) throw new Error(`layout-command: ${result.error ?? result.stderr}`);
  const answer = JSON.parse(result.stdout);
  if (!answer.ok) throw new Error(answer.error);
  const phases = result.stderr.split('\n').filter(l => l.startsWith('PHASE ')).map(l => l.slice(6));
  return phases.length ? {...answer, phases} : answer;
}

const hash = value => createHash('sha256').update(JSON.stringify(value)).digest('hex').slice(0, 16);

function describe(snapshot) {
  const boxes = Object.values(snapshot.boxes);
  const width = Math.max(...boxes.map(b => b.x + b.width)) - Math.min(...boxes.map(b => b.x));
  const height = Math.max(...boxes.map(b => b.y + b.height)) - Math.min(...boxes.map(b => b.y));
  return {
    nodes: snapshot.graph.nodes.length,
    edges: snapshot.graph.edges.length,
    routes: snapshot.routes.length,
    routeConflicts: snapshot.routeConflicts?.length ?? 0,
    routingIssues: snapshot.routingIssues?.length ?? 0,
    geometryIssues: snapshot.geometryIssues?.length ?? 0,
    diagnostics: snapshot.layoutDiagnostics ?? [],
    width: Math.round(width),
    height: Math.round(height),
    // Geometry only: boxes, routes and bus lines (the revision and the source graph differ between a patch and an import).
    geometry: hash([snapshot.boxes, snapshot.routes, snapshot.busLines ?? {}]),
  };
}

// How many of the overview's units keep their place (centre within 0.5) after the patch, and how far the others moved.
function kept(before, after) {
  let same = 0;
  let moved = 0;
  let far = 0;
  for (const [id, b] of Object.entries(before.boxes)) {
    const a = after.boxes[id];
    if (!a || !/^u\d+$/.test(id)) continue;
    const d = Math.hypot(a.x + a.width / 2 - (b.x + b.width / 2), a.y + a.height / 2 - (b.y + b.height / 2));
    if (d <= 0.5) same++;
    else moved++;
    far = Math.max(far, d);
  }
  return {unitsKept: same, unitsMoved: moved, maxMove: Math.round(far)};
}

// The bus taps that run into a box other than their own node's (or a container of it) by more than half a unit.
function tapHits(snapshot) {
  const g = snapshot.graph;
  const parent = new Map(g.nodes.map(n => [n.id, n.parentId]));
  const abs = id => {
    const r = {...snapshot.boxes[id]};
    for (let p = parent.get(id); p; p = parent.get(p)) {
      r.x += snapshot.boxes[p].x;
      r.y += snapshot.boxes[p].y;
    }
    return r;
  };
  const boxes = g.nodes.filter(n => snapshot.boxes[n.id]).map(n => [n.id, abs(n.id)]);
  const tapNode = new Map((g.buses ?? []).flatMap(b => (b.taps ?? []).map(t => [t.edge, t.node])));
  let hits = 0;
  let taps = 0;
  for (const line of Object.values(snapshot.busLines ?? {})) {
    for (const [edge, points] of Object.entries(line.taps)) {
      taps++;
      const own = new Set();
      for (let at = tapNode.get(edge); at; at = parent.get(at)) own.add(at);
      const hit = boxes.some(([id, r]) => !own.has(id) && points.slice(1).some((q, i) => {
        const p = points[i];
        const [x0, x1, y0, y1] = [Math.min(p.x, q.x), Math.max(p.x, q.x), Math.min(p.y, q.y), Math.max(p.y, q.y)];
        return x1 > r.x + 0.5 && x0 < r.x + r.width - 0.5 && y1 > r.y + 0.5 && y0 < r.y + r.height - 0.5;
      }));
      if (hit) hits++;
    }
  }
  return {taps, tapsThroughBoxes: hits};
}

async function score(snapshot) {
  process.env.LAYOUT_COMMAND = path.resolve(binary);
  const lib = await import('../eval/lib.mjs');
  const m = lib.metrics(lib.normalizeArqlet(snapshot));
  return {metrics: m, ...tapHits(snapshot)};
}

const median = runs => {
  const sorted = runs.filter(r => !r.timedOut).sort((a, b) => a.timings.total - b.timings.total);
  return {sorted, median: sorted[Math.floor((sorted.length - 1) / 2)]};
};

const done = new Set(
  args.out && existsSync(args.out)
    ? readFileSync(args.out, 'utf8')
        .split('\n')
        .filter(Boolean)
        .map(l => JSON.parse(l))
        .map(r => `${r.label}|${r.case}|${r.request}|${r.reps}|${Boolean(r.partial)}`)
    : [],
);

for (const c of cases) {
  const [units, buses, strategy, style, children, edges] = c.split('/');
  if (['import', 'patch', 'fresh'].every(r => done.has(`${args.label}|${c}|${r}|${reps}|${Boolean(args.partial)}`))) {
    console.log(`${c}: done`);
    continue;
  }
  const full = generated(units, buses, strategy, style, children, edges);
  const first = full.graph.nodes.find(n => n.parentId)?.parentId;
  const added = new Set(full.graph.nodes.filter(n => n.parentId && (!args.partial || n.parentId === first)).map(n => n.id));
  const overview = structuredClone(full);
  overview.graph.nodes = overview.graph.nodes.filter(n => !added.has(n.id));
  const newEdge = e => added.has(e.source) || added.has(e.target);
  overview.graph.edges = overview.graph.edges.filter(e => !newEdge(e));
  const patch = {nodes: full.graph.nodes.filter(n => added.has(n.id)), edges: full.graph.edges.filter(newEdge)};
  const runs = {import: [], patch: [], fresh: []};
  // The three requests in turn, so that a change of the machine's load reaches all of them.
  let base;
  for (let r = 0; r < reps; r++) {
    const imported = run(overview);
    if (imported.timedOut) throw new Error(`${c}: the overview import timed out`);
    runs.import.push(imported);
    base ??= imported;
    const s = imported.snapshot;
    runs.patch.push(run({op: 'patch', snapshot: s, expectedRevision: s.revision, expectedLayoutVersion: s.layoutVersion, patch}, patchEnv));
    runs.fresh.push(run(full));
  }
  const fresh = median(runs.fresh).median;
  for (const request of ['import', 'patch', 'fresh']) {
    const {sorted, median: m} = median(runs[request]);
    const row = {
      label: args.label, case: c, request, reps, partial: Boolean(args.partial), patchNodes: patch.nodes.length, patchEdges: patch.edges.length, units: Number(units), buses: Number(buses), strategy, style,
      children: Number(children), addedEdges: Number(edges), options: extra,
      timedOut: runs[request].filter(r => r.timedOut).length,
      ...(m ? {
        ...describe(m.snapshot),
        totalsMs: sorted.map(a => Math.round(a.timings.total)),
        timingsMs: m.timings,
        ...(m.phases ? {phases: m.phases} : {}),
      } : {}),
      ...(m && request === 'patch' ? {
        ...kept(base.snapshot, m.snapshot),
        sameAsFresh: fresh ? describe(m.snapshot).geometry === describe(fresh.snapshot).geometry : null,
      } : {}),
      ...(m && args.metrics && request !== 'import' ? await score(m.snapshot) : {}),
      load: os.loadavg().map(l => Math.round(l * 100) / 100),
    };
    if (args.out) appendFileSync(args.out, `${JSON.stringify(row)}\n`);
    const t = m?.timings ?? {};
    console.log(
      `${c} ${request.padEnd(6)} total ${Math.round(t.total)} layout ${Math.round(t.layout)} place ${Math.round(t.placement)} ` +
        `sep ${Math.round(t.separation)} repair ${Math.round(t.repair)} route ${Math.round(t.routing)} buses ${Math.round(t.buses)} ` +
        `conflicts ${row.routeConflicts} ${row.width}x${row.height}` +
        (request === 'patch' ? ` kept ${row.unitsKept}/${row.unitsKept + row.unitsMoved} sameAsFresh ${row.sameAsFresh}` : '') +
        ` load ${row.load[0]}` + (m?.phases ? `\n    ${m.phases.join('; ')}` : ''),
    );
  }
}
