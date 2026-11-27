// Before and after of #229 (ADR 0029 A1): the second routing pass of two builds on the same imports, with whole answers
// compared. Public and deterministic inputs: the bus meshes of `bus-mesh.mjs` (#212) and the benchmark datasets.
//
// Each request is an `import` of the whole graph, run in its own `layout-command` process with `timings: true`. A build
// with `second-pass-timer.patch` prints `PHASE secondPass MS` on standard error when the second routing pass runs; the
// row keeps it. The builds run in turn (the first build alternates per repetition), so load changes hit both alike.
//
// Usage: node benchmarks/experiments/overlap-index.mjs --base BIN --new BIN [options]
//   --cases LIST     UNITS/BUSES/STRATEGY/STYLE/CHILDREN/EDGES, comma-separated (bus meshes)
//   --datasets LIST  names of benchmarks/datasets/NAME.json, imported with limit 100
//   --reps N         repetitions per build (default 5)
//   --out FILE       appends one JSON line per run; runs already in FILE (same build, request and repetition) are skipped
//   --timeout S      time limit of one process in seconds (default 600); a run that hits it is tried once more
import {spawnSync} from 'node:child_process';
import {appendFileSync, existsSync, readFileSync} from 'node:fs';
import os from 'node:os';
import {createHash} from 'node:crypto';
import {fileURLToPath} from 'node:url';
import path from 'node:path';

const args = Object.fromEntries(
  process.argv.slice(2).reduce((pairs, a, i, all) => (a.startsWith('--') ? [...pairs, [a.slice(2), all[i + 1]]] : pairs), []),
);
const builds = {base: args.base, new: args.new};
if (!builds.base || !builds.new) throw new Error('--base and --new are required');
const reps = Number(args.reps ?? 5);
const timeout = Number(args.timeout ?? 600) * 1000;
const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, '../..');
const out = args.out;
const done = new Set(
  out && existsSync(out)
    ? readFileSync(out, 'utf8').split('\n').filter(Boolean).map(l => {
        const r = JSON.parse(l);
        return `${r.build} ${r.request} ${r.rep}`;
      })
    : [],
);

const requests = [];
for (const c of (args.cases ?? '').split(',').filter(Boolean)) {
  const res = spawnSync(process.execPath, [path.join(here, 'bus-mesh.mjs'), ...c.split('/')], {encoding: 'utf8', maxBuffer: 1 << 28});
  if (res.status !== 0) throw new Error(res.stderr);
  requests.push([c, JSON.parse(res.stdout)]);
}
for (const name of (args.datasets ?? '').split(',').filter(Boolean)) {
  const graph = JSON.parse(readFileSync(path.join(root, 'benchmarks/datasets', `${name}.json`), 'utf8'));
  requests.push([name, {op: 'import', graph, limit: 100}]);
}

const hash = value => createHash('sha256').update(JSON.stringify(value)).digest('hex').slice(0, 16);

function run(binary, body) {
  const call = () =>
    spawnSync(binary, [], {input: JSON.stringify({...body, timings: true}), encoding: 'utf8', maxBuffer: 1 << 30, timeout, killSignal: 'SIGKILL'});
  let result = call();
  // Once more after the time limit: a freshly started process has been seen to hang before it runs.
  if (result.error?.code === 'ETIMEDOUT' || result.signal === 'SIGKILL') result = call();
  if (result.error?.code === 'ETIMEDOUT' || result.signal) return {timedOut: true};
  if (result.status !== 0) throw new Error(`layout-command: ${result.error ?? result.stderr}`);
  const answer = JSON.parse(result.stdout);
  if (!answer.ok) throw new Error(answer.error);
  const second = result.stderr.split('\n').find(l => l.startsWith('PHASE secondPass '));
  const s = answer.snapshot;
  return {
    totalMs: answer.timings?.total ?? null,
    secondPassMs: second ? Number(second.split(' ')[2]) : null,
    routes: s.routes.length,
    routeConflicts: s.routeConflicts?.length ?? 0,
    // The whole snapshot (the timings are outside it), and its geometry alone.
    answer: hash(s),
    geometry: hash([s.boxes, s.routes, s.busLines ?? {}]),
  };
}

for (let rep = 1; rep <= reps; rep++) {
  for (const [request, body] of requests) {
    const order = rep % 2 ? ['base', 'new'] : ['new', 'base'];
    for (const build of order) {
      if (done.has(`${build} ${request} ${rep}`)) continue;
      const load = os.loadavg()[0];
      const row = {build, request, rep, load: Math.round(load * 100) / 100, ...run(builds[build], body)};
      console.log(JSON.stringify(row));
      if (out) appendFileSync(out, JSON.stringify(row) + '\n');
    }
  }
}
