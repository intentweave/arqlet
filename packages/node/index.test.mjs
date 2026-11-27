// Tests of the native Node module (ADR 0030). They need a build of it: `node packages/node/build.mjs` (or
// `ARQLET_NODE_ADDON=<file>`); without one they are skipped and say so. The WASM comparisons also need
// `target/wasm32-unknown-unknown/release/arqlet_wasm.wasm` (cargo build -p arqlet-wasm --target wasm32-unknown-unknown
// --release).
import test from 'node:test';
import assert from 'node:assert/strict';
import {existsSync, readFileSync} from 'node:fs';
import {join, relative} from 'node:path';
import {loadEngine, loadNativeEngine, nativeAvailable, nativeLoadError, platformKey} from './index.js';
import {createEngine} from '../engine-client/wasm-client.js';
import {createSession} from '../engine-client/session.js';

const root = join(import.meta.dirname, '../..');
const wasmFile = join(root, 'target/wasm32-unknown-unknown/release/arqlet_wasm.wasm');
const native = nativeAvailable();
// In CI (or with ARQLET_REQUIRE_NATIVE) a module that is missing or fails to load is a failure, not a reason to skip.
if (!native && (process.env.CI || process.env.ARQLET_REQUIRE_NATIVE)) throw nativeLoadError();
const skipNative = native ? false : `no native module (${nativeLoadError().message}); build one with node packages/node/build.mjs`;
const skipWasm = skipNative || (existsSync(wasmFile) ? false : 'no WASM build in target/');

// A deterministic graph of `n` nodes in containers of ten, with cycles and parallel edges.
function graph(n, seed = 1) {
  let s = seed;
  const next = () => (s = (s * 1103515245 + 12345) % 2147483648) / 2147483648;
  const groups = Math.ceil(n / 10);
  const nodes = [
    ...Array.from({length: groups}, (_, g) => ({id: `g${g}`, kind: 'group', label: `Group ${g}`})),
    ...Array.from({length: n}, (_, i) => ({id: `n${i}`, kind: 'service', label: `Node ${i}`, parentId: `g${Math.floor(i / 10)}`})),
  ];
  const edges = Array.from({length: Math.round(n * 1.3)}, (_, i) => ({
    id: `e${i}`, kind: 'calls', source: `n${Math.floor(next() * n)}`, target: `n${Math.floor(next() * n)}`,
  }));
  return {nodes, edges};
}

const script = n => [
  {op: 'import', graph: graph(n, n), limit: 400},
  {op: 'direction', direction: 'lr'},
  {op: 'toggle', nodeId: 'g0'},
  {op: 'scene'},
  {op: 'intent'},
];

test('a relative path is resolved before loading', {skip: skipNative}, () => {
  const file = join(import.meta.dirname, `arqlet.${process.platform}-${process.arch}.node`);
  if (!existsSync(file)) return;
  assert.equal(nativeAvailable({path: relative(process.cwd(), file)}), true);
});

test('platform key names the prebuilt package', () => {
  assert.equal(platformKey('darwin', 'arm64'), 'darwin-arm64');
  assert.equal(platformKey('win32', 'x64'), 'win32-x64-msvc');
  assert.match(platformKey('linux', 'x64'), /^linux-x64-(gnu|musl)$/);
});

test('an unknown engine choice is refused, a missing module is reported', async () => {
  await assert.rejects(loadEngine({engine: 'gpu'}), /Unknown engine/);
  assert.throws(() => loadNativeEngine({path: join(root, 'no-such-module.node')}), /No native Arqlet module/);
  assert.equal(nativeAvailable({path: join(root, 'no-such-module.node')}), false);
  assert.match(nativeLoadError({path: join(root, 'no-such-module.node')}).message, /No native Arqlet module/);
});

test('dispatch, sessions and columns answer as the WASM engine', {skip: skipWasm}, async () => {
  const run = loadNativeEngine();
  const wasm = await createEngine(readFileSync(wasmFile));
  for (const request of [script(30)[0], {op: 'elkLayout', graph: {id: 'root', children: [{id: 'a', width: 30, height: 20}, {id: 'b', width: 30, height: 20}], edges: [{id: 'e', sources: ['a'], targets: ['b']}]}}]) {
    assert.deepEqual(await run(request), wasm(request));
  }
  const [mine, theirs] = [await run.openSession(), wasm.openSession()];
  for (const request of script(40)) assert.deepEqual(await mine.run(request), theirs.run(request));
  const [a, b] = [await mine.columns(), theirs.columns()];
  assert.deepEqual(Object.keys(a).sort(), Object.keys(b).sort());
  for (const key of Object.keys(b)) {
    if (ArrayBuffer.isView(b[key])) {
      assert.equal(a[key].constructor, b[key].constructor, key);
      assert.deepEqual([...a[key]], [...b[key]], key);
    } else assert.deepEqual(a[key], b[key], key);
  }
  const overview = {op: 'overview', graph: graph(200)};
  const [o, p] = [await mine.overview(overview), theirs.overview(overview, {copy: true})];
  for (const key of Object.keys(p.overview)) {
    const [x, y] = [o.overview[key], p.overview[key]];
    assert.deepEqual(ArrayBuffer.isView(y) ? [...x] : x, ArrayBuffer.isView(y) ? [...y] : y, key);
  }
  await mine.close();
  theirs.close();
});

test('a timer fires while a long layout runs', {skip: skipNative}, async () => {
  const run = loadNativeEngine();
  const request = {op: 'import', graph: graph(1500), limit: 1500};
  let ticks = 0;
  const timer = setInterval(() => ticks++, 2);
  const started = performance.now();
  const answer = await run(request);
  const took = performance.now() - started;
  clearInterval(timer);
  assert.equal(answer.ok, true, answer.error);
  // The layout takes far longer than a tick; the event loop kept running the timer meanwhile.
  assert.ok(took > 20, `layout took ${took} ms`);
  assert.ok(ticks >= 3, `${ticks} ticks in ${took.toFixed(0)} ms`);
});

test('sessions in parallel answer as sessions one after the other', {skip: skipNative}, async () => {
  const run = loadNativeEngine();
  const sizes = [20, 35, 50, 65, 80, 95];
  const serial = [];
  for (const n of sizes) {
    const s = await run.openSession();
    const answers = [];
    for (const request of script(n)) answers.push(await s.text(JSON.stringify(request)));
    serial.push(answers);
    await s.close();
  }
  const parallel = await Promise.all(sizes.map(async n => {
    const s = await run.openSession();
    // All requests of a session sent at once: the session runs them one at a time, in order.
    const answers = await Promise.all(script(n).map(request => s.text(JSON.stringify(request))));
    await s.close();
    return answers;
  }));
  assert.deepEqual(parallel, serial);
  assert.deepEqual(run.sessions().sessions, []);
});

test('a closed session is unknown; the byte limit drops the least recently used idle session', {skip: skipNative}, async () => {
  const one = loadNativeEngine();
  const probe = await one.openSession();
  await probe.run(script(30)[0]);
  const weight = one.sessions().bytes;
  assert.ok(weight > 0);
  assert.deepEqual(await probe.close(), {ok: true});
  assert.deepEqual(await probe.run({op: 'scene'}), {ok: false, error: 'Unknown session'});

  const run = loadNativeEngine({sessionBytes: Math.floor(weight * 1.5)});
  const [a, b] = [await run.openSession(), await run.openSession()];
  await a.run(script(30)[0]);
  const answer = await b.run(script(30)[0]);
  assert.equal(answer.evictedSessions, 1);
  assert.deepEqual(run.sessions().sessions, [b.id]);
  assert.equal(run.sessions().evicted, 1);
  assert.equal((await a.run({op: 'scene'})).error, 'Unknown session');
});

test('memory and release answer the module heap', {skip: skipNative}, async () => {
  const run = loadNativeEngine();
  const s = await run.openSession();
  await s.run(script(30)[0]);
  const memory = await s.run({op: 'memory'});
  assert.equal(memory.ok, true);
  assert.ok(memory.heapBytes > memory.total, JSON.stringify(memory));
  assert.equal(memory.linearMemoryBytes, undefined);
  const released = await s.run({op: 'release', parts: ['columns']});
  assert.equal(released.ok, true);
  assert.ok(released.heapBytes > 0);
});

test('createSession runs on it, resident, and recycles to the WASM engine', {skip: skipWasm}, async () => {
  const run = loadNativeEngine();
  const session = createSession(run);
  assert.equal(session.resident, true);
  await session.import(graph(40), {limit: 400});
  const toggled = await session.toggle('g1');
  assert.equal(toggled.ok, true, toggled.error);
  const before = await session.exportSnapshot();
  const wasm = await createEngine(readFileSync(wasmFile));
  const recycled = await session.recycle(wasm);
  assert.equal(recycled.ok, true, recycled.error);
  assert.deepEqual(await session.exportSnapshot(), before);
  assert.deepEqual(run.sessions().sessions, []);
});

test('loadEngine chooses native or WASM', {skip: skipWasm}, async () => {
  const native = await loadEngine({engine: 'native'});
  assert.equal(native.kind, 'native');
  const auto = await loadEngine({engine: 'auto'});
  assert.equal(auto.kind, 'native');
  const wasm = await loadEngine({engine: 'wasm', wasmUrl: new URL(`file://${wasmFile}`)});
  assert.equal(wasm.kind, 'wasm');
  assert.equal(wasm.fallbackReason, undefined);
  const fallback = await loadEngine({engine: 'auto', path: 'no-such-module.node', wasmUrl: new URL(`file://${wasmFile}`)});
  assert.equal(fallback.kind, 'wasm');
  assert.match(fallback.fallbackReason, /No native Arqlet module/);
  const request = script(25)[0];
  assert.deepEqual(await native(request), wasm(request));
});
