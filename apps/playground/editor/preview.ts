/**
 * The live preview of a node drag, in two tiers:
 * - Tier 1 is computed in the browser on every pointer move: each route that touches a moved node gets a simple
 *   orthogonal shape between its two ports. It ignores obstacles, so it is a preview only.
 * - Tier 2 asks the engine for the real routes of the moved position, one request at a time. Only the newest position
 *   counts: a result for an older position is dropped. When the drag ends, the preview is cleared and the change is
 *   committed (or refused), which redraws the scene.
 * Nothing here changes the document.
 */
import type {Point, PortRecord, PortSide, Rect} from '@arqlet/contracts';
import {movedWithin} from './geometry.js';
import type {CurationSession} from './session.js';

/** The position of a port on its side, as the engine derives it. */
export function derivePosition(rect: Rect, side: PortSide, offset: number): Point {
  switch (side) {
    case 'left':
      return {x: rect.x, y: rect.y + rect.height * offset};
    case 'right':
      return {x: rect.x + rect.width, y: rect.y + rect.height * offset};
    case 'top':
      return {x: rect.x + rect.width * offset, y: rect.y};
    case 'bottom':
      return {x: rect.x + rect.width * offset, y: rect.y + rect.height};
  }
}

/** A simple orthogonal shape between two ports: a Z when both leave on the same axis, an L otherwise. */
export function previewRoute(a: Point, sideA: PortSide, b: Point, sideB: PortSide): Point[] {
  const vertical = (side: PortSide) => side === 'top' || side === 'bottom';
  if (vertical(sideA) && vertical(sideB)) {
    const y = (a.y + b.y) / 2;
    return [a, {x: a.x, y}, {x: b.x, y}, b];
  }
  if (!vertical(sideA) && !vertical(sideB)) {
    const x = (a.x + b.x) / 2;
    return [a, {x, y: a.y}, {x, y: b.y}, b];
  }
  return vertical(sideA) ? [a, {x: a.x, y: b.y}, b] : [a, {x: b.x, y: a.y}, b];
}

/** Draws a route's shape in the scene, dashed, on its visible line and its hit line. */
export function setRoutePreview(svg: SVGSVGElement, routeId: string, points: readonly Point[]): void {
  const value = points.map(point => `${point.x},${point.y}`).join(' ');
  for (const line of svg.querySelectorAll<SVGPolylineElement>('polyline[data-route-id]')) {
    if (line.getAttribute('data-route-id') !== routeId) continue;
    line.setAttribute('points', value);
    if (!line.classList.contains('route-hit')) line.setAttribute('stroke-dasharray', '4 3');
  }
}

export class LivePreview {
  /** The position the drag is at now, and a counter that rises with every move. */
  private offset = {dx: 0, dy: 0};
  private version = 0;
  private processed = 0;
  private busy = false;
  private ended = false;
  touched = false;

  constructor(
    private readonly session: CurationSession,
    private readonly svg: SVGSVGElement,
    private readonly ids: readonly string[],
    /** The drawn boxes of the containers of a node, from its direct parent upwards. */
    private readonly boxesOf: (id: string) => Rect[],
    private readonly grow: boolean,
  ) {}

  /** The pointer moved: draw the first tier at once and ask the engine for the second tier, if none is running. */
  update(offset: {dx: number; dy: number}): void {
    if (this.ended) return;
    this.offset = offset;
    this.version++;
    this.drawFirstTier();
    if (!this.busy) void this.refine();
  }

  /** The drag is over. Returns whether a preview was drawn, so the caller knows the scene must be redrawn. */
  end(): boolean {
    this.ended = true;
    return this.touched;
  }

  private moved(id: string): Rect | undefined {
    const box = this.session.document.curated.nodes[id];
    if (!box) return undefined;
    return movedWithin(box, this.offset.dx, this.offset.dy, this.boxesOf(id), this.grow).rect;
  }

  private drawFirstTier(): void {
    const curated = this.session.document.curated;
    const moved = new Map<string, Rect>();
    for (const id of this.ids) {
      const rect = this.moved(id);
      if (rect) moved.set(id, rect);
    }
    if (moved.size === 0) return;
    const routes = new Set(Object.values(curated.ports).filter(port => moved.has(port.nodeId)).map(port => port.edgeId));
    for (const routeId of routes) {
      const ports = Object.values(curated.ports);
      const source = ports.find(port => port.edgeId === routeId && port.endpoint === 'source');
      const target = ports.find(port => port.edgeId === routeId && port.endpoint === 'target');
      if (!source || !target) continue;
      const at = (port: PortRecord): Point => {
        const rect = moved.get(port.nodeId);
        return rect ? derivePosition(rect, port.side, port.offset) : port.position;
      };
      setRoutePreview(this.svg, routeId, previewRoute(at(source), source.side, at(target), target.side));
      this.touched = true;
    }
  }

  /** One engine request at a time; the newest position is always the next one asked for. */
  private async refine(): Promise<void> {
    this.busy = true;
    while (!this.ended && this.processed < this.version) {
      const version = this.version;
      const {dx, dy} = this.offset;
      const routes = await this.session.previewMoveNodes([...this.ids], dx, dy, this.grow);
      this.processed = version;
      if (!this.ended && routes && version === this.version) {
        for (const [routeId, points] of Object.entries(routes)) {
          if (points.length > 0) {
            setRoutePreview(this.svg, routeId, points);
            this.touched = true;
          }
        }
      }
    }
    this.busy = false;
  }
}
