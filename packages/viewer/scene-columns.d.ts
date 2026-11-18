import type {Scene} from '@arqlet/contracts';

/**
 * Provisional (ADR 0019, step 4): the columns of a scene as `session.columns()` of `@arqlet/engine-client` gives them
 * (`SceneColumns` in `@arqlet/engine-client/columns.js`): typed arrays and id tables. Declared by what the viewer reads,
 * so that this package does not depend on the engine client.
 */
export interface SceneColumnsLike {
  schemaVersion: number;
  revision: number;
  layoutVersion: number;
  nodeBounds: Float32Array;
  [column: string]: unknown;
}

/** Whether `scene` is the columns of a scene rather than a scene object. */
export function isSceneColumns(scene: Scene | SceneColumnsLike): scene is SceneColumnsLike;

/**
 * The scene object of a scene's columns, with the columns' `f32` coordinates. A route end outside the scene (which the
 * engine does not produce) reads as an empty id; a conflict leaves out a party outside the scene.
 */
export function sceneOfColumns(columns: SceneColumnsLike): Scene;
