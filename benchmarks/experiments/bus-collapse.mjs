// A bus view with buses as data and containers (#150), open against the same view with every container that sits on
// only one bus collapsed. Public and deterministic.
//
// CONTAINERS areas of UNITS units each on BUSES buses. Every area has a home bus that all its units are on; every
// GATEWAY-th area also has one unit on a second bus, so it sits on two buses and stays open. The collapsed view is the
// same import with `levelOfDetail: {kinds: ["single"]}` (the kind of the one-bus areas).
//
// Usage: node benchmarks/experiments/bus-collapse.mjs [options]
//   --binary PATH     layout-command (default target/release/layout-command)
//   --sizes A,B,..    containers per case (default 20,40,80,160)
//   --units N         units per container (default 6)
//   --buses N         buses; 0: one per 8 containers, at least 3 (default 0)
//   --gateway N       every N-th container sits on two buses (default 8)
//   --views LIST      strategy/busStyle pairs (default organic/lines,layered/lines,layered/trunk,layered/bar)
//   --reps N          runs per case; the median total is reported (default 3)
//   --out FILE        append one JSON line per case and variant
//   --label TEXT      recorded in each line (which build)
//   --request SIZE/STRATEGY/STYLE/VARIANT   print that import request (VARIANT: open or collapsed) and exit
import {spawnSync} from 'node:child_process';
import {appendFileSync} from 'node:fs';
import os from 'node:os';
import {createHash} from 'node:crypto';

const args = Object.fromEntries(
  process.argv.slice(2).reduce((pairs, a, i, all) => (a.startsWith('--') ? [...pairs, [a.slice(2), all[i + 1]]] : pairs), []),
);
const binary = args.binary ?? 'target/release/layout-command';
const sizes = (args.sizes ?? '20,40,80,160').split(',').map(Number);
const units = Number(args.units ?? 6);
const gateway = Number(args.gateway ?? 8);
const reps = Number(args.reps ?? 3);
const views = (args.views ?? 'organic/lines,layered/lines,layered/trunk,layered/bar').split(',');

function graph(containers, strategy) {
  const nbus = Number(args.buses ?? 0) || Math.max(3, Math.round(containers / 8));
  let seed = 7;
  // The high bits: the low bits of this generator repeat quickly.
  const rnd = n => (seed = (seed * 1103515245 + 12345) % 2147483648, Math.floor((seed / 2147483648) * n));
  const nodes = [];
  const buses = Array.from({length: nbus}, (_, b) => ({id: `bus${b}`, kind: 'bus', label: `Bus ${b}`, taps: []}));
  const tap = (b, node) => buses[b].taps.push({edge: `t${b}-${node}`, node, toBus: true, kind: 'on'});
  for (let c = 0; c < containers; c++) {
    const home = rnd(nbus);
    const two = c % gateway === 0;
    nodes.push({id: `c${c}`, kind: two ? 'gateway' : 'single', label: `Area ${c}`});
    for (let u = 0; u < units; u++) {
      nodes.push({id: `c${c}u${u}`, kind: 'unit', label: `U${c}.${u}`, parentId: `c${c}`});
      tap(home, `c${c}u${u}`);
    }
    if (two) tap((home + 1 + rnd(nbus - 1)) % nbus, `c${c}u0`);
  }
  const out = {nodes, edges: [], buses: buses.filter(b => b.taps.length)};
  if (strategy === 'organic') Object.assign(out, {defaultStrategy: 'organic', defaultDirection: 'auto'});
  return out;
}

function request(containers, strategy, busStyle, variant) {
  const options = {busStyle};
  if (variant === 'collapsed') options.levelOfDetail = {kinds: ['single']};
  return {op: 'import', graph: graph(containers, strategy), limit: 10000, options, timings: true};
}

function run(body) {
  const call = () => spawnSync(binary, [], {input: JSON.stringify(body), encoding: 'utf8', maxBuffer: 1 << 29, timeout: 300000});
  let result = call();
  // Once more after the time limit: a freshly started process has been seen to hang before it runs.
  if (result.error?.code === 'ETIMEDOUT') result = call();
  if (result.status !== 0) throw new Error(`layout-command: ${result.error ?? result.stderr ?? result.signal}`);
  const answer = JSON.parse(result.stdout);
  if (!answer.ok) throw new Error(answer.error);
  // A binary with `bus-collapse-phases.patch` prints one line per phase: `PHASE name ms [detail]`.
  const phases = result.stderr.split('\n').filter(l => l.startsWith('PHASE ')).map(l => l.slice(6));
  return phases.length ? {...answer, phases} : answer;
}

function describe(snapshot) {
  const boxes = Object.values(snapshot.boxes);
  const width = Math.max(...boxes.map(b => b.x + b.width)) - Math.min(...boxes.map(b => b.x));
  const height = Math.max(...boxes.map(b => b.y + b.height)) - Math.min(...boxes.map(b => b.y));
  const taps = snapshot.graph.buses.reduce((n, b) => n + (b.taps?.length ?? 0), 0);
  return {
    nodes: snapshot.graph.nodes.length,
    routeConflicts: snapshot.routeConflicts?.length ?? 0,
    edges: snapshot.graph.edges.length,
    collapsed: snapshot.collapsedIds?.length ?? 0,
    taps,
    width: Math.round(width),
    height: Math.round(height),
    geometry: createHash('sha256')
      .update(JSON.stringify([snapshot.boxes, snapshot.routes, snapshot.busLines ?? {}]))
      .digest('hex')
      .slice(0, 16),
  };
}

if (args.request) {
  const [size, strategy, style, variant] = args.request.split('/');
  process.stdout.write(JSON.stringify(request(Number(size), strategy, style, variant)));
} else {
  for (const containers of sizes) {
    for (const view of views) {
      const [strategy, busStyle] = view.split('/');
      // Open and collapsed in turn, so that a change of the machine's load reaches both.
      const runs = {};
      const variants = ['open', 'collapsed'];
      for (const variant of variants) runs[variant] = [];
      for (let r = 0; r < reps; r++) for (const variant of variants) runs[variant].push(run(request(containers, strategy, busStyle, variant)));
      for (const variant of variants) {
        const sorted = runs[variant].sort((a, b) => a.timings.total - b.timings.total);
        const median = sorted[Math.floor((sorted.length - 1) / 2)];
        const row = {
          label: args.label, containers, units, buses: median.snapshot.graph.buses.length, gateway, strategy, busStyle, variant, reps,
          ...describe(median.snapshot),
          totalsMs: sorted.map(a => a.timings.total),
          timingsMs: median.timings,
          ...(median.phases ? {phases: median.phases} : {}),
          load: os.loadavg().map(l => Math.round(l * 100) / 100),
        };
        if (args.out) appendFileSync(args.out, `${JSON.stringify(row)}\n`);
        const t = median.timings;
        console.log(
          `${String(containers).padStart(4)} ${view.padEnd(14)} ${variant.padEnd(14)} nodes ${String(row.nodes).padStart(5)} taps ${String(row.taps).padStart(5)} ` +
          `${row.width}x${row.height} total ${t.total} layout ${t.layout} place ${t.placement} stress ${t.organicStress} sep ${t.separation} ` +
          `repair ${t.repair} ports ${t.portResolution} route ${t.routing} buses ${t.buses} load ${row.load[0]}` +
          (median.phases ? `\n       ${median.phases.join('; ')}` : ''),
        );
      }
    }
  }
}
