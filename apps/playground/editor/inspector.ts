/**
 * The inspector's text: what the curator has selected, as plain lines. No markup is built from graph data, so labels
 * cannot inject HTML; the panel shows the lines as text. The facts come from the reference document, never from a
 * second computation of geometry.
 */
import type {ReferenceDocument} from '@arqlet/contracts';
import type {Selection} from './selection.js';

const round = (value: number): number => Math.round(value * 10) / 10;

export function describeSelection(document: ReferenceDocument, selection: Selection): string[] {
  const curated = document.curated;
  const graph = document.input.graph;
  const labelOf = (id: string): string => graph.nodes.find(node => node.id === id)?.label ?? id;
  const pending = new Set(curated.pendingRoutes);
  const locked = new Set(curated.lockedRoutes ?? []);

  const notesOn = (target: string): string[] => {
    const notes = (document.annotations ?? []).filter(note => note.target === target);
    return [
      `notes: ${notes.length}`,
      ...notes.map(note => `  ${note.id}${note.category ? ` [${note.category}]` : ''}${note.severity ? ` (${note.severity})` : ''}: ${note.text}`),
    ];
  };

  if (selection.route !== null) {
    const edge = graph.edges.find(item => item.id === selection.route);
    const points = curated.routes[selection.route] ?? [];
    const lines = [`Route ${selection.route}`];
    if (edge) lines.push(`${labelOf(edge.source)} → ${edge.target ? labelOf(edge.target) : 'unresolved'}`);
    lines.push(`${points.length} points`);
    lines.push(locked.has(selection.route) ? 'Locked: its geometry is kept.' : 'Not locked.');
    lines.push(pending.has(selection.route) ? 'Pending: re-route it before it is valid.' : 'Not pending.');
    return [...lines, ...notesOn(selection.route)];
  }

  const nodes = selection.nodes;
  if (nodes.length === 0) {
    return ['Nothing selected. Click a node or a route. Ctrl or Cmd adds a node to the selection.'];
  }
  if (nodes.length > 1) {
    return [
      `${nodes.length} nodes selected`,
      ...nodes.map(id => `  ${labelOf(id)}`),
      'Drag one of them to move the whole selection.',
    ];
  }

  const id = nodes[0]!;
  const box = curated.nodes[id];
  const node = graph.nodes.find(item => item.id === id);
  const lines = [`Node ${labelOf(id)}`, `id: ${id}`, `kind: ${node?.kind ?? 'unknown'}`];
  if (node?.parentId) lines.push(`container: ${labelOf(node.parentId)}`);
  if (box) {
    lines.push(`box: x ${round(box.x)}, y ${round(box.y)}, width ${round(box.width)}, height ${round(box.height)}`);
  }
  const pin = curated.sizePins?.[id];
  lines.push(pin ? `size pinned at ${round(pin.width)} × ${round(pin.height)}` : 'size not pinned');
  const ports = Object.entries(curated.ports).filter(([, port]) => port.nodeId === id);
  lines.push(`ports: ${ports.length}`);
  for (const [, port] of ports) {
    const share = Math.round(port.offset * 100);
    lines.push(`  ${port.side} ${share} %${port.explicit ? ' (explicit, locked)' : ''} · ${port.edgeId}`);
  }
  const incident = graph.edges.filter(edge => edge.source === id || edge.target === id);
  lines.push(`routes: ${incident.length}`);
  for (const edge of incident) {
    const flags = [locked.has(edge.id) ? 'locked' : null, pending.has(edge.id) ? 'pending' : null].filter(Boolean);
    lines.push(`  ${edge.id}${flags.length ? ` · ${flags.join(', ')}` : ''}`);
  }
  return [...lines, ...notesOn(id)];
}
