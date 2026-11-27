import type {EngineRequest, EngineResponse, Run} from '@arqlet/engine-client/wasm-client.js';
import type {SceneColumns} from '@arqlet/engine-client/columns.js';

export type {EngineRequest, EngineResponse} from '@arqlet/engine-client/wasm-client.js';

/** The open sessions of a native engine, their estimated weight together and how many the byte limit dropped so far. */
export interface SessionList {
  ok: true;
  sessions: number[];
  bytes: number;
  evicted: number;
}

/**
 * A session whose snapshot stays in the native module (provisional). Its requests run one at a time, in the order sent;
 * different sessions run in parallel on libuv's thread pool.
 */
export interface NativeSession {
  /** The session's handle in the module. */
  readonly id: number;
  run(request: EngineRequest): Promise<EngineResponse>;
  /** A request as JSON text, answered as the engine's JSON text (unparsed). */
  text(request: string): Promise<string>;
  /** Ends the session and frees its snapshot; `ok` is false if it was not open (closed, or dropped for the byte limit). */
  close(): Promise<{ok: boolean}>;
  /** The scene as columns; the numbers are copies and stay valid after later requests. Throws the engine's error. */
  columns(): Promise<SceneColumns>;
  /** An `overview` request answered with typed arrays instead of JSON arrays (always copies). */
  overview(request: EngineRequest): Promise<EngineResponse>;
}

/** The native engine: an asynchronous `run` (on libuv's thread pool) with resident sessions. Provisional. */
export type NativeRun = ((request: EngineRequest) => Promise<EngineResponse>) & {
  readonly kind: 'native';
  /** A request as JSON text, answered as the engine's JSON text (unparsed). */
  text(request: string): Promise<string>;
  openSession(): Promise<NativeSession>;
  sessions(): SessionList;
};

/** The WASM engine of `@arqlet/engine-client` (synchronous, on the calling thread), marked as such. */
export type WasmRun = Run & {
  kind: 'wasm';
  /** With `engine: 'auto'`: why the native module was not used (not found, or the error of loading it). */
  fallbackReason?: string;
};

export interface NativeOptions {
  /** The module file to load instead of the default lookup (any file name; a cargo build loads as it is). */
  path?: string;
  /** A limit on the sessions' estimated weight together, in bytes; the least recently used idle session goes first. */
  sessionBytes?: number;
}

export interface LoadEngineOptions extends NativeOptions {
  /** `'native'`, `'wasm'` or `'auto'` (native where available). Default: `ARQLET_ENGINE`, else `'auto'`. */
  engine?: 'auto' | 'native' | 'wasm';
  /** The WASM module for the fallback; default: the one shipped with `@arqlet/engine-client`. */
  wasmUrl?: string | URL;
}

/** The key of this platform's prebuilt module and package: `darwin-arm64`, `linux-x64-gnu`, `win32-x64-msvc`, ... */
export function platformKey(platform?: string, arch?: string): string;

/**
 * The raw binding of the native module (its `NativeEngine` class); loaded once per file. Throws without a module. Unlike
 * `loadNativeEngine`, it does not order the calls of one session: concurrent `sessionRun` calls on one handle are
 * serialized by the session's lock but may run in any order.
 */
export function nativeBinding(options?: {path?: string}): {NativeEngine: new (sessionBytes?: number) => unknown};

/** Whether a native module can be loaded on this machine. */
export function nativeAvailable(options?: {path?: string}): boolean;

/** Why no native module can be loaded (the lookup's or `process.dlopen`'s error), or null when one loads. */
export function nativeLoadError(options?: {path?: string}): Error | null;

/** Loads the native engine. Throws when no native module is available. */
export function loadNativeEngine(options?: NativeOptions): NativeRun;

/** Loads the native engine or the WASM engine, as `engine` chooses; `kind` on the answer says which. */
export function loadEngine(options?: LoadEngineOptions): Promise<NativeRun | WasmRun>;
