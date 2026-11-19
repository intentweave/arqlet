// Provisional: the overview built in steps. A rough overview of the top levels of the containment hierarchy is laid out
// first and shown at once; when the whole overview is done, every node starts at the place of its rough ancestor and
// moves to its own (the clusters bloom out of the rough picture). Pure functions; the page calls the engine.

/**
 * The top levels of `graph`'s containment, at most `limit` nodes: the nodes down to the deepest depth that keeps within
 * the limit, a cut container standing for what it holds (a radius for its leaves, `radii`), and the edges lifted to the
 * nodes that stand for their ends (one per pair and direction; edges inside one cut container are left out). `null` when
 * the graph has no containment to cut or already fits.
 */
export function coarseGraph(graph, {limit = 600, radius = 4} = {}) {
  if (graph.nodes.length <= limit) return null;
  const parent = new Map(graph.nodes.map(n => [n.id, n.parentId ?? null]));
  const depth = new Map();
  const depthOf = id => {
    if (depth.has(id)) return depth.get(id);
    const chain = [];
    let at = id, base = 0;
    while (at != null && !depth.has(at)) { chain.push(at); at = parent.get(at) ?? null; }
    if (at != null) base = depth.get(at) + 1;
    for (let k = chain.length - 1; k >= 0; k--) depth.set(chain[k], base + (chain.length - 1 - k));
    return depth.get(id);
  };
  const perDepth = [];
  for (const n of graph.nodes) { const d = depthOf(n.id); perDepth[d] = (perDepth[d] ?? 0) + 1; }
  // The deepest depth whose nodes, with all above, stay within the limit.
  let cut = -1, total = 0;
  for (let d = 0; d < perDepth.length; d++) {
    total += perDepth[d] ?? 0;
    if (total > limit) break;
    cut = d;
  }
  if (cut < 0 || cut === perDepth.length - 1) return null;
  // What stands for each node: itself down to the cut, else its ancestor at the cut.
  const standsFor = id => {
    let at = id;
    while (depth.get(at) > cut) at = parent.get(at);
    return at;
  };
  const leaves = new Map();
  const hasChildren = new Set(graph.nodes.map(n => n.parentId).filter(p => p != null));
  for (const n of graph.nodes) {
    if (hasChildren.has(n.id)) continue;
    const s = standsFor(n.id);
    leaves.set(s, (leaves.get(s) ?? 0) + 1);
  }
  const nodes = graph.nodes.filter(n => depth.get(n.id) <= cut);
  const radii = {};
  for (const n of nodes) {
    if (depth.get(n.id) !== cut || !hasChildren.has(n.id)) continue;
    // A disc holding as many leaves as the cut container, a little looser than packed.
    radii[n.id] = radius * Math.sqrt(leaves.get(n.id) ?? 1) * 1.3;
  }
  // One edge per pair and direction, weighted by the edges it stands for; only the strongest (at most two per node), so
  // that the rough picture shows its main lines and not a mesh.
  const pairs = new Map();
  for (const e of graph.edges) {
    if (e.target == null || !parent.has(e.source) || !parent.has(e.target)) continue;
    const a = standsFor(e.source), b = standsFor(e.target);
    if (a === b) continue;
    const key = `${a}\u0000${b}`;
    const pair = pairs.get(key) ?? {source: a, target: b, kind: e.kind, weight: 0, order: pairs.size};
    pair.weight++;
    pairs.set(key, pair);
  }
  const edges = [...pairs.values()]
    .sort((p, q) => q.weight - p.weight || p.order - q.order)
    .slice(0, 2 * nodes.length)
    .sort((p, q) => p.order - q.order)
    .map((p, k) => ({id: `rough:${k}`, kind: p.kind, source: p.source, target: p.target, weight: p.weight}));
  // The cut containers are leaves of the rough graph; their parents stay containers.
  return {graph: {...graph, nodes: nodes.map(n => ({...n})), edges}, radii, cut};
}

/**
 * Where the nodes of the whole overview `full` start blooming: at the place of their nearest ancestor (or themselves)
 * that the rough overview `rough` has, small; a node with none such starts at its own place. Same arrays as `full`.
 */
export function bloomStart(full, rough) {
  const index = new Map(rough.ids.map((id, i) => [id, i]));
  const n = full.ids.length;
  const x = new Array(n), y = new Array(n), r = new Array(n);
  for (let i = 0; i < n; i++) {
    let at = i;
    while (at >= 0 && !index.has(full.ids[at])) at = full.parent[at];
    if (at < 0) { x[i] = full.x[i]; y[i] = full.y[i]; r[i] = full.r[i]; continue; }
    const j = index.get(full.ids[at]);
    x[i] = rough.x[j]; y[i] = rough.y[j];
    // The node the rough overview has keeps its rough disc; what it holds starts as a point inside it.
    r[i] = at === i ? rough.r[j] : Math.min(full.r[i], rough.r[j]) * 0.25;
  }
  return {...full, x, y, r};
}
