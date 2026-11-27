// Provisional (ADR 0030): the Arqlet engine as a native Node.js module, with the interface of the WASM engine of
// `@arqlet/engine-client` (`run`, `run.openSession()` with `run`, `close`, `columns`, `overview`), asynchronous: every
// call that computes runs on libuv's thread pool and resolves with the same answer, byte for byte, as the WASM engine.
// `createSession` of `@arqlet/engine-client` takes it as it takes `workerRun`.
import {createRequire} from 'node:module';
import {existsSync} from 'node:fs';
import {join, resolve} from 'node:path';

const here = import.meta.dirname;

/** The platform key of the prebuilt module and its package (`darwin-arm64`, `linux-x64-gnu`, `win32-x64-msvc`, ...). */
let glibc;
export function platformKey(platform = process.platform, arch = process.arch) {
  if (platform === 'linux') {
    // glibc or musl: only glibc builds are planned (ADR 0030); a musl system builds from source. The report is costly
    // (it collects the whole process state), so it is read once.
    glibc ??= Boolean(process.report?.getReport?.().header?.glibcVersionRuntime);
    return `linux-${arch}-${glibc ? 'gnu' : 'musl'}`;
  }
  if (platform === 'win32') return `win32-${arch}-msvc`;
  return `${platform}-${arch}`;
}

// Where the module is looked for, in order: the path given, `ARQLET_NODE_ADDON`, a local build beside this file
// (`build.mjs`), the platform's package (`@arqlet/node-<key>`, an optional dependency once published).
function candidates(path) {
  const list = [];
  // Absolute paths: `process.dlopen` would otherwise resolve a relative one against the process's working directory, or
  // search the system's library path for a bare name.
  if (path) return [resolve(String(path))];
  if (process.env.ARQLET_NODE_ADDON) list.push(resolve(process.env.ARQLET_NODE_ADDON));
  list.push(join(here, `arqlet.${process.platform}-${process.arch}.node`));
  try {
    list.push(createRequire(import.meta.url).resolve(`@arqlet/node-${platformKey()}/arqlet.node`));
  } catch {
    // Not installed: no prebuilt module for this platform.
  }
  return list;
}

const loaded = new Map();

/**
 * The module's binding (`NativeEngine`), loaded once per file. `process.dlopen` loads it under any file name, so a
 * cargo build (`target/release/libarqlet_node.dylib`) loads as it is. Throws when no module is found or it fails to load.
 */
export function nativeBinding({path} = {}) {
  const tried = [];
  for (const file of candidates(path)) {
    if (!existsSync(file)) { tried.push(file); continue; }
    if (!loaded.has(file)) {
      const module = {exports: {}};
      process.dlopen(module, file);
      loaded.set(file, module.exports);
    }
    return loaded.get(file);
  }
  throw Error(`No native Arqlet module for ${platformKey()}; looked for ${tried.join(', ')}`);
}

/** Whether a native module can be loaded on this machine. */
export function nativeAvailable(options) {
  return nativeLoadError(options) === null;
}

/** Why no native module can be loaded (the error of the lookup or of `process.dlopen`), or null when one loads. */
export function nativeLoadError(options) {
  try { nativeBinding(options); return null; } catch (error) { return error; }
}

const views = {f32: Float32Array, u32: Uint32Array, i32: Int32Array, f64: Float64Array};

// The engine's buffers as typed arrays of the listed types. The buffers are copies made for this call; one whose start is
// not aligned for its type is copied once more.
function typed(listed, buffers) {
  const result = {};
  listed.forEach((c, index) => {
    const View = views[c.type];
    const b = buffers[index] ?? new Uint8Array(0);
    result[c.name] = b.byteOffset % View.BYTES_PER_ELEMENT === 0
      ? new View(b.buffer, b.byteOffset, b.byteLength / View.BYTES_PER_ELEMENT)
      : new View(new Uint8Array(b).buffer);
  });
  return result;
}

/**
 * Loads the native engine. `sessionBytes` limits the sessions' estimated weight together (the least recently used idle
 * session is dropped beyond it and answered as unknown afterwards). Answers an asynchronous `run` with `openSession()`,
 * `sessions()` and `kind: 'native'`. Throws when no native module is available (see `loadEngine` for a fallback).
 */
export function loadNativeEngine({path, sessionBytes} = {}) {
  const {NativeEngine} = nativeBinding({path});
  const engine = new NativeEngine(sessionBytes ?? undefined);
  const run = async request => JSON.parse(await engine.run(JSON.stringify(request)));
  run.kind = 'native';
  /** The answer text of a request, unparsed (the engine's bytes). */
  run.text = text => engine.run(text);
  run.sessions = () => JSON.parse(engine.sessions());
  run.openSession = async () => {
    const id = engine.openSession();
    // One request at a time per session, in the order sent: the next starts after the answer to the previous one.
    let queue = Promise.resolve();
    const serial = step => {
      const result = queue.then(step);
      queue = result.catch(() => {});
      return result;
    };
    const text = body => serial(() => engine.sessionRun(id, body));
    return {
      id,
      run: request => text(JSON.stringify(request)).then(JSON.parse),
      text,
      close: () => serial(() => ({ok: engine.closeSession(id)})),
      // The scene as columns. The numbers are copies (they stay valid after later requests); the id tables are copies.
      columns: () => serial(async () => {
        const {answer, columns} = await engine.sessionColumns(id, '{"op":"columns"}', false);
        const meta = JSON.parse(answer);
        if (!meta.ok) throw Error(meta.error);
        const {ok, columns: listed, ...result} = meta;
        return {...result, ...typed(listed, columns)};
      }),
      // An `overview` request whose numbers come back as typed arrays, shaped like the JSON answer. Always copies.
      overview: request => serial(async () => {
        const {answer, columns} = await engine.sessionColumns(id, JSON.stringify({...request, columns: true}), true);
        const parsed = JSON.parse(answer);
        if (!parsed.ok) return parsed;
        const {columns: listed, ...rest} = parsed.overview;
        return {...parsed, overview: {...rest, ...typed(listed, columns)}};
      }),
    };
  };
  return run;
}

/**
 * Loads an engine for a Node host: `engine` chooses `'native'` (an error without a native module), `'wasm'` (the WASM
 * engine of `@arqlet/engine-client`, synchronous, on this thread) or `'auto'` (native where available, else WASM). The
 * default is `ARQLET_ENGINE` from the environment, else `'auto'`. The answer's `kind` says which one was loaded; both give
 * the same answers. When `'auto'` falls back to WASM, `fallbackReason` says why.
 */
export async function loadEngine({engine = process.env.ARQLET_ENGINE || 'auto', path, sessionBytes, wasmUrl} = {}) {
  if (!['auto', 'native', 'wasm'].includes(engine)) throw Error(`Unknown engine "${engine}" (auto, native or wasm)`);
  const problem = engine === 'wasm' ? null : nativeLoadError({path});
  if (engine === 'native' || (engine === 'auto' && problem === null)) return loadNativeEngine({path, sessionBytes});
  const {loadEngine: loadWasm} = await import('@arqlet/engine-client/engine.js');
  const run = await loadWasm(wasmUrl);
  run.kind = 'wasm';
  // With 'auto': why the native module was not used (a missing module, or the error of loading it).
  if (problem) run.fallbackReason = problem.message;
  return run;
}
