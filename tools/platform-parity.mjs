#!/usr/bin/env node
// Cross-platform determinism: every request below must give the same response from the native `layout-command` of this
// platform and from the WASM engine (which computes the same everywhere, so native = WASM on every platform means every
// platform computes the same). Prints one line per request and, with `--digest`, the SHA-256 of each response, so that
// runs on different platforms can be compared directly. Exits 1 on any difference.
//
//   cargo build --release -p arqlet-runtime --bin layout-command --features query --locked
//   cargo build -p arqlet-wasm --target wasm32-unknown-unknown --release --features query --locked
//   python3 benchmarks/generate.py --no-manifest
//   python3 benchmarks/overview/generate.py --nodes 10000 --seed 1 --out benchmarks/datasets/overview-10000.json
//   node tools/platform-parity.mjs [--digest]
//
// With `--node` (ADR 0030) the native Node module (`crates/node`, built with
//   cargo build --release -p arqlet-node --features query --locked
// ) answers every request too, and its answer text must be the same bytes as the WASM engine's answer text (not only the
// same JSON value); a sequence of session requests is compared the same way. The printed lines and digests do not change.
import {readFileSync, existsSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {join} from 'node:path';
import {createEngine} from '../packages/engine-client/wasm-client.js';
import {loadNativeEngine} from '../packages/node/index.js';

const ROOT = new URL('../', import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, '$1');
const exe = process.platform === 'win32' ? '.exe' : '';
const native = join(ROOT, `target/release/layout-command${exe}`);
const wasm = join(ROOT, 'target/wasm32-unknown-unknown/release/arqlet_wasm.wasm');
const withNode = process.argv.includes('--node');
const addon = join(ROOT, 'target/release', {darwin: 'libarqlet_node.dylib', win32: 'arqlet_node.dll'}[process.platform] ?? 'libarqlet_node.so');
for (const f of [native, wasm, ...(withNode ? [addon] : [])]) if (!existsSync(f)) { console.error(`Missing ${f}; build it first (see the header).`); process.exit(2); }
const run = await createEngine(readFileSync(wasm));
const node = withNode ? loadNativeEngine({path: addon}) : null;

// The WASM engine's answers as text, exactly as the module writes them (no JSON round trip), for the byte comparison with
// the native Node module: `dispatch`, and sessions by handle.
async function rawWasm(bytes) {
  const {instance} = await WebAssembly.instantiate(bytes, {env: {lw_now: () => 0, lw_publish: () => {}}});
  const api = instance.exports;
  const exchange = (text, call) => {
    const encoded = new TextEncoder().encode(text);
    const ptr = api.lw_alloc(encoded.length);
    new Uint8Array(api.memory.buffer, ptr, encoded.length).set(encoded);
    const result = call(ptr, encoded.length);
    const out = Number(result & 0xffffffffn), len = Number(result >> 32n);
    const answer = new TextDecoder().decode(new Uint8Array(api.memory.buffer, out, len));
    api.lw_free(ptr, encoded.length);
    api.lw_free(out, len);
    return answer;
  };
  return {
    run: text => exchange(text, (p, l) => api.lw_run(p, l)),
    open: () => api.lw_session_open(),
    session: (id, text) => exchange(text, (p, l) => api.lw_session_run(id, p, l)),
    close: id => api.lw_session_close(id),
  };
}
const wasmText = withNode ? await rawWasm(readFileSync(wasm)) : null;
const graph = f => JSON.parse(readFileSync(join(ROOT, f), 'utf8'));
const elk = {id: 'root', layoutOptions: {'elk.direction': 'RIGHT'},
  children: Array.from({length: 60}, (_, i) => ({id: `n${i}`, width: 40, height: 30})),
  edges: Array.from({length: 80}, (_, i) => ({id: `e${i}`, sources: [`n${i % 60}`], targets: [`n${(i * 7 + 3) % 60}`]}))};
const requests = [
  ['import small (layered)', {op: 'import', graph: graph('examples/fixtures/small.json'), limit: 100}],
  ['import cloud-architecture', {op: 'import', graph: graph('benchmarks/datasets/cloud-architecture.json'), limit: 100}],
  ['import organic-25', {op: 'import', graph: graph('benchmarks/datasets/organic-25.json'), limit: 200}],
  ['import organic-100', {op: 'import', graph: graph('benchmarks/datasets/organic-100.json'), limit: 200}],
  ['import tree-100', {op: 'import', graph: graph('benchmarks/datasets/tree-100.json'), limit: 200}],
  ['import mixed-layouts', {op: 'import', graph: graph('benchmarks/datasets/mixed-layouts.json'), limit: 200}],
  // The octilinear star with its refinement (octilinearRefine): the made-up city's metro (cycles between its lines) placed
  // as an octilinear area, and the vehicle's buses as metro lines.
  ['import metro-map octilinear refined', {op: 'import', graph: {...graph('benchmarks/datasets/metro-map.json'), defaultDirection: 'auto', defaultStrategy: 'octilinear', defaultRouting: 'straight'}, limit: 100, options: {octilinearRefine: true}}],
  ['import vehicle-metro refined', {op: 'import', graph: graph('benchmarks/datasets/vehicle-metro.json'), limit: 100, options: {octilinearRefine: true}}],
  ['elkLayout', {op: 'elkLayout', graph: elk}],
  ['elkLayout refined', {op: 'elkLayout', graph: elk, refine: true}],
  ['overview 10 000', {op: 'overview', graph: graph('benchmarks/datasets/overview-10000.json')}],
];
// A view whose repair grid is over its bound, with the sparse grid (#203).
const mesh = JSON.parse(execFileSync(process.execPath, [join(ROOT, 'benchmarks/experiments/bus-mesh.mjs'), '100', '0', 'organic', 'lines', '120', '500'], {maxBuffer: 1 << 30}).toString());
requests.push(['import over the grid bound, sparse repair', {...mesh, options: {...mesh.options, sparseRepairGrid: true}}]);
// A bus view whose first routing leaves conflicts, so the second routing pass and its overlap resolution run (#212, #229).
requests.push(['import with the second routing pass', JSON.parse(execFileSync(process.execPath, [join(ROOT, 'benchmarks/experiments/bus-mesh.mjs'), '60', '12', 'organic', 'lines', '60', '250'], {maxBuffer: 1 << 30}).toString())]);
// The operation `query` (ADR 0021, M5b) on a snapshot and its content, without a session: the rows must be the same
// natively and in WASM. (Without the cargo feature both engines answer the same refusal, which also compares equal.)
const imported = JSON.parse(execFileSync(native, {
  input: JSON.stringify({op: 'import', graph: graph('benchmarks/datasets/organic-100.json'), limit: 200}),
  maxBuffer: 1 << 30,
}).toString());
const content = {nodes: Object.fromEntries(graph('benchmarks/datasets/organic-100.json').nodes.slice(0, 40).map((n, i) => [n.id, {rank: i, tag: i % 3 === 0 ? 'x' : 'y'}]))};
for (const [name, query] of [
  ['query nodes by kind', 'MATCH (n) RETURN n.kind, count(*) ORDER BY n.kind'],
  ['query content filter', "MATCH (n) WHERE n.tag = 'x' RETURN n.id, n.rank ORDER BY n.rank SKIP 2 LIMIT 5"],
  ['query two hops', 'MATCH (a)-[r1]->(b)-[r2]->(c) RETURN a.id, c.id ORDER BY a.id, c.id LIMIT 20'],
  ['query paths with a view', 'MATCH (a)-[*1..2]->(b) RETURN DISTINCT b.id ORDER BY b.id LIMIT 10'],
  // Case mapping, whitespace and the text of floats come from the Rust library: the same natively and in WASM.
  ['query text functions', "UNWIND ['straße', 'İSTANBUL', 'ﬁn ΟΔΟΣ', '\\u00a0 é \\u3000'] AS s RETURN toUpper(s), toLower(s), trim(s), size(s), reverse(s), split(s, ' ')"],
  ['query float text', 'UNWIND [0.1 + 0.2, 1.0 / 3, -0.0, 1234567.125, 0.001] AS f RETURN toString(f)'],
  ['query union', 'MATCH (n) RETURN labels(n) AS v UNION MATCH ()-[r]->() RETURN type(r) AS v'],
  ['query maps and lists', 'MATCH (n) WITH n.kind AS k, collect(n.id) AS ids RETURN {kind: k, some: ids[..3], odd: [i IN ids WHERE size(i) % 2 = 1 | toUpper(i)][-2..]} AS m ORDER BY m'],
]) {
  requests.push([name, {op: 'query', query, snapshot: imported.snapshot, content, view: name.includes('view')}]);
}
// The compact bookmark (#151): the same text from a view, and the same reading of it, natively and in WASM.
const abstract = JSON.parse(execFileSync(native, {
  input: JSON.stringify({op: 'import', graph: graph('benchmarks/datasets/organic-100.json'), limit: 200, options: {communities: 8}}),
  maxBuffer: 1 << 30,
}).toString()).snapshot;
const marked = {op: 'compactBookmark', snapshot: abstract, selection: {nodeId: abstract.graph.nodes[0].id}};
const bookmarked = JSON.parse(execFileSync(native, {input: JSON.stringify(marked), maxBuffer: 1 << 30}).toString());
if (!bookmarked.ok) { console.error(`compactBookmark: ${bookmarked.error}`); process.exit(1); }
requests.push(['compactBookmark', marked], ['readBookmark', {op: 'readBookmark', snapshot: abstract, text: bookmarked.text}]);
// The overview as a server result (#209): the edges' data, and a changed graph continued from places by id (removed,
// added and rewired nodes, communities matched by their members), natively and in WASM.
const large = graph('benchmarks/datasets/overview-10000.json');
const before = JSON.parse(execFileSync(native, {input: JSON.stringify({op: 'overview', graph: large}), maxBuffer: 1 << 30}).toString());
if (!before.ok) { console.error(`overview: ${before.error}`); process.exit(1); }
const places = Object.fromEntries(before.overview.ids.map((id, i) => [id, {x: before.overview.x[i], y: before.overview.y[i], r: before.overview.r[i]}]));
const containers = new Set(large.nodes.map(n => n.parentId));
const gone = new Set(large.nodes.filter((n, i) => i % 97 === 5 && !containers.has(n.id)).map(n => n.id));
const kept = large.nodes.filter(n => !gone.has(n.id));
const added = kept.filter((_, i) => i % 89 === 7).map((n, i) => ({id: `added-${i}`, kind: n.kind, label: `added ${i}`, parentId: n.parentId}));
const changed = {
  nodes: [...added.slice(0, 20), ...kept, ...added.slice(20)],
  edges: [
    ...large.edges.filter(e => !gone.has(e.source) && !gone.has(e.target)).map((e, i) => (i % 13 === 0 ? {...e, weight: 1 + i % 5} : e)),
    ...added.map((n, i) => ({id: `added-edge-${i}`, kind: 'added', source: n.id, target: kept[(i * 41) % kept.length].id})),
  ],
};
requests.push(['overview with edge data', {op: 'overview', graph: changed, edgeData: true}]);
// The containers' inner-edge share and modularity (#236): sums of f64 in a fixed order, the same natively and in WASM.
requests.push(['overview with container stats', {op: 'overview', graph: changed, containerStats: true}]);
for (const settle of [false, true]) {
  requests.push([`overview continued by id${settle ? ', settling' : ''}`, {op: 'overview', graph: changed, edgeData: true, start: {nodes: places, settle}}]);
}
// With each entry's parent, from an answer whose first containers are set far smaller than what they hold.
const radii = Object.fromEntries(before.overview.ids.filter((id, i) => i % 211 === 0 && containers.has(id)).map(id => [id, 5]));
const small = JSON.parse(execFileSync(native, {input: JSON.stringify({op: 'overview', graph: large, radii,
  start: {x: before.overview.x, y: before.overview.y, r: before.overview.r}}), maxBuffer: 1 << 30}).toString()).overview;
const parented = Object.fromEntries(small.ids.map((id, i) => [id, {x: small.x[i], y: small.y[i], r: small.r[i], parent: small.ids[small.parent[i]]}]));
requests.push(['overview continued by id with parents', {op: 'overview', graph: changed, start: {nodes: parented}}]);
// A work budget that runs out (#211): in the clustering, in the arrangement (natively on 4 threads, which must not
// matter), and in a continuation; the work counts are part of the answer.
requests.push(
  ['overview over its budget in the clustering', {op: 'overview', graph: large, workBudget: 40000}],
  ['overview over its budget in the arrangement, 4 threads', {op: 'overview', graph: large, workBudget: 20000000, threads: 4}],
  ['overview continued by id over its budget', {op: 'overview', graph: changed, start: {nodes: places}, workBudget: 300000}],
  ['overview over its budget, refused', {op: 'overview', graph: large, workBudget: 20000000, overBudget: 'refuse'}],
);
// Filling packed containers later (#235): one by one, all at once within a budget that runs out (4 threads), the top
// level, and by id.
const cut = JSON.parse(execFileSync(native, {input: JSON.stringify({op: 'overview', graph: large, workBudget: 20000000}), maxBuffer: 1 << 30}).toString()).overview;
const cutStart = {x: cut.x, y: cut.y, r: cut.r};
const packedIds = cut.work.packed.filter(c => c >= 0).map(c => cut.ids[c]);
const cutPlaces = Object.fromEntries(cut.ids.map((id, i) => [id, {x: cut.x[i], y: cut.y[i], r: cut.r[i], parent: cut.ids[cut.parent[i]]}]));
requests.push(
  ['overview filling one container', {op: 'overview', graph: large, start: {...cutStart, fill: packedIds.slice(-1)}}],
  ['overview filling all within a budget, 4 threads', {op: 'overview', graph: large, start: {...cutStart, fill: packedIds}, workBudget: 3000000, threads: 4}],
  ['overview filling the top level', {op: 'overview', graph: large, start: {...cutStart, fillTop: true}}],
  ['overview filling by id', {op: 'overview', graph: large, start: {nodes: cutPlaces, fill: packedIds.slice(0, 5)}}],
);
// The long form of a view that changed its source graph (#195), read against the view as imported: its digest is found
// again by writing the bookmark's decisions into that graph, with the same answer natively and in WASM.
const ask = request => JSON.parse(execFileSync(native, {input: JSON.stringify(request), maxBuffer: 1 << 30}).toString());
const reshaped = ask({op: 'shape', snapshot: abstract, nodeId: abstract.sourceGraph.nodes[0].id, shape: 'circle',
  expectedRevision: abstract.revision, expectedLayoutVersion: abstract.layoutVersion}).snapshot;
const long = {version: 1, digest: ask({op: 'digest', snapshot: reshaped}).digest, intent: ask({op: 'intent', snapshot: reshaped}).intent};
const reread = {op: 'readBookmark', snapshot: abstract, bookmark: long};
if (ask(reread).sameSource !== true) { console.error('readBookmark: a changed view is not the same source'); process.exit(1); }
requests.push(['readBookmark, long form of a changed view', reread]);
const digest = process.argv.includes('--digest');
let differ = 0;
for (const [name, request] of requests) {
  const body = JSON.stringify(request);
  const a = JSON.stringify(JSON.parse(execFileSync(native, {input: body, maxBuffer: 1 << 30}).toString()));
  const b = JSON.stringify(run(request));
  let same = a === b;
  if (node) {
    // Byte for byte: the native Node module's text against the WASM module's text.
    const n = await node.text(body);
    if (n !== wasmText.run(body)) { same = false; console.error(`native Node module and WASM differ: ${name}`); }
  }
  if (!same) differ++;
  const hash = createHash('sha256').update(a).digest('hex').slice(0, 16);
  console.log(`${same ? 'same     ' : 'DIFFERENT'} ${name}${digest ? `  ${hash}` : ''}`);
}
let nodeSessionsDiffer = 0;
if (node) {
  // Sessions (ADR 0019 step 3) in the native Node module and in WASM: the same requests, the same answer texts. `memory` is
  // not among them: its estimate counts sizes of Rust types, which differ between 64-bit and 32-bit (WASM) targets; it is
  // compared with the native `layout-command --sessions` below instead.
  const session = await node.openSession();
  const handle = wasmText.open();
  const organic = graph('benchmarks/datasets/organic-100.json');
  const mixed = graph('benchmarks/datasets/mixed-layouts.json');
  const script = [
    {op: 'import', graph: mixed, limit: 200},
    {op: 'direction', direction: 'lr'},
    {op: 'toggle', nodeId: mixed.nodes.find(n => mixed.nodes.some(m => m.parentId === n.id)).id},
    {op: 'scene'},
    {op: 'columns'},
    {op: 'payload', set: {nodes: {[mixed.nodes[0].id]: {tag: 'x'}}}},
    {op: 'details', nodeId: mixed.nodes[0].id},
    {op: 'export'},
    {op: 'overview', graph: organic},
    {op: 'intent'},
  ];
  let sessionDiffer = 0;
  for (const request of script) {
    const body = JSON.stringify(request);
    const mine = await session.text(body);
    if (JSON.parse(mine).ok !== true) { sessionDiffer++; console.error(`session request failed: ${request.op}: ${mine.slice(0, 200)}`); }
    if (mine !== wasmText.session(handle, body)) {
      sessionDiffer++;
      console.error(`native Node module and WASM differ in a session: ${request.op}`);
    }
  }
  // The session's memory estimate, against the native binary's session of the same requests (the module's own heap count,
  // `heapBytes`, is the adapter's and left out).
  const memory = (await session.text('{"op":"memory"}')).replace(/,"heapBytes":\d+/, '');
  const lines = [{session: 'open'}, ...script.map(request => ({session: 1, request})), {session: 1, request: {op: 'memory'}}];
  const binary = execFileSync(native, ['--sessions'], {input: lines.map(l => JSON.stringify(l)).join('\n') + '\n', maxBuffer: 1 << 30})
    .toString().trim().split('\n').at(-1);
  if (memory !== binary) { sessionDiffer++; console.error('native Node module and layout-command differ in a session: memory'); }
  await session.close();
  wasmText.close(handle);
  nodeSessionsDiffer = sessionDiffer;
  console.error(`native Node module: ${requests.length} requests and ${script.length} session requests compared with WASM byte for byte (memory with layout-command), ${sessionDiffer ? 'DIFFERENT' : 'same'}`);
}
console.log(`${process.platform}-${process.arch}: ${requests.length - differ} of ${requests.length} identical between native and WASM`);
process.exit(differ || nodeSessionsDiffer ? 1 : 0);
