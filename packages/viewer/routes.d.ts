import type {Point, Scene} from '@arqlet/contracts';

/** Direction changes of a polyline; collinear and repeated points do not count. */
export function bendsOf(points: Point[]): number;
export function pathLength(points: Point[]): number;
/** Whether two polylines cross in their interiors (touching ends are not counted). */
export function crosses(pointsA: Point[], pointsB: Point[]): boolean;

export interface RouteDetails {
  id: string;
  kind: string;
  source: string;
  target: string;
  length: number;
  bends: number;
  crossings: string[];
  members: string[];
}

/** Details of one route; `null` when the scene has no such route. `kinds` maps an edge id to its kind. */
export function routeDetails(scene: Scene, routeId: string, kinds?: Record<string, string>): RouteDetails | null;

/** The drawing the engine's shared metrics read (operation `routeConflicts`), from a scene. */
export function drawingOf(scene: Scene): {
  nodes: {id: string; parentId: string | null; container: boolean; x: number; y: number; width: number; height: number}[];
  edges: {id: string; source: string; target: string; points: Point[]; sourcePort: string | null; targetPort: string | null}[];
  trackSpacing: number;
  buses?: unknown[];
};

/** The conflicts of one route as the engine counts them. Throws the engine's error. */
export function routeConflictsOf(
  run: (request: {op: string; [field: string]: unknown}) => {ok: boolean; [field: string]: any},
  scene: Scene,
  routeId: string,
): unknown[];
