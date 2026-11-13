// ADR 0019, step 3: the browser path with the snapshot sent in every request against a session that keeps it in the
// engine (`lw_session_*`). Per request (a toggle that opens a container, the scene, a trace), the medians of `reps` runs
// after one warm-up of the whole path: the request and the answer through a worker (structured clone, Node
// worker_threads) and the engine call (stringify, copy, run, parse). The answers of the resident session include the
// scene, so a host draws without asking again; with the snapshot sent, a host asks for the scene in a second request
// (counted as `toggle + scene`).
//
//   node benchmarks/protocol/measure-resident.mjs <arqlet_wasm.wasm> <graph.json> [reps]
import {readFileSync} from 'node:fs';
import {Worker} from 'node:worker_threads';
import {performance} from 'node:perf_hooks';
import {createEngine} from '../../packages/engine-client/wasm-client.js';

const [wasmPath, graphPath, repsArg] = process.argv.slice(2);
const reps = Number(repsArg ?? 5);
const graph = JSON.parse(readFileSync(graphPath, 'utf8'));
const nodes = graph.nodes.length;
const run = await createEngine(readFileSync(wasmPath));
const median = v => [...v].sort((a, b) => a - b)[Math.floor(v.length / 2)];
const echo = new Worker('const {parentPort}=require("node:worker_threads");parentPort.on("message",m=>parentPort.postMessage(m));', {eval: true});
const roundTrip = value => new Promise(resolve => { echo.once('message', () => resolve()); echo.postMessage(value); });

// One request through the worker path: the request to the worker, the engine call, the answer back.
async function through(call, request) {
  const t = performance.now();
  await roundTrip(request);
  const answer = call(request);
  await roundTrip(answer);
  return {ms: performance.now() - t, answer};
}

const limit = Math.min(nodes, 10000);
const imported = run({op: 'import', graph, limit, options: {levelOfDetail: {depth: 1}}});
const snapshot = imported.snapshot;
const visible = new Set(snapshot.graph.nodes.map(n => n.id));
const container = snapshot.collapsedIds.find(c => visible.has(c));
const versions = {expectedRevision: snapshot.revision, expectedLayoutVersion: snapshot.layoutVersion};
const traceEnds = (() => {
  const ids = snapshot.sourceGraph.nodes.map(n => n.id);
  const edge = snapshot.sourceGraph.edges.find(e => e.target);
  return [edge.source, edge.target ?? ids[1]];
})();

const rows = [];
async function measure(name, mode, once) {
  await once();
  const times = [];
  for (let i = 0; i < reps; i++) times.push(await once());
  const row = {engine: 'wasm-node', path: mode, request: name, nodes, reps, ms: median(times)};
  rows.push(row);
  console.log(JSON.stringify(row));
}

// Snapshot sent: the same request on the same snapshot every time.
await measure('toggle + scene', 'snapshot sent', async () => {
  const a = await through(run, {op: 'toggle', snapshot, nodeId: container, ...versions});
  const b = await through(run, {op: 'scene', snapshot: a.answer.snapshot});
  return a.ms + b.ms;
});
await measure('scene', 'snapshot sent', async () => (await through(run, {op: 'scene', snapshot})).ms);
await measure('trace', 'snapshot sent', async () => (await through(run, {op: 'trace', snapshot, from: traceEnds[0], to: traceEnds[1]})).ms);

// Resident: the session holds the imported snapshot; a toggle opens the container (measured) and a second one closes it
// again (not measured), so that every measured toggle starts from the same view.
const session = run.openSession();
session.run({op: 'load', snapshot, scene: false});
await measure('toggle + scene', 'resident', async () => {
  const opened = await through(session.run, {op: 'toggle', nodeId: container});
  if (!opened.answer.ok || !opened.answer.scene) throw Error(JSON.stringify(opened.answer).slice(0, 200));
  session.run({op: 'toggle', nodeId: container, scene: false});
  return opened.ms;
});
await measure('scene', 'resident', async () => (await through(session.run, {op: 'scene'})).ms);
await measure('trace', 'resident', async () => (await through(session.run, {op: 'trace', from: traceEnds[0], to: traceEnds[1]})).ms);
session.close();
await echo.terminate();
