// Shared harness code: Arqlet invocation, normalization to a common geometry format, and the
// metric implementation used for every engine. Imported by compare.mjs and grid.mjs.
import {spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import {existsSync} from 'node:fs';

export const ROOT = fileURLToPath(new URL('../../', import.meta.url));
// `LAYOUT_COMMAND` in the environment picks another build (for quick checks during development; measurements use release).
export const LAYOUT_COMMAND = process.env.LAYOUT_COMMAND ?? `${ROOT}target/release/layout-command`;
export const LEAF = {width: 180, height: 54};
const EPS = 1e-9;

if (!existsSync(LAYOUT_COMMAND)) {
  throw new Error(`Missing ${LAYOUT_COMMAND}. Run: cargo build --release -p arqlet-runtime --bins`);
}

// ---------- Arqlet ----------
export function arqlet(request) {
  const started = performance.now();
  const result = spawnSync(LAYOUT_COMMAND, [], {input: JSON.stringify(request), encoding: 'utf8', maxBuffer: 256 * 1024 * 1024});
  if (result.status !== 0) throw new Error(result.stderr);
  const response = JSON.parse(result.stdout);
  if (!response.ok) throw new Error(response.error);
  return {snapshot: response.snapshot, ms: performance.now() - started};
}


export function normalizeArqlet(snapshot) {
  const graph = snapshot.graph;
  const parents = new Map(graph.nodes.map(n => [n.id, n.parentId]));
  const containers = new Set(graph.nodes.map(n => n.parentId).filter(Boolean));
  const absolute = id => {
    const r = {...snapshot.boxes[id]};
    for (let p = parents.get(id); p; p = parents.get(p)) {
      r.x += snapshot.boxes[p].x;
      r.y += snapshot.boxes[p].y;
    }
    return r;
  };
  const routes = new Map(snapshot.routes.map(r => [r.id, r.points]));
  // Explicit port identities: only an explicit port shared by two edges can be a shared stub.
  const ports = new Map(snapshot.resolvedPorts.map(p => [`${p.edgeId}:${p.endpoint}`, p.id]));
  const memberIds = new Set(snapshot.routes.filter(r => r.bundle_of).map(r => r.id));
  return {
    nodes: graph.nodes.map(n => {
      const r = absolute(n.id);
      return {id: n.id, parentId: parents.get(n.id) ?? null, container: containers.has(n.id), x: r.x, y: r.y, w: r.width, h: r.height};
    }),
    // Bundle members share their leader's route and are not drawn separately.
    edges: graph.edges.filter(e => e.target !== null && !memberIds.has(e.id)).map(e => ({
      id: e.id, source: e.source, target: e.target, points: routes.get(e.id) ?? [],
      sourcePort: ports.get(`${e.id}:source`) ?? null, targetPort: ports.get(`${e.id}:target`) ?? null,
    })),
  };
}


// ---------- Metrics (one implementation for every engine) ----------
// The metric set lives in crates/scene/src/metrics.rs and is reached through the `metrics` operation,
// so the Arqlet output and every other engine's output are scored by the same code. All metrics
// are computed on every call; none is skipped for an engine.
export function metrics(out) {
  const drawing = {
    nodes: out.nodes.map(n => ({id: n.id, parentId: n.parentId ?? null, container: n.container, x: n.x, y: n.y, width: n.w, height: n.h})),
    edges: out.edges.map(e => ({id: e.id, source: e.source, target: e.target, points: e.points, sourcePort: e.sourcePort ?? null, targetPort: e.targetPort ?? null})),
    // Explicit buses (their members may share lines), when the drawing has them.
    ...(out.buses?.length ? {buses: out.buses} : {}),
  };
  const result = spawnSync(LAYOUT_COMMAND, [], {input: JSON.stringify({op: 'metrics', drawing}), encoding: 'utf8', maxBuffer: 256 * 1024 * 1024});
  if (result.status !== 0) throw new Error(result.stderr);
  const response = JSON.parse(result.stdout);
  if (!response.ok) throw new Error(response.error);
  return response.metrics;
}
export const round = v => Math.round(v * 100) / 100;
