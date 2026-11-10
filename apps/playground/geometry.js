// The geometry the page reports to the engine: for each element the outline it drew and the revision of that measurement.
// The page owns colours, borders, icons and content; the engine reads only the outline (`Contour`) and uses the revision to
// recognise a measurement that arrives late.

/** The outline of a node drawn in `shape` with the box `size`: a circle by its diameter, anything else as a rectangle. */
export function contourOf(shape, size) {
  if (shape === 'circle') return { kind: 'circle', diameter: Math.max(size.width, size.height) };
  return { kind: 'rect', width: size.width, height: size.height };
}

/** Numbers the measurements of each element: every report of an element has a higher revision than the one before. */
export class GeometryReports {
  #revisions = new Map();

  /** One measurement of `id` with the next revision. */
  report(id, shape, size) {
    const revision = (this.#revisions.get(id) ?? 0) + 1;
    this.#revisions.set(id, revision);
    return { contour: contourOf(shape, size), geometryRevision: revision };
  }

  /** The measurements for several elements: `sizes` maps an id to its drawn box size, `shapeOf` gives its shape. */
  reports(sizes, shapeOf) {
    return Object.fromEntries(Object.entries(sizes).map(([id, size]) => [id, this.report(id, shapeOf(id), size)]));
  }

  /** Forgets the revisions, e.g. when a new graph is imported and the engine starts again. */
  clear() {
    this.#revisions.clear();
  }
}

/**
 * The box a node needs for its label, from the measured text width. A rectangle is as wide as the text plus padding and at
 * least 180 x 54. A circle must hold the label's text box inside the disc, so its diameter is the diagonal of that box
 * (text width plus padding by one line), and at least 56.
 */
export function labelSize(shape, textWidth) {
  if (shape === 'circle') {
    const diameter = Math.max(56, Math.ceil(Math.hypot(textWidth + 16, 22)));
    return { width: diameter, height: diameter };
  }
  return { width: Math.max(180, Math.ceil(textWidth) + 28), height: 54 };
}
