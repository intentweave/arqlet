// Compile-time contract check. The engine fixtures are real JSON output of the Rust engine; each must be
// assignable to the generated type. A field that the Rust side stops emitting, or emits with another shape
// (for example null where the type says an object), fails `pnpm --filter @arqlet/contracts typecheck`.
//
// Limits, both deliberate:
// - JSON imports widen string literals to `string`, so enum values are checked by the Rust contract test
//   (crates/contracts/tests/contract.rs), not here. `Relax` keeps every other part of the shape exact.
// - Extra JSON keys are not rejected; the Rust serialization tests cover what the engine emits.
import type {Scene, Snapshot} from './generated/index.js';
import sceneFixture from '../../viewer/fixtures/bpmn-pools.scene.json';
import stateSceneFixture from '../../viewer/fixtures/state-tcp.scene.json';
import snapshotFixture from '../../viewer/fixtures/bpmn-pools.snapshot.json';
import stateSnapshotFixture from '../../viewer/fixtures/state-tcp.snapshot.json';

type Relax<T> = T extends string
  ? string
  : T extends number | boolean | null | undefined
    ? T
    : T extends Array<infer U>
      ? Array<Relax<U>>
      : T extends object
        ? {[K in keyof T]: Relax<T[K]>}
        : T;

export const scenes: [Relax<Scene>, Relax<Scene>] = [sceneFixture, stateSceneFixture];
export const snapshots: [Relax<Snapshot>, Relax<Snapshot>] = [snapshotFixture, stateSnapshotFixture];
