// Where a dragged port belongs: the side of the node's border nearest to the pointer and the offset along it, in the terms the
// engine's `port` operation takes. A rectangle's offset runs from the left (top) end of the side to the other end; on a circle
// the side covers the 90 degrees around its direction and the offset runs through them, 0.5 in the middle (ports.rs).

const clamp01 = value => Math.min(1, Math.max(0, value));

export function placementOnBorder(bounds, shape, point) {
  if (shape === 'circle') {
    const cx = bounds.x + bounds.width / 2, cy = bounds.y + bounds.height / 2;
    const turn = Math.atan2(point.y - cy, point.x - cx);
    // Right 0, bottom pi/2, left pi, top 3pi/2: the side whose direction is nearest, and the angle from it.
    const sides = [['right', 0], ['bottom', Math.PI / 2], ['left', Math.PI], ['top', 3 * Math.PI / 2]];
    const wrap = angle => Math.atan2(Math.sin(angle), Math.cos(angle));
    const [side, base] = sides.reduce((best, entry) => Math.abs(wrap(turn - entry[1])) < Math.abs(wrap(turn - best[1])) ? entry : best);
    return {side, offset: clamp01(wrap(turn - base) / (Math.PI / 2) + 0.5)};
  }
  const right = bounds.x + bounds.width, bottom = bounds.y + bounds.height;
  const x = Math.min(right, Math.max(bounds.x, point.x)), y = Math.min(bottom, Math.max(bounds.y, point.y));
  // Distance to each side's segment; the nearest side wins, ties keep the order left, right, top, bottom.
  const distance = {
    left: Math.hypot(point.x - bounds.x, point.y - y),
    right: Math.hypot(point.x - right, point.y - y),
    top: Math.hypot(point.x - x, point.y - bounds.y),
    bottom: Math.hypot(point.x - x, point.y - bottom),
  };
  const side = Object.keys(distance).reduce((best, key) => distance[key] < distance[best] ? key : best);
  const offset = side === 'left' || side === 'right' ? (y - bounds.y) / bounds.height : (x - bounds.x) / bounds.width;
  return {side, offset: clamp01(offset)};
}

/** Where the port is drawn for a placement: the inverse of `placementOnBorder`, for the preview of the drag. */
export function positionOnBorder(bounds, shape, {side, offset}) {
  if (shape === 'circle') {
    const base = {right: 0, bottom: Math.PI / 2, left: Math.PI, top: 3 * Math.PI / 2}[side];
    const angle = base + (offset - 0.5) * (Math.PI / 2);
    const radius = Math.min(bounds.width, bounds.height) / 2;
    return {x: bounds.x + bounds.width / 2 + radius * Math.cos(angle), y: bounds.y + bounds.height / 2 + radius * Math.sin(angle)};
  }
  switch (side) {
    case 'left': return {x: bounds.x, y: bounds.y + offset * bounds.height};
    case 'right': return {x: bounds.x + bounds.width, y: bounds.y + offset * bounds.height};
    case 'top': return {x: bounds.x + offset * bounds.width, y: bounds.y};
    default: return {x: bounds.x + offset * bounds.width, y: bounds.y + bounds.height};
  }
}
