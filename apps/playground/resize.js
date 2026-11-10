// Resizing a leaf from its bottom-right corner. The size the user chooses is a requirement for the engine (the measure
// operation), not a measurement; the engine lays out around it.

/** The corner area, in scene units, around a leaf's bottom-right corner in which a press starts a resize. */
export const RESIZE_CORNER = 14;

/** The smallest leaf the playground asks for: an engineering minimum, not a measured label size. */
export const MIN_LEAF = { width: 60, height: 30 };

/** Whether a scene point lies in the corner area of a box. */
export function nearResizeCorner(point, bounds) {
  return (
    point.x >= bounds.x + bounds.width - RESIZE_CORNER &&
    point.y >= bounds.y + bounds.height - RESIZE_CORNER &&
    point.x <= bounds.x + bounds.width + RESIZE_CORNER &&
    point.y <= bounds.y + bounds.height + RESIZE_CORNER
  );
}

/**
 * The size of a leaf after its corner has moved from `start` to `pointer`. The box keeps its origin and is never below
 * `MIN_LEAF`; whole units only.
 */
export function resizedLeaf(bounds, start, pointer) {
  return {
    width: Math.max(MIN_LEAF.width, Math.round(bounds.width + pointer.x - start.x)),
    height: Math.max(MIN_LEAF.height, Math.round(bounds.height + pointer.y - start.y)),
  };
}
