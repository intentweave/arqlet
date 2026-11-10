/**
 * The containment tree of a scene (ADR 0010). It is a view of the scene's IDs, not a second hierarchy: every row is a
 * node of the scene, in the scene's order, and its state comes from the accepted scene, the authored options and the
 * geometry issues. Pure and framework-independent; the renderer only shows what this returns.
 */

/** The state of one axis of a container: content (`auto`), a minimum or a fixed size, and where it is set. */
export function axisState(axis) {
  if (!axis) return {state: 'auto', value: null, label: 'auto', source: 'default'};
  if ('min' in axis) return {state: 'min', value: axis.min, label: `min ${round(axis.min)}`, source: 'set here'};
  return {state: 'fixed', value: axis.fixed, label: `fixed ${round(axis.fixed)}`, source: 'set here'};
}

/** The size state of a container from its authored sizing (ADR 0010, decision 1). */
export function sizeState(sizing = {}) {
  return {width: axisState(sizing.width), height: axisState(sizing.height)};
}

/**
 * The conflicts per container: a child that does not fit its container is reported with the container as its second
 * node (the geometry validator's `container_bounds`). The message is kept for display.
 */
export function conflictsOf(geometryIssues = []) {
  const conflicts = new Map();
  for (const issue of geometryIssues) {
    const ids = issue.nodeIds ?? issue.node_ids ?? [];
    if (issue.kind !== 'container_bounds' || ids.length < 2) continue;
    const container = ids[1];
    if (!conflicts.has(container)) conflicts.set(container, issue.message);
  }
  return conflicts;
}

/**
 * Every conflict per node: the scene's own list (each kind, every party) or, for a scene without one, the geometry issues. A node
 * has an entry for each conflict that names it, with the other nodes and the routes involved.
 */
export function conflictsByNode(sceneConflicts, geometryIssues = []) {
  const list = Array.isArray(sceneConflicts)
    ? sceneConflicts
    : geometryIssues.map(issue => ({kind: issue.kind, nodes: issue.nodeIds ?? issue.node_ids ?? [], routes: [], reason: issue.message}));
  const byNode = new Map();
  for (const conflict of list) {
    for (const id of conflict.nodes ?? []) {
      if (!byNode.has(id)) byNode.set(id, []);
      byNode.get(id).push({
        kind: conflict.kind,
        reason: conflict.reason,
        with: (conflict.nodes ?? []).filter(other => other !== id),
        routes: conflict.routes ?? [],
      });
    }
  }
  return byNode;
}

/**
 * The rows of the tree, depth first, in the scene's order. A collapsed container shows no children, because the scene
 * has none for it. `pins` is the set of pinned node IDs (or an object keyed by ID); `containerSizing` is the authored
 * sizing keyed by container ID.
 */
export function containmentRows(scene, {pins = {}, containerSizing = {}, geometryIssues = [], settingsFor} = {}) {
  const pinned = new Set(Array.isArray(pins) ? pins : Object.keys(pins));
  const conflicts = conflictsOf(geometryIssues);
  const allConflicts = conflictsByNode(scene.conflicts, geometryIssues);
  const childrenOf = new Map();
  for (const node of scene.nodes) {
    const parent = node.parentId ?? null;
    if (!childrenOf.has(parent)) childrenOf.set(parent, []);
    childrenOf.get(parent).push(node);
  }
  const rows = [];
  const visit = (parentId, depth) => {
    for (const node of childrenOf.get(parentId) ?? []) {
      const children = childrenOf.get(node.id) ?? [];
      // A collapsed container is reported by the engine as collapsed without its children, and not always as a
      // container, so it is a container row either way.
      const container = Boolean(node.container || node.collapsed);
      rows.push({
        id: node.id,
        label: node.label,
        kind: node.kind,
        depth,
        parentId: parentId,
        container,
        expanded: container ? !node.collapsed : null,
        hasChildren: children.length > 0,
        pinned: pinned.has(node.id),
        invalid: node.invalid,
        shape: node.shape ?? 'rect',
        conflict: conflicts.get(node.id) ?? null,
        conflicts: allConflicts.get(node.id) ?? [],
        size: container ? sizeState(containerSizing[node.id]) : null,
        settings: container && settingsFor ? settingsFor(node.id) : null,
      });
      visit(node.id, depth + 1);
    }
  };
  visit(null, 0);
  return rows;
}

/**
 * The ID of the row that a key moves to (WAI-ARIA tree pattern, read-only subset): Up and Down move between rows, Home
 * and End to the first and last row, Right moves into the first child of an open container, Left to the parent.
 * Returns `current` when the key does nothing.
 */
export function nextRowId(rows, current, key) {
  const index = rows.findIndex(row => row.id === current);
  if (index < 0) return rows[0]?.id ?? null;
  const row = rows[index];
  switch (key) {
    case 'ArrowDown':
      return rows[Math.min(index + 1, rows.length - 1)].id;
    case 'ArrowUp':
      return rows[Math.max(index - 1, 0)].id;
    case 'Home':
      return rows[0].id;
    case 'End':
      return rows[rows.length - 1].id;
    case 'ArrowRight': {
      const next = rows[index + 1];
      return row.expanded && next?.parentId === row.id ? next.id : current;
    }
    case 'ArrowLeft':
      return row.parentId ?? current;
    default:
      return current;
  }
}

/**
 * The `containerSizing` entry for one container from its form state (ADR 0010, decision 3). Each axis is `auto`
 * (content), `min` or `fixed`, with a value for the last two. Returns `null` when both axes follow their content, which
 * removes the entry. Throws when a value is not a positive, finite number: the engine would reject it too, and the form
 * says so before anything is sent.
 */
export function sizingEntryOf({width, height}) {
  const axis = ({mode, value}) => {
    if (mode === 'auto') return undefined;
    const number = typeof value === 'number' ? value : Number(value);
    if (!(Number.isFinite(number) && number > 0) || value === '' || value === null) {
      throw new Error(`enter a positive size for ${mode === 'min' ? 'the minimum' : 'the fixed size'}`);
    }
    return {[mode]: number};
  };
  const entry = {width: axis(width), height: axis(height)};
  if (entry.width === undefined && entry.height === undefined) return null;
  for (const key of ['width', 'height']) if (entry[key] === undefined) delete entry[key];
  return entry;
}

function round(value) {
  return Math.round(value * 10) / 10;
}
