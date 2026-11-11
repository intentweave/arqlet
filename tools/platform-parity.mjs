#!/usr/bin/env node
// Cross-platform determinism: every request below must give the same response from the native `layout-command` of this
// platform and from the WASM engine (which computes the same everywhere, so native = WASM on every platform means every
// platform computes the same). Prints one line per request and, with `--digest`, the SHA-256 of each response, so that
// runs on different platforms can be compared directly. Exits 1 on any difference.
//
//   cargo build --release -p arqlet-runtime --bin layout-command --locked
//   cargo build -p arqlet-wasm --target wasm32-unknown-unknown --release --locked
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
  ['elkLayout', {op: 'elkLayout', graph: elk}],
  ['elkLayout refined', {op: 'elkLayout', graph: elk, refine: true}],
  ['overview 10 000', {op: 'overview', graph: graph('benchmarks/datasets/overview-10000.json')}],
];
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
