// ADR 0019, step 4: the scene of a resident session as JSON objects (`scene`) against columns (`columns`), on the browser
// path: the engine call in WASM (Node), then the answer to the page through a worker (Node worker_threads). The JSON
// scene is parsed in the page's thread and cloned by `postMessage`; the columns are copied once out of the module's
// memory (what the worker does) and transferred. Also the zero-copy case (a host that calls the engine directly and reads
// the views, no worker). Medians of `reps` runs after one warm-up; sizes in bytes of the JSON text and of the columns.
//
//   node benchmarks/protocol/measure-columns.mjs <arqlet_wasm.wasm> <graph.json> [reps] [view]
//
// `view`: `lod` (import collapsed from depth 1, then open the first collapsed container; the view of the earlier
// protocol measurements) or `full` (import with every node of the limit, no abstraction).
import {readFileSync} from 'node:fs';
import {Worker} from 'node:worker_threads';
import {performance} from 'node:perf_hooks';
import {createEngine} from '../../packages/engine-client/wasm-client.js';

const [wasmPath, graphPath, repsArg, view = 'lod'] = process.argv.slice(2);
const reps = Number(repsArg ?? 5);
const graph = JSON.parse(readFileSync(graphPath, 'utf8'));
const nodes = graph.nodes.length;
const run = await createEngine(readFileSync(wasmPath));
const median = v => [...v].sort((a, b) => a - b)[Math.floor(v.length / 2)];
const echo = new Worker('const {parentPort}=require("node:worker_threads");parentPort.on("message",m=>parentPort.postMessage(m,m.transfer||[]));', {eval: true});
// A message to the worker and back; `transfer` lists the buffers that move instead of being cloned.
const roundTrip = (value, transfer = []) => new Promise(resolve => { echo.once('message', resolve); echo.postMessage(value, transfer); });
const toPage = async (value, transfer = []) => { await roundTrip(value, transfer); };

const limit = Math.min(nodes, 10000);
const session = run.openSession();
const imported = session.run({op: 'import', graph, limit, scene: false, ...(view === 'lod' ? {options: {levelOfDetail: {depth: 1}}} : {})});
if (!imported.ok) throw Error(JSON.stringify(imported).slice(0, 200));
if (view === 'lod') {
  const exported = session.run({op: 'export'}).snapshot;
  const visible = new Set(exported.graph.nodes.map(n => n.id));
  const container = exported.collapsedIds.find(c => visible.has(c));
  const opened = session.run({op: 'toggle', nodeId: container, scene: false});
  if (!opened.ok) throw Error(JSON.stringify(opened).slice(0, 200));
}

const rows = [];
async function measure(name, once, extra = {}) {
  await once();
  const times = [];
  for (let i = 0; i < reps; i++) times.push(await once());
  const row = {engine: 'wasm-node', view, request: name, nodes, reps, ms: median(times), ...extra};
  rows.push(row);
  console.log(JSON.stringify(row));
}

const sceneAnswer = session.run({op: 'scene'});
const sceneText = JSON.stringify(sceneAnswer.scene);
const shape = {boxes: sceneAnswer.scene.nodes.length, routes: sceneAnswer.scene.routes.length,
  routePoints: sceneAnswer.scene.routes.reduce((n, r) => n + r.points.length, 0)};
const columns = session.columns();
const columnBytes = Object.values(columns).filter(ArrayBuffer.isView).reduce((n, a) => n + a.byteLength, 0);
const idBytes = JSON.stringify({nodeIds: columns.nodeIds, routeIds: columns.routeIds}).length;
console.log(JSON.stringify({view, nodes, ...shape, sceneJsonBytes: sceneText.length, columnBytes, idTableJsonBytes: idBytes}));

const asObjects = async () => {
  const t = performance.now();
  const answer = session.run({op: 'scene'});
  await toPage(answer);
  return performance.now() - t;
};
const asColumnsThroughWorker = async () => {
  const t = performance.now();
  const c = session.columns();
  const moved = {};
  const transfer = [];
  for (const [k, v] of Object.entries(c)) { moved[k] = ArrayBuffer.isView(v) ? v.slice() : v; if (ArrayBuffer.isView(v)) transfer.push(moved[k].buffer); }
  await toPage({...moved, transfer}, transfer);
  return performance.now() - t;
};
const asColumnsDirect = async () => {
  const t = performance.now();
  const c = session.columns();
  // Touch the first and last element of each column, as a renderer reading them would start to.
  let sink = 0;
  for (const v of Object.values(c)) if (ArrayBuffer.isView(v) && v.length) sink += v[0] + v[v.length - 1];
  if (Number.isNaN(sink)) throw Error('unreachable');
  return performance.now() - t;
};
// The engine call alone, with the parse of its answer: nothing is moved to a page.
const sceneEngineOnly = async () => { const t = performance.now(); session.run({op: 'scene'}); return performance.now() - t; };

await measure('scene (JSON objects), to the page', asObjects, {...shape, bytes: sceneText.length});
await measure('columns, copied and transferred', asColumnsThroughWorker, {...shape, bytes: columnBytes + idBytes});
await measure('columns, views (no worker)', asColumnsDirect, {...shape, bytes: 0});
await measure('scene (JSON), engine call and parse, nothing moved', sceneEngineOnly, shape);
session.close();

// The overview (`view` is irrelevant to it; measured once per size, with `lod`): the whole answer for a new graph and the
// continuation on the same clusters with `positionsOnly`. Both modes compute the same layout; what differs is the answer:
// JSON arrays (parsed, then structured-cloned to the page) or typed arrays (copied once, transferred). The new graph is
// sent in both (counted in both). Fewer repetitions: the layout itself takes seconds at 100 000 nodes.
if (view === 'lod') {
  const overviewReps = Math.min(reps, 3);
  const sizeOf = answer => {
    const {overview} = answer;
    const typed = Object.values(overview).filter(ArrayBuffer.isView).reduce((n, a) => n + a.byteLength, 0);
    const rest = Object.fromEntries(Object.entries(overview).filter(([, v]) => !ArrayBuffer.isView(v)));
    return typed + JSON.stringify(rest).length;
  };
  const moveTyped = async answer => {
    const transfer = Object.values(answer.overview).filter(ArrayBuffer.isView).map(a => a.buffer);
    await toPage({...answer, transfer}, transfer);
  };
  async function overviewRow(name, once, extra) {
    await once();
    const times = [];
    for (let i = 0; i < overviewReps; i++) times.push(await once());
    const row = {engine: 'wasm-node', view: 'overview', request: name, nodes, reps: overviewReps, ms: median(times), ...extra};
    console.log(JSON.stringify(row));
  }
  for (const [kind, columnar] of [['JSON arrays', false], ['typed arrays', true]]) {
    const s = run.openSession();
    const ask = request => (columnar ? s.overview(request, {copy: true}) : s.run(request));
    const t0 = performance.now();
    const whole = ask({op: 'overview', graph});
    if (!whole.ok) throw Error(JSON.stringify(whole).slice(0, 200));
    console.log(JSON.stringify({view: 'overview', kind, nodes, firstOpenMs: performance.now() - t0, bytes: columnar ? sizeOf(whole) : JSON.stringify(whole).length}));
    await overviewRow(`overview whole answer, ${kind}`, async () => {
      const t = performance.now();
      const answer = ask({op: 'overview', graph});
      await (columnar ? moveTyped(answer) : toPage(answer));
      return performance.now() - t;
    }, {bytes: columnar ? sizeOf(whole) : JSON.stringify(whole).length});
    const continuation = {op: 'overview', positionsOnly: true, start: {}};
    const probe = ask(continuation);
    await overviewRow(`overview continuation (positionsOnly), ${kind}`, async () => {
      const t = performance.now();
      const answer = ask(continuation);
      await (columnar ? moveTyped(answer) : toPage(answer));
      return performance.now() - t;
    }, {bytes: columnar ? sizeOf(probe) : JSON.stringify(probe).length});
    s.close();
  }
}
await echo.terminate();
