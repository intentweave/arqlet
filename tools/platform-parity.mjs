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
import {readFileSync, existsSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {join} from 'node:path';
import {createEngine} from '../packages/engine-client/wasm-client.js';

const ROOT = new URL('../', import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, '$1');
const exe = process.platform === 'win32' ? '.exe' : '';
const native = join(ROOT, `target/release/layout-command${exe}`);
const wasm = join(ROOT, 'target/wasm32-unknown-unknown/release/arqlet_wasm.wasm');
for (const f of [native, wasm]) if (!existsSync(f)) { console.error(`Missing ${f}; build it first (see the header).`); process.exit(2); }
const run = await createEngine(readFileSync(wasm));
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
  // The corridor plan of organic areas (`organicCorridors`, #156): the report is computed in integers.
  ['import organic corridors', {op: 'import', graph: graph('benchmarks/datasets/vehicle-buses-organic.json'), limit: 200,
    options: {organicCorridors: {}, busStyle: 'lines'}}],
  ['elkLayout', {op: 'elkLayout', graph: elk}],
  ['elkLayout refined', {op: 'elkLayout', graph: elk, refine: true}],
  ['overview 10 000', {op: 'overview', graph: graph('benchmarks/datasets/overview-10000.json')}],
];
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
const digest = process.argv.includes('--digest');
let differ = 0;
for (const [name, request] of requests) {
  const body = JSON.stringify(request);
  const a = JSON.stringify(JSON.parse(execFileSync(native, {input: body, maxBuffer: 1 << 30}).toString()));
  const b = JSON.stringify(run(request));
  const same = a === b;
  if (!same) differ++;
  const hash = createHash('sha256').update(a).digest('hex').slice(0, 16);
  console.log(`${same ? 'same     ' : 'DIFFERENT'} ${name}${digest ? `  ${hash}` : ''}`);
}
console.log(`${process.platform}-${process.arch}: ${requests.length - differ} of ${requests.length} identical between native and WASM`);
process.exit(differ ? 1 : 0);
