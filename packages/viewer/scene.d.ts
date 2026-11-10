import type {Point, Scene} from '@arqlet/contracts';

/** Returns the scene if its schema is supported (v1); throws otherwise. */
export function checkScene(scene: Scene): Scene;
/** SVG path data (`M x y L x y ...`) for a polyline. */
export function routePath(points: Point[]): string;
