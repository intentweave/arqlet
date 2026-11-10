/**
 * Pointer gestures for a rendered curation scene. A gesture previews by moving the SVG element only; the document
 * changes once, on release, so one gesture is one undo step. A press without movement changes nothing.
 */
import type {Point, Rect, Scene} from '@arqlet/contracts';

const SVG_NS = 'http://www.w3.org/2000/svg';
import {movedWithin, nearestShiftableSegment, outwardSide, resized} from './geometry.js';
import type {CurationSession, EngineReply} from './session.js';
import {LivePreview} from './preview.js';

export interface GestureHooks {
  /** The document changed: the caller re-renders the scene. */
  changed(): void;
  /** A short message for the status line. */
  status(text: string): void;
  /** Whether a dragged node may grow the containers it leaves. */
  growContainers(): boolean;
  /** The nodes the curator has selected. A drag of one of them moves the whole selection. */
  selectedNodes?(): string[];
}

/** Binds drags on nodes and on route segments. Call again after every render, because rendering replaces the SVG. */
export function bindCurationGestures(
  svg: SVGSVGElement,
  session: CurationSession,
  scene: Scene,
  hooks: GestureHooks,
): void {
  for (const group of svg.querySelectorAll<SVGGElement>('g.node')) {
    const nodeId = group.querySelector('title')?.textContent;
    const body = group.querySelector('rect');
    const node = scene.nodes.find(item => item.id === nodeId);
    if (!nodeId || !body || !node) continue;
    body.style.cursor = 'move';
    if (session.document.curated.sizePins?.[nodeId]) body.setAttribute('stroke-dasharray', '3 3');
    if (hooks.selectedNodes?.().includes(nodeId)) body.setAttribute('stroke', '#f6c177');
    body.addEventListener('pointerdown', event => {
      if (event.shiftKey) {
        event.stopPropagation();
        void togglePin(session, nodeId, hooks);
        return;
      }
      // The bottom-right corner resizes the leaf; the rest of the box moves it.
      if (nearCorner(scenePoint(svg, event), node.bounds)) startResize(event, svg, body, session, scene, nodeId, hooks, false);
      else startNodeDrag(event, svg, group, session, scene, nodeId, hooks);
    });
  }
  for (const body of svg.querySelectorAll<SVGRectElement>('rect[data-container-id]')) {
    const containerId = body.getAttribute('data-container-id');
    const container = scene.nodes.find(item => item.id === containerId);
    if (!containerId || !container) continue;
    if (session.document.curated.sizePins?.[containerId]) body.setAttribute('stroke-dasharray', '3 3');
    body.addEventListener('pointerdown', event => {
      if (event.shiftKey) {
        event.stopPropagation();
        void togglePin(session, containerId, hooks);
        return;
      }
      if (nearCorner(scenePoint(svg, event), container.bounds)) startResize(event, svg, body, session, scene, containerId, hooks, true);
    });
  }
  for (const title of svg.querySelectorAll<SVGTextElement>('text[data-container-id]')) {
    const containerId = title.getAttribute('data-container-id');
    if (!containerId || !scene.nodes.some(item => item.id === containerId)) continue;
    title.style.cursor = 'move';
    title.addEventListener('pointerdown', event => startContainerDrag(event, svg, session, scene, containerId, hooks));
  }
  for (const dot of svg.querySelectorAll<SVGCircleElement>('circle[data-port-id]')) {
    const portId = dot.getAttribute('data-port-id');
    if (!portId) continue;
    dot.style.pointerEvents = 'all';
    dot.style.cursor = 'move';
    dot.addEventListener('pointerdown', event => startPortDrag(event, svg, dot, session, scene, portId, hooks));
  }
  for (const label of svg.querySelectorAll<SVGRectElement>('rect[data-label-id]')) {
    const labelId = label.getAttribute('data-label-id');
    if (!labelId || !scene.labels.some(item => item.id === labelId)) continue;
    label.style.pointerEvents = 'all';
    label.style.cursor = 'move';
    label.addEventListener('pointerdown', event => startLabelDrag(event, svg, label, session, scene, labelId, hooks));
  }
  for (const hit of svg.querySelectorAll<SVGPolylineElement>('polyline.route-hit')) {
    const routeId = hit.getAttribute('data-route-id');
    if (!routeId) continue;
    hit.addEventListener('pointerdown', event => startSegmentDrag(event, svg, hit, session, scene, routeId, hooks));
  }
  for (const line of svg.querySelectorAll<SVGPolylineElement>('polyline[data-route-id]')) {
    const routeId = line.getAttribute('data-route-id');
    if (!line.classList.contains('route-hit') && routeId && session.document.curated.lockedRoutes?.includes(routeId)) {
      line.setAttribute('stroke-dasharray', '2 4');
    }
  }
}

function startNodeDrag(
  event: PointerEvent,
  svg: SVGSVGElement,
  group: SVGGElement,
  session: CurationSession,
  scene: Scene,
  nodeId: string,
  hooks: GestureHooks,
): void {
  if (event.button !== 0) return;
  event.stopPropagation();
  const node = scene.nodes.find(item => item.id === nodeId);
  if (!node) return;
  const containers = containersOf(scene, node);
  const boxes = containers.map(item => item.bounds);
  const growth = new Map<string, SVGRectElement>();
  const start = scenePoint(svg, event);
  const target = event.currentTarget as SVGElement;
  target.setPointerCapture(event.pointerId);
  let offset = {dx: 0, dy: 0};
  // A drag of a selected node moves the whole selection; the preview follows the same set.
  const selectedIds = hooks.selectedNodes?.() ?? [];
  const movedIds = selectedIds.includes(nodeId) && selectedIds.length > 1 ? selectedIds : [nodeId];
  const boxesOf = (id: string): Rect[] => {
    const item = scene.nodes.find(other => other.id === id);
    return item ? containersOf(scene, item).map(container => container.bounds) : [];
  };
  const live = new LivePreview(session, svg, movedIds, boxesOf, hooks.growContainers());
  const ghost = svg.ownerDocument.createElementNS(SVG_NS, 'rect');
  ghost.setAttribute('fill', 'none');
  ghost.setAttribute('stroke-dasharray', '6 4');
  ghost.setAttribute('stroke-width', '2');
  ghost.setAttribute('rx', '7');
  ghost.setAttribute('pointer-events', 'none');
  const move = (moveEvent: PointerEvent) => {
    const here = scenePoint(svg, moveEvent);
    const {rect, grown, clamped} = movedWithin(
      node.bounds,
      here.x - start.x,
      here.y - start.y,
      boxes,
      hooks.growContainers(),
    );
    offset = {dx: rect.x - node.bounds.x, dy: rect.y - node.bounds.y};
    group.setAttribute('transform', `translate(${offset.dx},${offset.dy})`);
    live.update(offset);
    // The ghost shows where the node will be. Red means the container stops it at its border.
    ghost.setAttribute('x', String(rect.x));
    ghost.setAttribute('y', String(rect.y));
    ghost.setAttribute('width', String(rect.width));
    ghost.setAttribute('height', String(rect.height));
    ghost.setAttribute('stroke', clamped ? '#f08f80' : '#90e4c3');
    if (!ghost.isConnected) svg.append(ghost);
    // A container that would grow is outlined in amber, at the size it would have.
    containers.forEach((item, index) => {
      const box = grown[index]!;
      const outline = growth.get(item.id);
      if (!resized(item.bounds, box)) {
        outline?.remove();
        return;
      }
      const element = outline ?? svg.ownerDocument.createElementNS(SVG_NS, 'rect');
      if (!outline) {
        element.setAttribute('fill', 'none');
        element.setAttribute('stroke-dasharray', '6 4');
        element.setAttribute('stroke-width', '2');
        element.setAttribute('pointer-events', 'none');
        element.setAttribute('stroke', '#f6c177');
        growth.set(item.id, element);
      }
      element.setAttribute('x', String(box.x));
      element.setAttribute('y', String(box.y));
      element.setAttribute('width', String(box.width));
      element.setAttribute('height', String(box.height));
      if (!element.isConnected) svg.append(element);
    });
  };
  const up = async () => {
    target.removeEventListener('pointermove', move);
    target.removeEventListener('pointerup', up);
    target.removeEventListener('pointercancel', up);
    ghost.remove();
    for (const outline of growth.values()) outline.remove();
    group.removeAttribute('transform');
    // The preview is cleared by the redraw that follows: a commit or a refusal both redraw the scene.
    const drawn = live.end();
    if (offset.dx === 0 && offset.dy === 0) {
      if (drawn) hooks.changed();
      return;
    }
    const reply = await session.moveNodes(movedIds, offset.dx, offset.dy, hooks.growContainers());
    report(reply, hooks, movedIds.length > 1 ? 'Selection moved' : 'Node moved');
    if (!reply.ok || drawn) hooks.changed();
  };
  target.addEventListener('pointermove', move);
  target.addEventListener('pointerup', up);
  target.addEventListener('pointercancel', up);
}

/** The size of the handle at a box's bottom-right corner, in scene units. */
const RESIZE_CORNER = 14;

function nearCorner(point: Point, box: Rect): boolean {
  return point.x >= box.x + box.width - RESIZE_CORNER && point.y >= box.y + box.height - RESIZE_CORNER;
}

/**
 * Resizes a leaf or a container from its bottom-right corner. The ghost shows the requested size; the session clamps
 * it to the minimum (a leaf) or to the boxes inside (a container) and reports the result.
 */
function startResize(
  event: PointerEvent,
  svg: SVGSVGElement,
  target: SVGElement,
  session: CurationSession,
  scene: Scene,
  nodeId: string,
  hooks: GestureHooks,
  container: boolean,
): void {
  if (event.button !== 0) return;
  event.stopPropagation();
  const node = scene.nodes.find(item => item.id === nodeId);
  if (!node) return;
  const start = scenePoint(svg, event);
  target.setPointerCapture(event.pointerId);
  let size = {width: node.bounds.width, height: node.bounds.height};
  const ghost = svg.ownerDocument.createElementNS(SVG_NS, 'rect');
  ghost.setAttribute('x', String(node.bounds.x));
  ghost.setAttribute('y', String(node.bounds.y));
  ghost.setAttribute('fill', 'none');
  ghost.setAttribute('stroke', '#90e4c3');
  ghost.setAttribute('stroke-dasharray', '6 4');
  ghost.setAttribute('stroke-width', '2');
  ghost.setAttribute('pointer-events', 'none');
  const move = (moveEvent: PointerEvent) => {
    const here = scenePoint(svg, moveEvent);
    size = {
      width: Math.max(1, node.bounds.width + here.x - start.x),
      height: Math.max(1, node.bounds.height + here.y - start.y),
    };
    ghost.setAttribute('width', String(size.width));
    ghost.setAttribute('height', String(size.height));
    if (!ghost.isConnected) svg.append(ghost);
  };
  const up = async () => {
    target.removeEventListener('pointermove', move);
    target.removeEventListener('pointerup', up);
    target.removeEventListener('pointercancel', up);
    ghost.remove();
    if (size.width === node.bounds.width && size.height === node.bounds.height) return;
    const reply = container
      ? await session.resizeContainer(nodeId, size.width, size.height)
      : await session.resizeNode(nodeId, size.width, size.height);
    report(reply, hooks, 'Size changed');
  };
  target.addEventListener('pointermove', move);
  target.addEventListener('pointerup', up);
  target.addEventListener('pointercancel', up);
}

/** The containers of a scene node, from its direct parent upwards, with their drawn boxes. */
function containersOf(scene: Scene, node: {parentId?: string | null}): {id: string; bounds: Rect}[] {
  const containers: {id: string; bounds: Rect}[] = [];
  for (let parentId = node.parentId ?? null; parentId; ) {
    const item = scene.nodes.find(other => other.id === parentId);
    if (!item) break;
    containers.push({id: item.id, bounds: item.bounds});
    parentId = item.parentId ?? null;
  }
  return containers;
}

/**
 * A container and everything in it move together. The ghost shows the container's new box, outlined red where its
 * parent stops it; release applies the move through the session.
 */
function startContainerDrag(
  event: PointerEvent,
  svg: SVGSVGElement,
  session: CurationSession,
  scene: Scene,
  containerId: string,
  hooks: GestureHooks,
): void {
  if (event.button !== 0) return;
  event.stopPropagation();
  const container = scene.nodes.find(item => item.id === containerId);
  if (!container) return;
  const boxes = containersOf(scene, container).map(item => item.bounds);
  const start = scenePoint(svg, event);
  const target = event.currentTarget as SVGElement;
  target.setPointerCapture(event.pointerId);
  let offset = {dx: 0, dy: 0};
  const ghost = svg.ownerDocument.createElementNS(SVG_NS, 'rect');
  ghost.setAttribute('fill', 'none');
  ghost.setAttribute('stroke-dasharray', '6 4');
  ghost.setAttribute('stroke-width', '2');
  ghost.setAttribute('rx', '10');
  ghost.setAttribute('pointer-events', 'none');
  const move = (moveEvent: PointerEvent) => {
    const here = scenePoint(svg, moveEvent);
    const {rect, clamped} = movedWithin(
      container.bounds,
      here.x - start.x,
      here.y - start.y,
      boxes,
      hooks.growContainers(),
    );
    offset = {dx: rect.x - container.bounds.x, dy: rect.y - container.bounds.y};
    ghost.setAttribute('x', String(rect.x));
    ghost.setAttribute('y', String(rect.y));
    ghost.setAttribute('width', String(rect.width));
    ghost.setAttribute('height', String(rect.height));
    ghost.setAttribute('stroke', clamped ? '#f08f80' : '#90e4c3');
    if (!ghost.isConnected) svg.append(ghost);
  };
  const up = async () => {
    target.removeEventListener('pointermove', move);
    target.removeEventListener('pointerup', up);
    target.removeEventListener('pointercancel', up);
    ghost.remove();
    if (offset.dx === 0 && offset.dy === 0) return;
    report(await session.moveContainer(containerId, offset.dx, offset.dy, hooks.growContainers()), hooks, 'Container moved');
  };
  target.addEventListener('pointermove', move);
  target.addEventListener('pointerup', up);
  target.addEventListener('pointercancel', up);
}

/** Locks or unlocks a route; the status line names the result. */
async function toggleLock(session: CurationSession, routeId: string, hooks: GestureHooks): Promise<void> {
  const locked = session.document.curated.lockedRoutes?.includes(routeId) ?? false;
  report(await session.lockRoute(routeId, !locked), hooks, locked ? 'Route unlocked' : 'Route locked');
}

/** Pins or unpins the current size of a node. */
async function togglePin(session: CurationSession, nodeId: string, hooks: GestureHooks): Promise<void> {
  const pinned = Boolean(session.document.curated.sizePins?.[nodeId]);
  report(await session.toggleSizePin(nodeId), hooks, pinned ? 'Size unpinned' : 'Size pinned');
}

/** The jog of a dogleg added by Alt-click, in scene units. */
const DOGLEG_DEPTH = 36;

/** A label box moves with its text; release saves the new position through the session. */
function startLabelDrag(
  event: PointerEvent,
  svg: SVGSVGElement,
  box: SVGRectElement,
  session: CurationSession,
  scene: Scene,
  labelId: string,
  hooks: GestureHooks,
): void {
  if (event.button !== 0) return;
  event.stopPropagation();
  if (!scene.labels.some(item => item.id === labelId)) return;
  const start = scenePoint(svg, event);
  box.setPointerCapture(event.pointerId);
  const text = box.nextElementSibling;
  let offset = {dx: 0, dy: 0};
  const move = (moveEvent: PointerEvent) => {
    const here = scenePoint(svg, moveEvent);
    offset = {dx: here.x - start.x, dy: here.y - start.y};
    const transform = `translate(${offset.dx},${offset.dy})`;
    box.setAttribute('transform', transform);
    text?.setAttribute('transform', transform);
  };
  const up = async () => {
    box.removeEventListener('pointermove', move);
    box.removeEventListener('pointerup', up);
    box.removeEventListener('pointercancel', up);
    box.removeAttribute('transform');
    text?.removeAttribute('transform');
    if (Math.abs(offset.dx) < 1 && Math.abs(offset.dy) < 1) return;
    report(await session.moveLabel(labelId, offset.dx, offset.dy), hooks, 'Label moved');
  };
  box.addEventListener('pointermove', move);
  box.addEventListener('pointerup', up);
  box.addEventListener('pointercancel', up);
}

function startSegmentDrag(
  event: PointerEvent,
  svg: SVGSVGElement,
  hit: SVGPolylineElement,
  session: CurationSession,
  scene: Scene,
  routeId: string,
  hooks: GestureHooks,
): void {
  if (event.button !== 0) return;
  event.stopPropagation();
  if (event.shiftKey) {
    void toggleLock(session, routeId, hooks);
    return;
  }
  const route = scene.routes.find(item => item.id === routeId);
  if (!route) return;
  const start = scenePoint(svg, event);
  const segment = nearestShiftableSegment(route.points, start);
  if (!segment) {
    hooks.status('Only inner horizontal or vertical segments can be moved.');
    return;
  }
  hit.setPointerCapture(event.pointerId);
  const up = async (upEvent: PointerEvent) => {
    hit.removeEventListener('pointerup', up);
    hit.removeEventListener('pointercancel', up);
    const here = scenePoint(svg, upEvent);
    const delta = segment.axis === 'vertical' ? here.x - start.x : here.y - start.y;
    // Alt-click adds a dogleg on the side the pointer is on; a drag moves the segment as before.
    if (upEvent.altKey && Math.abs(delta) < 1) {
      report(await session.addDogleg(routeId, segment.index, delta < 0 ? -DOGLEG_DEPTH : DOGLEG_DEPTH), hooks, 'Dogleg added');
      return;
    }
    if (Math.abs(delta) < 1) return;
    report(await session.shiftSegment(routeId, segment.index, delta), hooks, 'Segment moved');
  };
  hit.addEventListener('pointerup', up);
  hit.addEventListener('pointercancel', up);
}

/** A port slides along its side. The offset changes on release; the gesture only previews the marker. */
function startPortDrag(
  event: PointerEvent,
  svg: SVGSVGElement,
  dot: SVGCircleElement,
  session: CurationSession,
  scene: Scene,
  portId: string,
  hooks: GestureHooks,
): void {
  if (event.button !== 0) return;
  event.stopPropagation();
  const port = scene.ports.find(item => item.id === portId);
  if (!port) return;
  const node = scene.nodes.find(item => item.id === port.nodeId);
  if (!node) return;
  const horizontal = port.side === 'top' || port.side === 'bottom';
  const start = scenePoint(svg, event);
  dot.setPointerCapture(event.pointerId);
  let along = 0;
  let last = start;
  const label = svg.ownerDocument.createElementNS(SVG_NS, 'text');
  label.setAttribute('font-size', '12');
  label.setAttribute('fill', '#edf5ff');
  label.setAttribute('pointer-events', 'none');
  const move = (moveEvent: PointerEvent) => {
    const here = scenePoint(svg, moveEvent);
    last = here;
    along = horizontal ? here.x - start.x : here.y - start.y;
    dot.setAttribute('transform', horizontal ? `translate(${along},0)` : `translate(0,${along})`);
    // The label shows the offset the port would get on release, as a share of its side.
    const length = horizontal ? node.bounds.width : node.bounds.height;
    const share = Math.round(Math.min(1, Math.max(0, (session.document.curated.ports[portId]?.offset ?? 0) + along / length)) * 100);
    label.textContent = `${share} %`;
    label.setAttribute('x', String(port.position.x + 8));
    label.setAttribute('y', String(port.position.y - 8));
    if (!label.isConnected) svg.append(label);
  };
  const up = async () => {
    dot.removeEventListener('pointermove', move);
    dot.removeEventListener('pointerup', up);
    dot.removeEventListener('pointercancel', up);
    label.remove();
    dot.removeAttribute('transform');
    // A port dragged clearly past its node moves to that side; otherwise it slides along its own side.
    const side = outwardSide(last, node.bounds, port.side);
    if (side) {
      report(await session.movePortSide(portId, side), hooks, 'Port moved to another side');
      return;
    }
    if (Math.abs(along) < 1) return;
    report(await session.movePort(portId, along), hooks, 'Port moved');
  };
  dot.addEventListener('pointermove', move);
  dot.addEventListener('pointerup', up);
  dot.addEventListener('pointercancel', up);
}

function report(reply: EngineReply, hooks: GestureHooks, done: string): void {
  if (!reply.ok) {
    hooks.status(reply.error ?? 'The change was rejected.');
    return;
  }
  const pending = reply.report?.pendingRoutes ?? 0;
  const hard = reply.report?.hardViolations ?? 0;
  const note = reply.notice ? ` · ${reply.notice}` : '';
  hooks.status(`${done} · ${pending} routes pending · ${hard} hard violations${note}`);
  hooks.changed();
}

/** A pointer position in scene coordinates, through the SVG's own transform. */
export function scenePoint(svg: SVGSVGElement, event: {clientX: number; clientY: number}): Point {
  const matrix = svg.getScreenCTM();
  if (!matrix) return {x: event.clientX, y: event.clientY};
  const point = svg.createSVGPoint();
  point.x = event.clientX;
  point.y = event.clientY;
  const mapped = point.matrixTransform(matrix.inverse());
  return {x: mapped.x, y: mapped.y};
}
