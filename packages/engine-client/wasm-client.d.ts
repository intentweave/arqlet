/** A request to the engine's dispatcher: an operation name and its fields (see the repository's docs). Provisional. */
export interface EngineRequest {
  op: string;
  /** Worker threads for native builds; ignored in WASM, where jobs run serially. Output is identical for any value. */
  threads?: number;
  [field: string]: unknown;
}

/** The dispatcher's answer: `ok: true` with the operation's fields (`snapshot`, `scene`, `graph`, ...), or an error. */
export type EngineResponse =
  | ({ok: true} & {[field: string]: any})
  | {ok: false; error: string; [field: string]: unknown};

/** Runs one request synchronously on the engine. */
export type Run = (request: EngineRequest) => EngineResponse;

export interface CreateEngineOptions {
  /** Receives each progressive publication (JSON text) while the call that produced it is still running. */
  onPublish?: (message: string) => void;
}

/** Instantiates the WASM engine from its bytes. */
export function createEngine(bytes: BufferSource, options?: CreateEngineOptions): Promise<Run>;
