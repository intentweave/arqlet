import type {ConfigSet, Snapshot} from '@arqlet/contracts';
import type {EngineRequest, EngineResponse} from './wasm-client.js';

/** A versioned preset, as in the repository's `presets/*.json`. Provisional. */
export interface Preset {
  id: string;
  version: number;
  title?: string;
  description?: string;
  graph: {defaultDirection: string; defaultStrategy?: string; defaultRouting?: string};
  refine?: boolean;
  budgetMs?: number;
  configSet: ConfigSet;
  [field: string]: unknown;
}

/** Which preset a snapshot is in: `preset` only for the exact `<id>@<version>` in `presets`; an older one is `stale`. */
export function presetState(snapshot: Snapshot, presets: Map<string, Preset>): {preset: Preset | null; stale: boolean};

/** Applies a preset as two engine operations (direction, then configuration). Nothing is retried. */
export function applyPreset(args: {
  preset: Preset;
  snapshot: Snapshot;
  run: (request: EngineRequest) => EngineResponse | Promise<EngineResponse>;
  stillCurrent: () => boolean;
}): Promise<{ok: true; snapshot: Snapshot} | {ok: false; error: string}>;
