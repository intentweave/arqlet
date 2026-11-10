// Experimental (global layering, first view within a node budget): a collapsed container's edges are drawn once per visible
// pair and direction (the engine's `view::merge_aggregated`). The drawn edge stands for all of them: its weight is their
// number and its members are the others, so the viewer draws it as a bundle with its count and lists them.
export function withAggregatedWeights(scene, snapshot) {
 const source = snapshot?.sourceGraph, collapsed = new Set(snapshot?.collapsedIds ?? []);
 // Only a view within a node budget merges the edges; otherwise each one has its own route.
 if (!source || !collapsed.size || !(snapshot.options?.visibleNodeBudget > 0)) return scene;
 const parent = new Map(source.nodes.map(n => [n.id, n.parentId ?? null]));
 // The node an id is drawn as: its outermost collapsed ancestor, or itself.
 const owner = id => { let shown = id; for (let at = parent.get(id); at; at = parent.get(at)) if (collapsed.has(at)) shown = at; return shown; };
 const groups = new Map();
 for (const e of source.edges) {
  if (e.target == null) continue;
  const s = owner(e.source), t = owner(e.target);
  if (s === e.source && t === e.target) continue;
  const key = `${s}\u0000${t}`;
  if (!groups.has(key)) groups.set(key, []);
  groups.get(key).push(e.id);
 }
 const byEdge = new Map();
 for (const ids of groups.values()) if (ids.length > 1) for (const id of ids) byEdge.set(id, ids);
 if (!byEdge.size) return scene;
 return {...scene, routes: scene.routes.map(r => {
  const ids = byEdge.get(r.id);
  return ids ? {...r, weight: ids.length, members: ids.filter(id => id !== r.id)} : r;
 })};
}
