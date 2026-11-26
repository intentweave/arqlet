// A bus view with buses as data and containers (#150), open against the same view with every container that sits on
// only one bus collapsed. Public and deterministic.
//
// CONTAINERS areas of UNITS units each on BUSES buses. Every area has a home bus that all its units are on; every
// GATEWAY-th area also has one unit on a second bus, so it sits on two buses and stays open. The collapsed view is the
// same import with `levelOfDetail: {kinds: ["single"]}` (the kind of the one-bus areas).
//
// With `--skip 1` both views also run with `skipTapReroute`, and each such row lists the snapshot fields that differ from
// the same view without it (`differs`; `options` aside) after the import and, with `--ops move`, after a move of the first
// node (`moveDiffers`); `--ops move,refine` also compares a refinement of that (`refineDiffers`; slow on large views).
// An operation that fails is recorded (`moveFailed`, `refineFailed`: the messages without and with the option). Where
// the drawing differs after an operation, `moveQuality` and `refineQuality` give both drawings' size, bends, line length,
// lines through boxes and routing issues, without and with the option.
//
// Usage: node benchmarks/experiments/bus-collapse.mjs [options]
//   --binary PATH     layout-command (default target/release/layout-command)
//   --sizes A,B,..    containers per case (default 20,40,80,160)
//   --units N         units per container (default 6)
//   --buses N         buses; 0: one per 8 containers, at least 3 (default 0)
//   --gateway N       every N-th container sits on two buses (default 8)
//   --views LIST      strategy/busStyle pairs (default organic/lines,layered/lines,layered/trunk,layered/bar)
//   --reps N          runs per case; the median total is reported (default 3)
//   --skip 1          also measure with `skipTapReroute`
//   --ops LIST        with --skip: compare the two also after `move`, or `move,refine`
//   --out FILE        append one JSON line per case and variant
//   --label TEXT      recorded in each line (which build)
//   --request SIZE/STRATEGY/STYLE/VARIANT   print that import request (VARIANT: open or collapsed, with or without +skip) and exit
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
const skip = args.skip === '1';
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
  if (variant.startsWith('collapsed')) options.levelOfDetail = {kinds: ['single']};
  if (variant.endsWith('+skip')) options.skipTapReroute = true;
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

// The snapshot fields that differ between two answers (their options aside).
function differing(a, b) {
  const keys = new Set([...Object.keys(a), ...Object.keys(b)]);
  keys.delete('options');
  return [...keys].filter(k => JSON.stringify(a[k]) !== JSON.stringify(b[k])).sort();
}

function on(snapshot, op) {
  return run({...op, snapshot, expectedRevision: snapshot.revision, expectedLayoutVersion: snapshot.layoutVersion}).snapshot;
}

// A move of the view's first node by 400 units (a gesture of two steps), and with `refine` a refinement of the result.
// An operation that fails ends the list with its message.
function operations(snapshot, refine) {
  const done = [];
  try {
    steps(snapshot, refine, done);
  } catch (e) {
    const panic = e.message.match(/panicked at (\S+?):?\n/)?.[1];
    done.push({failed: panic ? `panicked at ${panic}` : e.message.split('\n')[0]});
  }
  return done;
}

function steps(snapshot, refine, done) {
  const node = snapshot.graph.nodes.find(n => !n.parentId).id;
  const position = {x: snapshot.boxes[node].x + 400, y: snapshot.boxes[node].y};
  let moved = snapshot;
  for (const [seq, phase] of [[1, 'begin'], [2, 'finish']]) moved = on(moved, {op: 'move', gestureId: 'g', seq, phase, node, position, release: false});
  done.push(moved);
  if (refine) done.push(on(moved, {op: 'refine', threads: 1}));
}

// What can be seen of a drawing's quality: the box around it, the lines' bends and length, how often a line (a route or a
// tap of a bar) runs through a box that is neither its own node nor one around it, and the routing issues reported.
function quality(snapshot) {
  const parent = new Map(snapshot.graph.nodes.map(n => [n.id, n.parentId]));
  const absolute = new Map();
  const at = id => {
    if (!absolute.has(id)) {
      const box = snapshot.boxes[id];
      const p = parent.get(id) ? at(parent.get(id)) : {x: 0, y: 0};
      absolute.set(id, {x: box.x + p.x, y: box.y + p.y, width: box.width, height: box.height});
    }
    return absolute.get(id);
  };
  const boxes = [...parent.keys()].filter(id => snapshot.boxes[id]).map(id => [id, at(id)]);
  // A route without an edge in the view (seen after a move of a view with both bars and lines) is counted apart.
  const edges = new Map(snapshot.graph.edges.map(e => [e.id, e]));
  const lines = snapshot.routes.filter(r => edges.has(r.id)).map(r => [r.points, [edges.get(r.id).source, edges.get(r.id).target]]);
  const strayRoutes = snapshot.routes.length - lines.length;
  for (const bus of snapshot.graph.buses) {
    for (const tap of bus.taps ?? []) {
      const points = snapshot.busLines?.[bus.id]?.taps[tap.edge];
      if (points) lines.push([points, [tap.node]]);
    }
  }
  let bends = 0, length = 0, hits = 0;
  for (const [points, ends] of lines) {
    const own = new Set();
    for (let id of ends) for (; id; id = parent.get(id)) own.add(id);
    bends += Math.max(0, points.length - 2);
    for (let i = 1; i < points.length; i++) {
      const [a, b] = [points[i - 1], points[i]];
      length += Math.abs(a.x - b.x) + Math.abs(a.y - b.y);
      for (const [id, r] of boxes) {
        if (!own.has(id) && Math.max(a.x, b.x) > r.x + 0.5 && Math.min(a.x, b.x) < r.x + r.width - 0.5 &&
            Math.max(a.y, b.y) > r.y + 0.5 && Math.min(a.y, b.y) < r.y + r.height - 0.5) hits++;
      }
    }
  }
  const {width, height} = describe(snapshot);
  return {width, height, bends, length: Math.round(length), hits, issues: snapshot.routingIssues?.length ?? 0, strayRoutes};
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
      const variants = skip ? ['open', 'open+skip', 'collapsed', 'collapsed+skip'] : ['open', 'collapsed'];
      for (const variant of variants) runs[variant] = [];
      for (let r = 0; r < reps; r++) for (const variant of variants) runs[variant].push(run(request(containers, strategy, busStyle, variant)));
      for (const variant of variants) {
        const sorted = runs[variant].sort((a, b) => a.timings.total - b.timings.total);
        const median = sorted[Math.floor((sorted.length - 1) / 2)];
        const compared = {};
        if (variant.endsWith('+skip')) {
          const plain = runs[variant.replace('+skip', '')][0].snapshot;
          compared.differs = differing(plain, median.snapshot);
          if (args.ops) {
            const refine = args.ops.includes('refine');
            const [a, b] = [operations(plain, refine), operations(median.snapshot, refine)];
            for (const [i, name] of (refine ? ['move', 'refine'] : ['move']).entries()) {
              if (a[i]?.failed || b[i]?.failed) compared[`${name}Failed`] = [a[i]?.failed ?? null, b[i]?.failed ?? null];
              else if (a[i] && b[i]) {
                const differs = differing(a[i], b[i]);
                compared[`${name}Differs`] = differs;
                if (differs.some(k => ['boxes', 'routes', 'busLines'].includes(k))) compared[`${name}Quality`] = [quality(a[i]), quality(b[i])];
              }
            }
          }
        }
        const row = {
          label: args.label, containers, units, buses: median.snapshot.graph.buses.length, gateway, strategy, busStyle, variant, reps,
          ...describe(median.snapshot),
          totalsMs: sorted.map(a => a.timings.total),
          timingsMs: median.timings,
          ...compared,
          ...(median.phases ? {phases: median.phases} : {}),
          load: os.loadavg().map(l => Math.round(l * 100) / 100),
        };
        if (args.out) appendFileSync(args.out, `${JSON.stringify(row)}\n`);
        const t = median.timings;
        console.log(
          `${String(containers).padStart(4)} ${view.padEnd(14)} ${variant.padEnd(14)} nodes ${String(row.nodes).padStart(5)} taps ${String(row.taps).padStart(5)} ` +
          `${row.width}x${row.height} total ${t.total} layout ${t.layout} place ${t.placement} stress ${t.organicStress} sep ${t.separation} ` +
          `repair ${t.repair} ports ${t.portResolution} route ${t.routing} buses ${t.buses} load ${row.load[0]}` +
          (compared.differs ? ` differs ${compared.differs.join('+') || '-'}` : '') +
          (compared.moveDiffers ? ` move ${compared.moveDiffers.join('+') || '-'}` : '') +
          (compared.refineDiffers ? ` refine ${compared.refineDiffers.join('+') || '-'}` : '') +
          (compared.moveFailed || compared.refineFailed ? ` FAILED ${JSON.stringify(compared.moveFailed ?? compared.refineFailed)}` : '') +
          (median.phases ? `\n       ${median.phases.join('; ')}` : ''),
        );
      }
    }
  }
}
