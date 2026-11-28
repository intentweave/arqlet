// The linear memory a WASM engine grows to for single requests (ADR 0024, C3), next to the engine's own estimate for
// wasm32. Every request runs in a fresh instance of the module, so its growth is that request's alone: WebAssembly never
// gives memory back, and a worker's linear memory is the high-water mark of what it ran. Two ways, as hosts use them:
// `dispatch` (`lw_run`, the request text with its snapshot) and a resident session (`lw_session_run`: an import; an edit
// on a session that `load`ed the import's snapshot first, so the growth is that of the load and the edit together).
//
//   node benchmarks/memory-estimate/wasm-peak.mjs MODULE.wasm GRAPH.json LIMIT OPTIONS_JSON [OPS]
//
// One JSON line per request on stdout: `linear` (bytes the memory grew by), `heapPeak` (the counted heap peak of the
// request above the live heap before it, from `heapPeakBytes`), `heap` (the live heap after it, in a session), the
// estimate and its time.
import {readFileSync} from 'node:fs';
import {basename} from 'node:path';

const [modulePath, graphPath, limitText, optionsText = '{}', opsText = 'import,toggle,direction,patch,refine,overview'] =
  process.argv.slice(2);
const wasm = new WebAssembly.Module(readFileSync(modulePath));
const encoder = new TextEncoder();
const decoder = new TextDecoder();

function instance() {
  let memory;
  const inst = new WebAssembly.Instance(wasm, {
    env: {lw_publish() {}, lw_now: () => performance.now()},
  });
  memory = inst.exports.memory;
  const e = inst.exports;
  const call = (fn, text) => {
    const bytes = encoder.encode(text);
    const ptr = e.lw_alloc(bytes.length);
    new Uint8Array(memory.buffer, ptr, bytes.length).set(bytes);
    const packed = fn(ptr, bytes.length);
    const out = Number(BigInt.asUintN(32, packed));
    const len = Number(BigInt.asUintN(64, packed) >> 32n);
    const answer = decoder.decode(new Uint8Array(memory.buffer, out, len).slice());
    e.lw_free(out, len);
    e.lw_free(ptr, bytes.length);
    return answer;
  };
  return {
    size: () => memory.buffer.byteLength,
    run: (text) => call(e.lw_run, text),
    session: () => {
      const id = e.lw_session_open();
      return (text) => call((p, l) => e.lw_session_run(id, p, l), text);
    },
  };
}

const graphText = readFileSync(graphPath, 'utf8').trim();
const graph = JSON.parse(graphText);
const limit = Number(limitText);
const options = JSON.parse(optionsText);
const ops = opsText.split(',');
const common = {graph: basename(graphPath, '.json'), nodes: graph.nodes.length, edges: graph.edges.length, limit,
  options, pointer: 32};
const emit = (row) => console.log(JSON.stringify({...row, ...common}));

/// The estimate of a request in a fresh instance, with its time and that instance's growth.
function estimate(text, inSession = null) {
  const engine = instance();
  const base = engine.size();
  const run = inSession ? inSession(engine) : engine.run;
  const start = performance.now();
  const answer = JSON.parse(run(`{"op":"estimate","request":${text}}`));
  const ms = performance.now() - start;
  if (!answer.ok) return {error: answer.error};
  return {...answer.estimate, ms, linear: engine.size() - base};
}

// The counted heap peak of what runs between two `memory` answers (`heapPeakBytes`, ADR 0024 C3), read through a session
// of the same instance; the text copied in counts, as in the linear memory.
function measure(run, engine, text) {
  const probe = engine.probe ?? (engine.probe = engine.session());
  const before = JSON.parse(probe('{"op":"memory"}'));
  const base = engine.size();
  const start = performance.now();
  const answer = run(text);
  const ms = performance.now() - start;
  const linear = engine.size() - base;
  const after = JSON.parse(probe('{"op":"memory"}'));
  const heapPeak = after.heapPeakBytes !== undefined ? after.heapPeakBytes - before.heapBytes : undefined;
  return {answer, ms, linear, base, heapPeak};
}

function viewSize(s) {
  const parents = new Set(s.graph.nodes.map((n) => n.parentId).filter(Boolean));
  return {nodes: s.graph.nodes.length, edges: s.graph.edges.length, routes: (s.routes ?? []).length, open: parents.size,
    collapsed: (s.collapsedIds ?? []).length, source: s.sourceGraph?.nodes?.length ?? 0};
}

function toggleTarget(s) {
  const children = new Map();
  for (const n of (s.sourceGraph ?? s.graph).nodes) if (n.parentId) children.set(n.parentId, (children.get(n.parentId) ?? 0) + 1);
  const shown = new Set(s.graph.nodes.map((n) => n.id));
  const collapsed = (s.collapsedIds ?? []).filter((id) => !id.startsWith('rest:') && shown.has(id) && children.has(id))
    .map((id) => [children.get(id), id]).sort((a, b) => a[0] - b[0] || (a[1] < b[1] ? -1 : a[1] > b[1] ? 1 : 0));
  if (collapsed.length) return collapsed[Math.floor(collapsed.length / 2)][1];
  let best = null;
  for (const [id, c] of [...children].sort((a, b) => (a[0] < b[0] ? -1 : 1))) {
    if (shown.has(id) && (!best || c > best[1])) best = [id, c];
  }
  return best?.[0] ?? null;
}

function patchOf(s) {
  const parents = new Set(s.graph.nodes.map((n) => n.parentId).filter(Boolean));
  const leaf = s.graph.nodes.find((n) => !parents.has(n.id)) ?? s.graph.nodes[0];
  const nodes = [...Array(10).keys()].map((i) => ({id: `peak-new-${i}`, kind: 'symbol', label: `new ${i}`,
    parentId: leaf.parentId ?? null}));
  const edges = [...Array(10).keys()].map((i) => ({id: `peak-edge-${i}`, kind: 'calls', source: `peak-new-${i}`,
    target: leaf.id}));
  return {nodes, edges};
}

function opRequest(op, s) {
  switch (op) {
    case 'toggle': { const id = toggleTarget(s); return id && {op: 'toggle', nodeId: id}; }
    case 'direction': return {op: 'direction', direction: 'lr'};
    case 'patch': return {op: 'patch', patch: patchOf(s)};
    case 'refine': return {op: 'refine', threads: 1, workBudget: 4};
    default: return null;
  }
}

// The import through `dispatch`.
const importText = `{"op":"import","limit":${limit},"options":${JSON.stringify(options)},"graph":${graphText}}`;
let snapshot;
{
  const engine = instance();
  const m = measure(engine.run, engine, importText);
  const answer = JSON.parse(m.answer);
  if (!answer.ok) throw new Error(answer.error);
  snapshot = answer.snapshot;
  if (ops.includes('import')) {
    emit({mode: 'dispatch', op: 'import', textBytes: importText.length, answerBytes: m.answer.length, linear: m.linear, heapPeak: m.heapPeak,
      base: m.base, ms: m.ms, viewAfter: viewSize(snapshot), estimate: estimate(importText)});
  }
}
const snapshotText = JSON.stringify(snapshot);
for (const op of ops.filter((o) => !['import', 'overview'].includes(o))) {
  const request = opRequest(op, snapshot);
  if (!request) { emit({mode: 'dispatch', op, skipped: 'no target'}); continue; }
  const text = JSON.stringify({...request, expectedRevision: snapshot.revision,
    expectedLayoutVersion: snapshot.layoutVersion}).slice(0, -1) + `,"snapshot":${snapshotText}}`;
  const engine = instance();
  const m = measure(engine.run, engine, text);
  const answer = JSON.parse(m.answer);
  if (!answer.ok) { emit({mode: 'dispatch', op, error: answer.error}); continue; }
  emit({mode: 'dispatch', op, textBytes: text.length, answerBytes: m.answer.length, linear: m.linear, heapPeak: m.heapPeak, base: m.base,
    ms: m.ms, viewBefore: viewSize(snapshot), viewAfter: viewSize(answer.snapshot), estimate: estimate(text)});
}

// A resident session: the import in a fresh instance; each edit in a fresh instance that loaded the snapshot.
if (ops.includes('import')) {
  const engine = instance();
  const run = engine.session();
  const text = importText.replace('{"op":"import",', '{"op":"import","scene":false,');
  const m = measure(run, engine, text);
  const heap = JSON.parse(run('{"op":"memory"}'));
  emit({mode: 'session', op: 'import', linear: m.linear, heapPeak: m.heapPeak, base: m.base, ms: m.ms, heap: heap.heapBytes,
    kept: heap.total, viewAfter: viewSize(snapshot), estimate: estimate(text, (e) => e.session())});
}
const loadText = `{"op":"load","scene":false,"snapshot":${snapshotText}}`;
for (const op of ops.filter((o) => !['import', 'overview'].includes(o))) {
  const request = opRequest(op, snapshot);
  if (!request) continue;
  const text = JSON.stringify({...request, scene: false});
  const engine = instance();
  const run = engine.session();
  const loaded = measure(run, engine, loadText);
  const m = measure(run, engine, text);
  const answer = JSON.parse(m.answer);
  if (!answer.ok) { emit({mode: 'session', op, error: answer.error}); continue; }
  const heap = JSON.parse(run('{"op":"memory"}'));
  emit({mode: 'session', op, linear: m.linear, heapPeak: m.heapPeak, loadLinear: loaded.linear, base: loaded.base, ms: m.ms,
    heap: heap.heapBytes, kept: heap.total, viewBefore: viewSize(snapshot),
    estimate: estimate(text, (e) => { const r = e.session(); r(loadText); return r; })});
}

if (ops.includes('overview')) {
  const text = `{"op":"overview","graph":${graphText}}`;
  const engine = instance();
  const m = measure(engine.run, engine, text);
  emit({mode: 'dispatch', op: 'overview', ok: JSON.parse(m.answer).ok, textBytes: text.length,
    answerBytes: m.answer.length, linear: m.linear, heapPeak: m.heapPeak, base: m.base, ms: m.ms, estimate: estimate(text)});
}
