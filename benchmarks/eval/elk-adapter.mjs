// Shared evaluation adapter; engine implementations remain external dependencies.
import {LEAF} from "./lib.mjs";

// With `containerDirections`, a container with a fixed lr or tb direction of its own gets it as `elk.direction`; when any
// differs from the root's, the containers are laid out separately (`SEPARATE_CHILDREN`), which is how ELK takes mixed
// directions. Without it, one direction at the root and `INCLUDE_CHILDREN` (the uniform task).
const DIRECTION = {lr: 'RIGHT', tb: 'DOWN', hybrid: 'RIGHT'};
export async function elkLayout(view, engine, {containerDirections = false} = {}) {
  const totalStarted = performance.now();
  const containers = new Set(view.nodes.map(n => n.parentId).filter(Boolean));
  const own = id => containerDirections ? {lr: 'RIGHT', tb: 'DOWN'}[view.containerDirections?.[id]] : undefined;
  const mixed = containerDirections && [...containers].some(id => own(id) && own(id) !== DIRECTION[view.defaultDirection]);
  const elkNodes = new Map();
  for (const n of view.nodes) {
    const layoutOptions = own(n.id) ? {'elk.direction': own(n.id)} : {};
    elkNodes.set(n.id, containers.has(n.id) ? {id: n.id, children: [], layoutOptions} : {id: n.id, ...LEAF});
  }
  const root = {id: 'root', children: [], edges: []};
  for (const n of view.nodes) {
    (n.parentId ? elkNodes.get(n.parentId).children : root.children).push(elkNodes.get(n.id));
  }
  root.edges = view.edges.filter(e => e.target !== null).map(e => ({id: e.id, sources: [e.source], targets: [e.target]}));
  root.layoutOptions = {
    'elk.algorithm': 'layered',
    'elk.direction': DIRECTION[view.defaultDirection],
    'elk.edgeRouting': 'ORTHOGONAL',
    'elk.hierarchyHandling': mixed ? 'SEPARATE_CHILDREN' : 'INCLUDE_CHILDREN',
  };
  const prepareMs = performance.now() - totalStarted;
  const started = performance.now();
  const laid = await engine.layout(root);
  const ms = performance.now() - started;
  const normalizeStarted = performance.now();
  const nodes = [];
  const origin = new Map([['root', {x: 0, y: 0}]]);
  const parent = new Map();
  const walk = (node, ox, oy) => {
    const x = ox + (node.x ?? 0), y = oy + (node.y ?? 0);
    if (node.id !== 'root') {
      const up = parent.get(node.id);
      nodes.push({id: node.id, parentId: up && up !== 'root' ? up : null, container: containers.has(node.id), x, y, w: node.width, h: node.height});
      origin.set(node.id, {x, y});
    }
    for (const child of node.children ?? []) {
      parent.set(child.id, node.id);
      walk(child, x, y);
    }
  };
  walk(laid, 0, 0);
  // ELK names the section's coordinate frame; a self-loop's frame is its parent, not the leaf.
  const ancestors = id => { const chain = []; for (let c = id; c; c = parent.get(c)) chain.push(c); return chain; };
  const edges = [];
  const collect = node => {
    for (const e of node.edges ?? []) edges.push(e);
    for (const child of node.children ?? []) collect(child);
  };
  collect(laid);
  const converted = edges.map(e => {
    if (e.sections?.length !== 1) throw new Error(`ELK edge ${e.id}: expected exactly one section`);
    const chainT = ancestors(e.targets[0]);
    const common = ancestors(e.sources[0]).find(c => chainT.includes(c)) ?? 'root';
    const frame = e.container ?? (containers.has(common) || common === 'root' ? common : parent.get(common));
    const o = origin.get(frame);
    if (!o) throw new Error(`ELK edge ${e.id}: unknown coordinate frame ${frame}`);
    const s = e.sections[0];
    const points = [s.startPoint, ...(s.bendPoints ?? []), s.endPoint];
    return {id: e.id, source: e.sources[0], target: e.targets[0], points: points.map(p => ({x: o.x + p.x, y: o.y + p.y}))};
  });
  const normalizeMs = performance.now() - normalizeStarted;
  return {nodes, edges: converted, ms, prepareMs, normalizeMs, totalMs: performance.now() - totalStarted};
}
