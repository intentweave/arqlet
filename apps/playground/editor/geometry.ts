// Pure geometry of manual edits. No engine, no DOM: every rule here is checked by the reference validator as well.
import type {Edge, Point, PortSide, Rect} from '@arqlet/contracts';

const EPSILON = 1e-9;

export type Axis = 'horizontal' | 'vertical';

/** The rectangle moved by (dx, dy), kept inside `parent` (containment is never changed by a drag). */
export function movedInside(rect: Rect, dx: number, dy: number, parent: Rect | null): {rect: Rect; clamped: boolean} {
  const wanted = {...rect, x: rect.x + dx, y: rect.y + dy};
  if (!parent) return {rect: wanted, clamped: false};
  const x = clamp(wanted.x, parent.x, parent.x + parent.width - rect.width);
  const y = clamp(wanted.y, parent.y, parent.y + parent.height - rect.height);
  return {rect: {...rect, x, y}, clamped: x !== wanted.x || y !== wanted.y};
}

/**
 * The bounds of a drawing after curation: the baseline's bounds, grown to include every box and route point the curator
 * placed. Boxes get `margin` around them; route points do not. The baseline's bounds are never shrunk.
 */
export function boundsAround(base: Rect, boxes: readonly Rect[], points: readonly Point[], margin: number): Rect {
  let minX = base.x;
  let minY = base.y;
  let maxX = base.x + base.width;
  let maxY = base.y + base.height;
  for (const box of boxes) {
    minX = Math.min(minX, box.x - margin);
    minY = Math.min(minY, box.y - margin);
    maxX = Math.max(maxX, box.x + box.width + margin);
    maxY = Math.max(maxY, box.y + box.height + margin);
  }
  for (const point of points) {
    minX = Math.min(minX, point.x);
    minY = Math.min(minY, point.y);
    maxX = Math.max(maxX, point.x);
    maxY = Math.max(maxY, point.y);
  }
  return {x: minX, y: minY, width: maxX - minX, height: maxY - minY};
}

/** Space kept between a node and the right or bottom border of a container that has grown to hold it. */
export const GROW_PADDING = 12;

/**
 * Where a dragged node lands and how its containers respond. `ancestors` runs from the direct parent upwards. Without
 * `grow` the node stays inside its parent (`movedInside`) and `grown` equals `ancestors`. With `grow` the containers
 * extend to the right and bottom as far as the node needs; their top-left corners stay, so their children keep their
 * positions. The left and top borders still stop the node.
 */
export function movedWithin(
  rect: Rect,
  dx: number,
  dy: number,
  ancestors: readonly Rect[],
  grow: boolean,
): {rect: Rect; grown: Rect[]; clamped: boolean} {
  if (!grow) {
    const {rect: moved, clamped} = movedInside(rect, dx, dy, ancestors[0] ?? null);
    return {rect: moved, grown: [...ancestors], clamped};
  }
  const wanted = {...rect, x: rect.x + dx, y: rect.y + dy};
  const parent = ancestors[0];
  const x = parent ? Math.max(wanted.x, parent.x) : wanted.x;
  const y = parent ? Math.max(wanted.y, parent.y) : wanted.y;
  const placed = {...rect, x, y};
  const grown: Rect[] = [];
  let inner = placed;
  for (const container of ancestors) {
    const right = Math.max(container.x + container.width, inner.x + inner.width + GROW_PADDING);
    const bottom = Math.max(container.y + container.height, inner.y + inner.height + GROW_PADDING);
    inner = {...container, width: right - container.x, height: bottom - container.y};
    grown.push(inner);
  }
  return {rect: placed, grown, clamped: x !== wanted.x || y !== wanted.y};
}

/** Whether a container's size changed, so its border moved. */
export function resized(before: Rect, after: Rect): boolean {
  return before.width !== after.width || before.height !== after.height;
}

/** Whether two rectangles share interior area. Touching edges do not count. */
export function overlaps(a: Rect, b: Rect): boolean {
  return a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height;
}

/**
 * Routes that run along a container border, not routes that lie inside it: a segment is near the border when it
 * reaches the margin zone around the rectangle and not both of its ends lie inside the rectangle shrunk by the margin.
 */
export function routesNearBorder(routes: Readonly<Record<string, readonly Point[]>>, rect: Rect, margin: number): string[] {
  const inner = {x: rect.x + margin, y: rect.y + margin, width: rect.width - 2 * margin, height: rect.height - 2 * margin};
  const inside = (p: Point) => p.x >= inner.x && p.x <= inner.x + inner.width && p.y >= inner.y && p.y <= inner.y + inner.height;
  return Object.entries(routes)
    .filter(([, points]) =>
      points.some((point, index) => {
        const next = points[index + 1];
        return next !== undefined && segmentNear(point, next, rect, margin) && !(inside(point) && inside(next));
      }))
    .map(([id]) => id);
}

/** The axis of the segment from `a` to `b`, or null when it is neither horizontal nor vertical. */
export function axisOf(a: Point, b: Point): Axis | null {
  if (Math.abs(a.x - b.x) <= EPSILON) return 'vertical';
  if (Math.abs(a.y - b.y) <= EPSILON) return 'horizontal';
  return null;
}

/**
 * Whether segment `index` (from points[index] to points[index + 1]) may be shifted. Only an interior, orthogonal
 * segment qualifies: the first and last segments carry the endpoints, which stay attached to their nodes.
 */
export function isShiftable(points: readonly Point[], index: number): boolean {
  if (index < 1 || index + 2 > points.length - 1) return false;
  const a = points[index];
  const b = points[index + 1];
  return a !== undefined && b !== undefined && axisOf(a, b) !== null;
}

/** The shiftable segments of a route, with their axis. */
export function shiftableSegments(points: readonly Point[]): {index: number; axis: Axis}[] {
  const result: {index: number; axis: Axis}[] = [];
  for (let index = 0; index + 1 < points.length; index++) {
    if (!isShiftable(points, index)) continue;
    const axis = axisOf(points[index]!, points[index + 1]!);
    if (axis) result.push({index, axis});
  }
  return result;
}

/** Shifts one interior orthogonal segment perpendicular to its direction. Both of its corners move together. */
export function shiftSegment(points: readonly Point[], index: number, delta: number): Point[] {
  if (!isShiftable(points, index)) throw Error('Only an interior orthogonal segment can be shifted');
  const axis = axisOf(points[index]!, points[index + 1]!);
  const moved = points.map(point => ({...point}));
  for (const at of [index, index + 1]) {
    const point = moved[at]!;
    if (axis === 'vertical') point.x += delta;
    else point.y += delta;
  }
  return moved;
}

/**
 * A dogleg: the interior orthogonal segment `index` gets a jog of `depth` perpendicular to it. The jog runs in the
 * middle third of the segment, so the route keeps its endpoints and its arrival sides. The result is normalized.
 */
export function withDogleg(points: readonly Point[], index: number, depth: number): Point[] {
  if (!isShiftable(points, index)) throw Error('A dogleg needs an interior orthogonal segment');
  if (!(Math.abs(depth) > EPSILON)) throw Error('A dogleg needs a nonzero depth');
  const a = points[index]!;
  const b = points[index + 1]!;
  const horizontal = axisOf(a, b) === 'horizontal';
  const along = horizontal ? b.x - a.x : b.y - a.y;
  if (Math.abs(along) < EPSILON) throw Error('A dogleg needs a segment with length');
  const at = (t: number, across: number): Point =>
    horizontal ? {x: a.x + along * t, y: a.y + across} : {x: a.x + across, y: a.y + along * t};
  const jog = [at(1 / 3, 0), at(1 / 3, depth), at(2 / 3, depth), at(2 / 3, 0)];
  return normalizedRoute([...points.slice(0, index + 1), ...jog, ...points.slice(index + 1)]);
}

/** Drops repeated points and interior points that lie straight between their neighbours. The endpoints stay. */
export function normalizedRoute(points: readonly Point[]): Point[] {
  const distinct: Point[] = [];
  for (const point of points) {
    const last = distinct[distinct.length - 1];
    if (!last || Math.abs(last.x - point.x) > EPSILON || Math.abs(last.y - point.y) > EPSILON) distinct.push({...point});
  }
  return distinct.filter((point, index) => {
    if (index === 0 || index === distinct.length - 1) return true;
    const before = distinct[index - 1]!;
    const after = distinct[index + 1]!;
    const straight =
      (Math.abs(before.x - point.x) <= EPSILON && Math.abs(point.x - after.x) <= EPSILON) ||
      (Math.abs(before.y - point.y) <= EPSILON && Math.abs(point.y - after.y) <= EPSILON);
    return !straight;
  });
}

/** The distance from `p` to the segment `a`–`b`. */
export function distanceToSegment(p: Point, a: Point, b: Point): number {
  const dx = b.x - a.x;
  const dy = b.y - a.y;
  const lengthSquared = dx * dx + dy * dy;
  const t = lengthSquared === 0 ? 0 : clamp(((p.x - a.x) * dx + (p.y - a.y) * dy) / lengthSquared, 0, 1);
  return Math.hypot(p.x - (a.x + t * dx), p.y - (a.y + t * dy));
}

/** The shiftable segment of `points` nearest to `p`, if any. */
export function nearestShiftableSegment(points: readonly Point[], p: Point): {index: number; axis: Axis} | null {
  let best: {index: number; axis: Axis; distance: number} | null = null;
  for (const segment of shiftableSegments(points)) {
    const distance = distanceToSegment(p, points[segment.index]!, points[segment.index + 1]!);
    if (!best || distance < best.distance) best = {...segment, distance};
  }
  return best ? {index: best.index, axis: best.axis} : null;
}

/** Route ids of the edges that touch `nodeId`. These routes no longer follow their node after it moves. */
export function routesTouching(edges: readonly Edge[], nodeId: string, routeIds: Iterable<string>): string[] {
  const known = new Set(routeIds);
  return edges
    .filter(edge => known.has(edge.id) && (edge.source === nodeId || edge.target === nodeId))
    .map(edge => edge.id);
}

export function centre(rect: Rect): Point {
  return {x: rect.x + rect.width / 2, y: rect.y + rect.height / 2};
}

function clamp(value: number, low: number, high: number): number {
  return Math.min(Math.max(value, low), Math.max(low, high));
}

/**
 * Whether the segment a–b comes within `margin` of `rect` (Liang–Barsky clipping against the expanded rectangle).
 * Used to find the routes that a node box touches before and after a move.
 */
export function segmentNear(a: Point, b: Point, rect: Rect, margin: number): boolean {
  const xMin = rect.x - margin;
  const xMax = rect.x + rect.width + margin;
  const yMin = rect.y - margin;
  const yMax = rect.y + rect.height + margin;
  const dx = b.x - a.x;
  const dy = b.y - a.y;
  let enter = 0;
  let leave = 1;
  const clip = (p: number, q: number): boolean => {
    if (p === 0) return q >= 0;
    const r = q / p;
    if (p < 0) {
      if (r > leave) return false;
      if (r > enter) enter = r;
    } else {
      if (r < enter) return false;
      if (r < leave) leave = r;
    }
    return true;
  };
  return clip(-dx, a.x - xMin) && clip(dx, xMax - a.x) && clip(-dy, a.y - yMin) && clip(dy, yMax - a.y) && enter <= leave;
}

/** The route ids that have a segment within `margin` of any of the rectangles. */
export function routesNear(routes: Readonly<Record<string, readonly Point[]>>, rects: readonly Rect[], margin: number): string[] {
  return Object.entries(routes)
    .filter(([, points]) =>
      points.some((point, index) => index + 1 < points.length
        && rects.some(rect => segmentNear(point, points[index + 1]!, rect, margin))))
    .map(([id]) => id);
}

/**
 * The side of `box` that the pointer has moved past by at least `margin`, when exactly one side other than the current
 * one qualifies. A port dragged well outside its node moves to that side; anything else keeps the port where it is.
 */
export function outwardSide(point: Point, box: Rect, current: PortSide, margin = 12): PortSide | null {
  const within = (value: number, low: number, high: number) => value >= low && value <= high;
  const sides: [PortSide, boolean][] = [
    ['top', point.y < box.y - margin && within(point.x, box.x, box.x + box.width)],
    ['bottom', point.y > box.y + box.height + margin && within(point.x, box.x, box.x + box.width)],
    ['left', point.x < box.x - margin && within(point.y, box.y, box.y + box.height)],
    ['right', point.x > box.x + box.width + margin && within(point.y, box.y, box.y + box.height)],
  ];
  const found = sides.filter(([side, outward]) => outward && side !== current).map(([side]) => side);
  return found.length === 1 ? found[0]! : null;
}
