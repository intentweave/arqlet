// The same operations on two builds of `layout-command`, answer for answer (#150): the bus views of `bus-collapse.mjs`
// imported, then moved, pinned, configured, patched, collapsed and expanded, revealed, restored, refined and read, each
// step from the state the step before left in that build. Run stateless (the snapshot sent with every request) and as a
// session of `--sessions`.
//
// Answers are compared whole, except for what may differ: the routes, ports and border crossings kept for the taps
// (`busTaps`, its roots compared) and `recomputedContainers`. Every step whose raw answers differ there is counted.
//
// Usage: node benchmarks/experiments/bus-collapse-compare.mjs --old PATH --new PATH [options]
//   --cases LIST      SIZE:UNITS:BUSES:GATEWAY, comma separated
//   --views LIST      strategy/busStyle pairs (default organic/lines,organic/trunk,organic/bar,layered/lines,layered/trunk,layered/bar)
//   --variants LIST   open,collapsed (default both)
//   --modes LIST      stateless,session (default both)
//   --refine N        refine in cases of at most N containers (default 24)
//   --extra JSON      options added to every import, and `graph`: keys merged into the graph
//   --jobs N          cases at a time (default 2)
//   --out FILE        one JSON line per case and mode
import {spawn, spawnSync} from 'node:child_process';
import {appendFileSync} from 'node:fs';
import {dirname, join} from 'node:path';
import {fileURLToPath} from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const args = Object.fromEntries(
  process.argv.slice(2).reduce((pairs, a, i, all) => (a.startsWith('--') ? [...pairs, [a.slice(2), all[i + 1]]] : pairs), []),
);
const binaries = [args.old, args.new];
const cases = (args.cases ?? '8:3:3:2,16:3:3:2').split(',');
const views = (args.views ?? 'organic/lines,organic/trunk,organic/bar,layered/lines,layered/trunk,layered/bar').split(',');
const variants = (args.variants ?? 'open,collapsed').split(',');
const modes = (args.modes ?? 'stateless,session').split(',');
const refineUpTo = Number(args.refine ?? 24);
const extra = args.extra ? JSON.parse(args.extra) : {};
const LIMIT = 600000;

function importRequest(spec, view, variant) {
  const [size, units, buses, gateway] = spec.split(':');
  const made = spawnSync('node', [join(here, 'bus-collapse.mjs'), '--request', `${size}/${view}/${variant}`, '--units', units, '--buses', buses, '--gateway', gateway], {encoding: 'utf8', maxBuffer: 1 << 29, timeout: 60000});
  const request = JSON.parse(made.stdout);
  delete request.timings;
  const {graph, ...options} = extra;
  Object.assign(request.options, options);
  Object.assign(request.graph, graph ?? {});
  return request;
}

// One process that answers messages line by line (`--sessions`), or one process per request.
function host(binary, mode) {
  if (mode === 'stateless') {
    const once = body => new Promise((resolve, reject) => {
      const child = spawn(binary, []);
      const out = [];
      let err = '';
      const timer = setTimeout(() => child.kill('SIGKILL'), LIMIT);
      child.stdout.on('data', d => out.push(d));
      child.stderr.on('data', d => (err += d));
      child.on('error', reject);
      child.on('close', (status, signal) => {
        clearTimeout(timer);
        if (signal === 'SIGKILL') return resolve({timedOut: true});
        // The file and the message of a panic: line numbers differ between two builds.
        const panic = err.match(/panicked at (\S+?):\d+:\d+:?\n(.*)/);
        if (status !== 0) return resolve({ok: false, crashed: panic ? `panic in ${panic[1]}: ${panic[2]}` : `status ${status} ${signal}`});
        resolve(JSON.parse(Buffer.concat(out).toString()));
      });
      child.stdin.end(JSON.stringify(body));
    });
    // Once more after the time limit: a freshly started process has been seen to hang before it runs.
    return {send: async body => { const a = await once(body); return a.timedOut ? once(body) : a; }, close() {}};
  }
  let child, buffer, waiting, dead, id = null;
  function start() {
    child = spawn(binary, ['--sessions']);
    [buffer, waiting, dead] = [[], null, false];
    const mine = child;
    child.stdout.on('data', data => {
      let from = 0;
      for (let at = data.indexOf(10); at >= 0; at = data.indexOf(10, from)) {
        buffer.push(data.subarray(from, at));
        const line = Buffer.concat(buffer).toString();
        buffer = [];
        from = at + 1;
        waiting?.(JSON.parse(line));
      }
      if (from < data.length) buffer.push(data.subarray(from));
    });
    child.on('close', () => {
      if (mine !== child) return;
      dead = true;
      waiting?.({ok: false, crashed: 'the session process ended'});
    });
  }
  const line = (message, limit = LIMIT) => new Promise(resolve => {
    if (dead) return resolve({ok: false, crashed: 'the session process ended'});
    const mine = child;
    const timer = setTimeout(() => mine.kill('SIGKILL'), limit);
    waiting = answer => { clearTimeout(timer); waiting = null; resolve(answer); };
    mine.stdin.write(`${JSON.stringify(message)}\n`);
  });
  start();
  return {
    async send(request) {
      // Opening answers at once; where it does not, the process hung before it ran and is started again.
      for (let tries = 0; id === null && tries < 3; tries++) {
        const opened = await line({session: 'open'}, 60000);
        if (opened.ok) id = opened.session;
        else start();
      }
      return line({session: id, request});
    },
    close() { child.stdin.end(); },
  };
}

// An answer without what may differ.
function drawn(value) {
  if (Array.isArray(value)) return value.map(drawn);
  if (value === null || typeof value !== 'object') return value;
  return Object.fromEntries(
    Object.entries(value)
      .filter(([k]) => k !== 'recomputedContainers' && k !== 'timings')
      .map(([k, v]) => [k, k === 'busTaps' ? {roots: v.roots} : drawn(v)]),
  );
}

// The fields of two answers and of their snapshots that differ.
function differingFields(a, b) {
  const keys = (x, y, prefix) => [...new Set([...Object.keys(x ?? {}), ...Object.keys(y ?? {})])]
    .filter(k => JSON.stringify(x?.[k]) !== JSON.stringify(y?.[k]))
    .map(k => prefix + k);
  return [...keys(a, b, '').filter(k => k !== 'snapshot'), ...keys(a.snapshot, b.snapshot, 'snapshot.')];
}

// The first place two values differ.
function firstDifference(a, b, path = '') {
  if (JSON.stringify(a) === JSON.stringify(b)) return null;
  if (a && b && typeof a === 'object' && typeof b === 'object') {
    for (const k of new Set([...Object.keys(a), ...Object.keys(b)])) {
      const found = firstDifference(a[k], b[k], `${path}.${k}`);
      if (found) return found;
    }
  }
  return `${path}: ${JSON.stringify(a)?.slice(0, 80)} != ${JSON.stringify(b)?.slice(0, 80)}`;
}

// Which of the fields that may differ do, anywhere in two answers.
function hidden(a, b, found) {
  if (!a || !b || typeof a !== 'object' || typeof b !== 'object') return found;
  for (const k of new Set([...Object.keys(a), ...Object.keys(b)])) {
    if (k === 'recomputedContainers' || k === 'busTaps') {
      if (JSON.stringify(a[k]) !== JSON.stringify(b[k])) found.add(k);
    } else if (k !== 'timings') hidden(a[k], b[k], found);
  }
  return found;
}

async function compare(spec, view, variant, mode) {
  const hosts = binaries.map(b => host(b, mode));
  const state = [null, null];
  const result = {case: spec, view, variant, mode, steps: 0, hidden: {}, errors: [], differences: []};
  // One operation on both builds. `request(snapshot)` is built from each build's own state.
  async function step(name, request, {edits = true} = {}) {
    const answers = await Promise.all(hosts.map((h, i) => {
      const body = request(state[i]);
      if (mode === 'stateless' && state[i] && body.op !== 'import') {
        body.snapshot = state[i];
        body.expectedRevision ??= state[i].revision;
        body.expectedLayoutVersion ??= state[i].layoutVersion;
      }
      return h.send(body);
    }));
    // A session answers with a summary: the snapshot is asked for after every step that changes it.
    if (mode === 'session' && edits && answers.every(a => a.ok)) {
      const exported = await Promise.all(hosts.map(h => h.send({op: 'export'})));
      answers.forEach((a, i) => (a.snapshot = exported[i].snapshot));
    }
    result.steps++;
    const [old, next] = answers.map(drawn);
    const difference = firstDifference(old, next);
    if (difference) result.differences.push({step: name, fields: differingFields(old, next), difference});
    for (const key of hidden(answers[0], answers[1], new Set())) result.hidden[key] = (result.hidden[key] ?? 0) + 1;
    if (answers.some(a => !a.ok)) result.errors.push({step: name, answers: answers.map(a => a.ok ? 'ok' : a.crashed ?? a.error ?? 'timed out')});
    if (edits) answers.forEach((a, i) => { if (a.ok && a.snapshot) state[i] = a.snapshot; });
    return answers[0];
  }
  const read = (name, request) => step(name, request, {edits: false});
  const view0 = () => state[0];
  const at = (s, node, dx, dy) => ({x: s.boxes[node].x + dx, y: s.boxes[node].y + dy});
  const move = async (name, node, release, dx) => {
    const target = at(view0(), node, dx, 40);
    for (const [seq, phase] of [[1, 'begin'], [2, 'update'], [3, 'finish']]) {
      await step(`${name} ${phase}`, () => ({op: 'move', gestureId: name, seq, phase, node, position: target, release}));
    }
  };
  try {
    await step('import', () => importRequest(spec, view, variant));
    if (!view0()) return result;
    const shown = () => view0().graph.nodes;
    const tops = () => shown().filter(n => !n.parentId).map(n => n.id);
    const unit = () => shown().find(n => n.parentId)?.id ?? tops().at(-1);
    const container = kind => view0().graph.nodes.find(n => n.kind === kind)?.id;
    await read('scene', () => ({op: 'scene'}));
    const imported = await read('intent', () => ({op: 'intent'}));
    await move('move', tops()[0], false, 400);
    await move('move released', unit(), true, -150);
    await step('pin', s => ({op: 'pin', nodeId: tops()[1] ?? tops()[0]}));
    await step('unpin', () => ({op: 'pin', nodeId: tops()[1] ?? tops()[0], unpin: true}));
    await step('configure appearance', () => ({op: 'configure', options: {appearance: {showPorts: false}}}));
    await step('configure organicGap', () => ({op: 'configure', options: {organicGap: 40}}));
    const style = view.split('/')[1];
    await step('configure other style', () => ({op: 'configure', options: {busStyle: style === 'bar' ? 'lines' : 'bar'}}));
    await step('configure style back', () => ({op: 'configure', options: {busStyle: style}}));
    const gateway = container('gateway');
    if (gateway) {
      await step('collapse gateway', () => ({op: 'toggle', nodeId: gateway}));
      await step('expand gateway', () => ({op: 'toggle', nodeId: gateway}));
    }
    const single = container('single');
    if (single) {
      await step('toggle single', () => ({op: 'toggle', nodeId: single}));
      await move('move after toggle', tops()[0], false, 250);
      await step('toggle single back', () => ({op: 'toggle', nodeId: single}));
    }
    await step('reveal', () => ({op: 'reveal', nodeIds: ['c1u0']}));
    await step('measure', () => ({op: 'measure', sizes: {[unit()]: {width: 220, height: 70}}}));
    await step('shape', () => ({op: 'shape', nodeId: unit(), shape: 'circle'}));
    await step('direction lr', () => ({op: 'direction', direction: 'lr'}));
    await step('direction auto', () => ({op: 'direction', direction: 'auto'}));
    await step('settle', () => ({op: 'settle'}));
    await step('filter', () => ({op: 'filter', hide: {nodes: ['c2u1']}}));
    await step('filter off', () => ({op: 'filter', hide: {}}));
    await step('patch add node', () => ({op: 'patch', patch: {nodes: [{id: 'new', kind: 'unit', label: 'New', parentId: 'c1'}]}}));
    // Removing units leaves buses with fewer taps (a bus with one tap is drawn as a bar).
    await step('patch remove units', () => ({op: 'patch', patch: {removeNodes: ['c0u0', 'c3u0', 'c3u1']}}));
    await move('move after patch', tops()[0], false, 300);
    await step('restore', () => ({op: 'restore', intent: imported.intent}));
    const bookmark = await read('compactBookmark', () => ({op: 'compactBookmark'}));
    if (bookmark.ok) await read('readBookmark', () => ({op: 'readBookmark', text: bookmark.text}));
    await read('trace', () => ({op: 'trace', from: 'c1u0', to: 'c2u0'}));
    await read('neighbourhood', () => ({op: 'neighbourhood', nodeIds: ['c1u0'], hops: 2}));
    if (Number(spec.split(':')[0]) <= refineUpTo) {
      await step('refine', () => ({op: 'refine', threads: 1}));
      await move('move after refine', tops()[0], false, 200);
      if (single) await step('toggle after refine', () => ({op: 'toggle', nodeId: single}));
      await step('refine again', () => ({op: 'refine', threads: 4}));
    }
    await step('add', s => ({op: 'add', sourceId: unit(), label: 'Added'}));
    await read('scene at the end', () => ({op: 'scene'}));
  } finally {
    hosts.forEach(h => h.close());
  }
  return result;
}

const work = cases.flatMap(spec => views.flatMap(view => variants.flatMap(variant => modes.map(mode => [spec, view, variant, mode]))));
const totals = {runs: 0, steps: 0, differing: 0, errors: 0};
async function worker() {
  for (let next = work.shift(); next; next = work.shift()) {
    const result = await compare(...next);
    totals.runs++;
    totals.steps += result.steps;
    totals.differing += result.differences.length;
    totals.errors += result.errors.length;
    if (args.out) appendFileSync(args.out, `${JSON.stringify({...result, ...(args.extra ? {extra} : {})})}\n`);
    console.log(
      `${next.join(' ')}: ${result.steps} steps, ${result.differences.length} differ, ${result.errors.length} refused` +
      ` ${JSON.stringify(result.hidden)}` + (result.differences.length ? ` ${JSON.stringify(result.differences[0])}` : ''),
    );
  }
}
await Promise.all(Array.from({length: Number(args.jobs ?? 2)}, worker));
console.log(JSON.stringify(totals));
