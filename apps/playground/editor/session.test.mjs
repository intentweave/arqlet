// Curation session against the real WASM engine: every change goes through the same Rust reference operations as
// the browser, so these tests check the editor's contract, not a mock.
import {test} from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {createEngine} from '@arqlet/engine-client/wasm-client.js';
import {Selection} from '../generated/editor/selection.js';
import {describeSelection} from '../generated/editor/inspector.js';
import {CurationSession} from '../generated/editor/index.js';
import {shiftableSegments} from '../generated/editor/geometry.js';

const run = await createEngine(readFileSync(new URL('../generated/arqlet.wasm', import.meta.url)));
const engine = async request => run(request);
const snapshot = JSON.parse(readFileSync(new URL('../../../packages/viewer/fixtures/state-tcp.snapshot.json', import.meta.url), 'utf8'));
// The nested fixture: a container with leaves inside it, a boundary-crossing route and an unresolved edge.
const nestedGraph = JSON.parse(readFileSync(new URL('../../../examples/fixtures/reference/container-boundary.graph.json', import.meta.url), 'utf8'));
const nested = (await engine({op: 'import', graph: nestedGraph, limit: 40})).snapshot;

function parents(document) {
  return new Set(document.input.graph.nodes.map(node => node.parentId).filter(Boolean));
}

function leafWithRoute(document) {
  const containers = parents(document);
  for (const edge of document.input.graph.edges) {
    if (!edge.target || !document.curated.routes[edge.id]) continue;
    for (const id of [edge.source, edge.target]) {
      const node = document.input.graph.nodes.find(item => item.id === id);
      if (node && !containers.has(id)) return {node: id, route: edge.id};
    }
  }
  throw Error('fixture has no leaf with a route');
}

test('a draft from a snapshot starts at revision 1, valid for its geometry, with nothing pending', async () => {
  const session = await CurationSession.create(engine, snapshot, 'state-tcp');
  assert.equal(session.document.revision, 1);
  assert.equal(session.report.currentValidation, true);
  assert.equal(session.report.pendingRoutes, 0);
  assert.equal(session.canUndo, false);
});

test('moving a node is one undoable step, and the routes that touch it become pending', async () => {
  const session = await CurationSession.create(engine, snapshot, 'state-tcp');
  const {node, route} = leafWithRoute(session.document);
  const before = await session.canonical();

  let reply = await session.moveNode(node, 5, 0);
  if (!reply.ok) reply = await session.moveNode(node, -5, 0);
  assert.equal(reply.ok, true, reply.error);
  assert.equal(session.document.revision, 2);
  assert.ok(session.document.curated.pendingRoutes.includes(route));
  assert.equal(session.report.valid, false, 'a draft with pending routes is never valid');
  const moved = await session.canonical();
  assert.notEqual(moved, before);

  assert.equal(session.undo(), true);
  assert.equal(session.document.revision, 1);
  assert.deepEqual(session.document.curated.pendingRoutes, []);
  assert.equal(await session.canonical(), before, 'undo restores the document exactly');

  assert.equal(session.redo(), true);
  assert.equal(await session.canonical(), moved, 'redo reproduces the accepted change exactly');
});

test('a node cannot be dragged out of its container', async () => {
  const session = await CurationSession.create(engine, nested, 'container-boundary');
  const {node} = leafWithRoute(session.document);
  const containerId = session.document.input.graph.nodes.find(item => item.id === node).parentId;
  const reply = await session.moveNode(node, 100000, 100000);
  assert.equal(reply.ok, true, reply.error);
  const inner = session.document.curated.nodes[node];
  const outer = session.document.curated.nodes[containerId];
  assert.ok(inner.x >= outer.x && inner.x + inner.width <= outer.x + outer.width);
  assert.ok(inner.y >= outer.y && inner.y + inner.height <= outer.y + outer.height);
});

test('with growth on, a node leaves its container and the container grows to hold it, as one undoable step', async () => {
  const session = await CurationSession.create(engine, nested, 'container-boundary');
  const {node} = leafWithRoute(session.document);
  const containerId = session.document.input.graph.nodes.find(item => item.id === node).parentId;
  const before = {...session.document.curated.nodes[containerId]};
  const reply = await session.moveNode(node, 300, 0, true);
  assert.equal(reply.ok, true, reply.error);
  const inner = session.document.curated.nodes[node];
  const outer = session.document.curated.nodes[containerId];
  assert.ok(inner.x + inner.width <= outer.x + outer.width, 'the node is inside the grown container');
  assert.ok(outer.width > before.width, 'the container grew to the right');
  assert.equal(outer.x, before.x, 'its top-left corner stays');
  assert.equal(outer.y, before.y, 'its top-left corner stays');
  assert.equal(session.undo(), true);
  assert.deepEqual(session.document.curated.nodes[containerId], before, 'undo restores the container');
});

test('moving a container translates its members once, keeps their relative geometry, and undo restores them', async () => {
  const session = await CurationSession.create(engine, nested, 'container-boundary');
  const {node} = leafWithRoute(session.document);
  const containerId = session.document.input.graph.nodes.find(item => item.id === node).parentId;
  const before = structuredClone(session.document.curated.nodes);
  const members = Object.keys(before).filter(id => id !== containerId && session.document.input.graph.nodes.some(n => n.id === id && n.parentId === containerId));
  const reply = await session.moveContainer(containerId, 30, 20);
  assert.equal(reply.ok, true, reply.error);
  const after = session.document.curated.nodes;
  const shift = {x: after[containerId].x - before[containerId].x, y: after[containerId].y - before[containerId].y};
  assert.ok(shift.x !== 0 || shift.y !== 0, 'the container moved');
  for (const id of members) {
    assert.equal(after[id].x - before[id].x, shift.x, `${id} moved with the container`);
    assert.equal(after[id].y - before[id].y, shift.y, `${id} moved with the container`);
  }
  assert.equal(session.undo(), true);
  assert.deepEqual(session.document.curated.nodes, before, 'undo restores every member');
});

test('an automatic port moves to another side and its route becomes pending; explicit ports stay locked', async () => {
  const session = await CurationSession.create(engine, snapshot, 'state-tcp');
  const [portId, port] = Object.entries(session.document.curated.ports).find(([, p]) => !p.explicit);
  const side = port.side === 'left' ? 'right' : 'left';
  const reply = await session.movePortSide(portId, side);
  assert.equal(reply.ok, true, reply.error);
  const moved = session.document.curated.ports[portId];
  assert.equal(moved.side, side);
  const box = session.document.curated.nodes[moved.nodeId];
  const edgeX = side === 'left' ? box.x : box.x + box.width;
  assert.equal(moved.position.x, edgeX, 'the derived position lies on the new side');
  assert.ok(session.pending.has(port.edgeId), 'the route the port serves is pending');
  assert.equal(session.undo(), true);
  assert.equal(session.document.curated.ports[portId].side, port.side);
});

test('a dogleg adds a jog to an interior segment and undo removes it', async () => {
  const session = await CurationSession.create(engine, snapshot, 'state-tcp');
  const [routeId, segment] = Object.entries(session.document.curated.routes)
    .map(([id, points]) => [id, shiftableSegments(points)[0]])
    .find(([, first]) => first !== undefined);
  const before = session.document.curated.routes[routeId];
  const reply = await session.addDogleg(routeId, segment.index, 30);
  assert.equal(reply.ok, true, reply.error);
  const after = session.document.curated.routes[routeId];
  assert.equal(after.length, before.length + 4);
  assert.deepEqual(after[0], before[0]);
  assert.deepEqual(after.at(-1), before.at(-1));
  assert.equal(session.undo(), true);
  assert.deepEqual(session.document.curated.routes[routeId], before);
});

test('a leaf resize keeps the minimum, moves its ports with it, and undo restores it', async () => {
  const session = await CurationSession.create(engine, nested, 'container-boundary');
  const {node} = leafWithRoute(session.document);
  const before = {...session.document.curated.nodes[node]};
  const reply = await session.resizeNode(node, 10, 10);
  assert.equal(reply.ok, true, reply.error);
  const after = session.document.curated.nodes[node];
  assert.equal(after.width, 60, 'the width is clamped to the minimum');
  assert.equal(after.height, 30, 'the height is clamped to the minimum');
  assert.equal(after.x, before.x, 'the top-left corner stays');
  assert.equal(session.undo(), true);
  assert.deepEqual(session.document.curated.nodes[node], before);
});

test('a container resize never cuts into the boxes inside it', async () => {
  const session = await CurationSession.create(engine, nested, 'container-boundary');
  const {node} = leafWithRoute(session.document);
  const containerId = session.document.input.graph.nodes.find(item => item.id === node).parentId;
  const members = Object.keys(session.document.curated.nodes).filter(id => id !== containerId && session.document.input.graph.nodes.some(n => n.id === id && n.parentId === containerId));
  const before = structuredClone(session.document.curated.nodes);
  const reply = await session.resizeContainer(containerId, 1, 1);
  assert.equal(reply.ok, true, reply.error);
  const outer = session.document.curated.nodes[containerId];
  for (const id of members) {
    const inner = session.document.curated.nodes[id];
    assert.deepEqual(inner, before[id], `${id} keeps its geometry`);
    assert.ok(inner.x + inner.width + 12 <= outer.x + outer.width, `${id} stays inside with padding`);
    assert.ok(inner.y + inner.height + 12 <= outer.y + outer.height, `${id} stays inside with padding`);
  }
});

test('a port dragged past an automatic port on its side swaps places with it; undo restores both', async () => {
  const session = await CurationSession.create(engine, snapshot, 'state-tcp');
  const ports = Object.entries(session.document.curated.ports);
  const found = ports.flatMap(([id, a]) =>
    ports
      .filter(([other, b]) => other !== id && !a.explicit && !b.explicit && a.nodeId === b.nodeId && a.side === b.side && b.offset > a.offset + 0.05 && b.offset < 0.9)
      .map(([other, b]) => [id, a, other, b]),
  )[0];
  assert.ok(found, 'the fixture has two automatic ports on one side');
  const [id, a, other, b] = found;
  const box = session.document.curated.nodes[a.nodeId];
  const length = a.side === 'top' || a.side === 'bottom' ? box.width : box.height;
  const reply = await session.movePort(id, (b.offset - a.offset + 0.05) * length);
  assert.equal(reply.ok, true, reply.error);
  assert.ok(Math.abs(session.document.curated.ports[other].offset - a.offset) < 1e-9, 'the other port takes the old offset');
  assert.ok(session.pending.has(a.edgeId), "the moved port's route is pending");
  assert.equal(session.undo(), true);
  assert.equal(session.document.curated.ports[id].offset, a.offset);
  assert.equal(session.document.curated.ports[other].offset, b.offset);
});

test('a node clamped by its container says so in the reply', async () => {
  const session = await CurationSession.create(engine, nested, 'container-boundary');
  const {node} = leafWithRoute(session.document);
  const reply = await session.moveNode(node, 100000, 0);
  assert.equal(reply.ok, true, reply.error);
  assert.match(reply.notice, /stops at the border/);
});

test('a locked route keeps its geometry and refuses changes that would re-route it, until it is unlocked', async () => {
  const session = await CurationSession.create(engine, nested, 'container-boundary');
  const routeId = Object.keys(session.document.curated.routes)[0];
  const source = session.document.input.graph.edges.find(item => item.id === routeId).source;
  assert.equal((await session.lockRoute(routeId, true)).ok, true);
  assert.deepEqual(session.document.curated.lockedRoutes, [routeId]);
  const segmentRefused = await session.shiftSegment(routeId, 1, 10);
  assert.match(segmentRefused.error, /locked/);
  let moved = await session.moveNode(source, 25, 0);
  if (!moved.ok && !/locked/.test(moved.error)) moved = await session.moveNode(source, -25, 0);
  assert.match(moved.error, /locked/, 'a move that would re-route the locked route is refused');
  assert.equal((await session.lockRoute(routeId, false)).ok, true);
  assert.deepEqual(session.document.curated.lockedRoutes ?? [], [], 'an empty list is omitted by the engine');
});

test('a pinned size is kept through resizing until it is unpinned', async () => {
  const session = await CurationSession.create(engine, nested, 'container-boundary');
  const {node} = leafWithRoute(session.document);
  assert.equal((await session.toggleSizePin(node)).ok, true);
  const refused = await session.resizeNode(node, 90, 40);
  assert.match(refused.error, /pinned/);
  assert.equal((await session.toggleSizePin(node)).ok, true);
  assert.equal((await session.resizeNode(node, 90, 40)).ok, true);
});

test('a selected set of leaves moves by the same offset in one undoable step', async () => {
  const session = await CurationSession.create(engine, nested, 'container-boundary');
  const leaves = session.document.input.graph.nodes.filter(item => !session.document.input.graph.nodes.some(n => n.parentId === item.id)).map(item => item.id);
  const [a, b] = leaves.slice(0, 2);
  const before = structuredClone(session.document.curated.nodes);
  const reply = await session.moveNodes([a, b], 1, 1);
  assert.equal(reply.ok, true, reply.error);
  const after = session.document.curated.nodes;
  assert.equal(after[a].x - before[a].x, 1);
  assert.equal(after[b].x - before[b].x, 1);
  assert.equal(after[a].y - before[a].y, 1);
  assert.equal(after[b].y - before[b].y, 1);
  assert.equal(session.undo(), true);
  assert.deepEqual(session.document.curated.nodes, before);
});

test('a note is saved on its target, shown by the inspector, removed, and never changes the geometry', async () => {
  const session = await CurationSession.create(engine, nested, 'container-boundary');
  const {node} = leafWithRoute(session.document);
  const before = structuredClone(session.document.curated.nodes);
  assert.match((await session.addNote(node, '   ')).error, /needs text/);
  assert.match((await session.addNote('nowhere', 'hi')).error, /Unknown target/);
  const added = await session.addNote(node, '  check the port  ');
  assert.equal(added.ok, true, added.error);
  assert.equal(session.document.annotations.length, 1);
  assert.equal(session.document.annotations[0].text, 'check the port');
  assert.deepEqual(session.document.curated.nodes, before, 'a note does not move anything');
  const selection = new Selection();
  selection.replaceNode(node);
  assert.ok(describeSelection(session.document, selection).includes('  note-1: check the port'));
  assert.equal((await session.removeNotes(node)).ok, true);
  assert.equal(session.document.annotations.length, 0);
  assert.match((await session.removeNotes(node)).error, /no note/);
});

test('a note can name a category and a severity, and the inspector shows them', async () => {
  const session = await CurationSession.create(engine, nested, 'container-boundary');
  const {node} = leafWithRoute(session.document);
  assert.equal((await session.addNote(node, 'too close', 'clearance', 'warning')).ok, true);
  assert.equal(session.document.annotations[0].category, 'clearance');
  assert.equal(session.document.annotations[0].severity, 'warning');
  const selection = new Selection();
  selection.replaceNode(node);
  assert.ok(describeSelection(session.document, selection).includes('  note-1 [clearance] (warning): too close'));
});

test('two edits started together are applied one after the other; the second is refused, never applied over the first', async () => {
  const session = await CurationSession.create(engine, snapshot, 'state-tcp');
  const ids = Object.keys(session.document.curated.nodes);
  const [first, second] = [session.moveNode(ids[0], 4, 0), session.moveNode(ids[1], 4, 0)];
  const [a, b] = await Promise.all([first, second]);
  assert.equal([a, b].filter(reply => reply.ok).length, 1, 'exactly one of them applies');
  assert.equal(session.document.revision, 2, 'the document moved on once');
  const refused = [a, b].find(reply => !reply.ok);
  if (refused) assert.match(refused.error, /changed while this change waited/);
});

test('an interior orthogonal segment moves, and the endpoints stay where they were', async () => {
  const session = await CurationSession.create(engine, snapshot, 'state-tcp');
  const [routeId, segment] = Object.entries(session.document.curated.routes)
    .map(([id, points]) => [id, shiftableSegments(points)[0]])
    .find(([, first]) => first !== undefined);
  const points = session.document.curated.routes[routeId];
  const reply = await session.shiftSegment(routeId, segment.index, 10);
  assert.equal(reply.ok, true, reply.error);
  const moved = session.document.curated.routes[routeId];
  assert.deepEqual(moved[0], points[0]);
  assert.deepEqual(moved[moved.length - 1], points[points.length - 1]);
  assert.notDeepEqual(moved, points);
});

test('a saved reference reopens with the same identity and the same bytes', async () => {
  const session = await CurationSession.create(engine, snapshot, 'state-tcp');
  const text = await session.canonical();
  const reopened = await CurationSession.load(engine, text);
  assert.equal(reopened.document.integrity.payloadHash, session.document.integrity.payloadHash);
  assert.equal(await reopened.canonical(), text);
});

test('a tampered file is refused before it can become a draft', async () => {
  const session = await CurationSession.create(engine, snapshot, 'state-tcp');
  const text = (await session.canonical()).replace('"referenceId":"state-tcp"', '"referenceId":"other"');
  await assert.rejects(CurationSession.load(engine, text), /Integrity mismatch/);
  await assert.rejects(CurationSession.load(engine, '{not json'), /not valid JSON/);
});

test('the scene while curating shows moved nodes, and pending routes as provisional connectors', async () => {
  const session = await CurationSession.create(engine, nested, 'container-boundary');
  const baseline = (await engine({op: 'scene', snapshot: nested})).scene;
  const {node, route} = leafWithRoute(session.document);
  assert.equal((await session.moveNode(node, 5, 0)).ok || (await session.moveNode(node, -5, 0)).ok, true);
  const scene = session.scene(baseline);
  const moved = scene.nodes.find(item => item.id === node);
  assert.deepEqual(moved.bounds, session.document.curated.nodes[node]);
  assert.equal(scene.routes.find(item => item.id === route).points.length, 2);
  assert.deepEqual(scene.labels, []);
});

test('unknown nodes and routes are explicit errors, not silent no-ops', async () => {
  const session = await CurationSession.create(engine, snapshot, 'state-tcp');
  assert.match((await session.moveNode('no-such-node', 1, 1)).error, /Unknown node/);
  assert.match((await session.shiftSegment('no-such-route', 1, 1)).error, /has no curated route/);
  assert.equal(session.document.revision, 1);
});

/** Every route that is not pending meets the derived position of its ports. */
function portsMeetTheirRoutes(document) {
  const pending = new Set(document.curated.pendingRoutes);
  for (const [id, port] of Object.entries(document.curated.ports)) {
    const route = document.curated.routes[port.edgeId];
    if (!route || pending.has(port.edgeId)) continue;
    const end = port.endpoint === 'source' ? route[0] : route[route.length - 1];
    assert.ok(Math.abs(end.x - port.position.x) < 1e-6 && Math.abs(end.y - port.position.y) < 1e-6, `route ${port.edgeId} does not meet ${id}`);
  }
}

test('a moved node is re-routed, and its routes end on their ports again', async () => {
  const session = await CurationSession.create(engine, nested, 'container-boundary');
  const {node} = leafWithRoute(session.document);
  let reply = await session.moveNode(node, 15, 0);
  if (!reply.ok) reply = await session.moveNode(node, -15, 0);
  assert.equal(reply.ok, true, reply.error);
  assert.ok(session.document.curated.pendingRoutes.length > 0);
  const rerouted = await session.reroute();
  assert.equal(rerouted.ok, true, rerouted.error);
  assert.deepEqual(session.document.curated.pendingRoutes, []);
  assert.equal(session.report.currentValidation, true);
  portsMeetTheirRoutes(session.document);
});

test('a port slides along its side, its route becomes pending, and re-routing keeps it on the port', async () => {
  const session = await CurationSession.create(engine, snapshot, 'state-tcp');
  const [portId, port] = Object.entries(session.document.curated.ports).find(([, item]) => !item.explicit && session.document.curated.routes[item.edgeId]);
  let reply = await session.movePort(portId, 4);
  if (!reply.ok) reply = await session.movePort(portId, -4);
  assert.equal(reply.ok, true, reply.error);
  assert.ok(session.document.curated.pendingRoutes.includes(port.edgeId));
  assert.notEqual(session.document.curated.ports[portId].offset, port.offset);
  const rerouted = await session.reroute();
  assert.equal(rerouted.ok, true, rerouted.error);
  portsMeetTheirRoutes(session.document);
});

test('an explicit port is locked: curation refuses to move it', async () => {
  const graph = JSON.parse(readFileSync(new URL('../../../examples/fixtures/reference/ports-fanout.graph.json', import.meta.url), 'utf8'));
  const fanout = (await engine({op: 'import', graph, limit: 40})).snapshot;
  const session = await CurationSession.create(engine, fanout, 'ports-fanout');
  const [portId] = Object.entries(session.document.curated.ports).find(([, item]) => item.explicit);
  const reply = await session.movePort(portId, 5);
  assert.equal(reply.ok, false);
  assert.match(reply.error, /locked/);
  assert.equal(session.document.revision, 1);
});

test('re-routing without pending routes is refused with an explicit message', async () => {
  const session = await CurationSession.create(engine, snapshot, 'state-tcp');
  assert.match((await session.reroute()).error, /No pending routes/);
});

test('a pending route is drawn between its two ports, not between the node centres', async () => {
  const session = await CurationSession.create(engine, snapshot, 'state-tcp');
  const baseline = (await engine({op: 'scene', snapshot})).scene;
  const [portId, port] = Object.entries(session.document.curated.ports).find(([, item]) => !item.explicit && session.document.curated.routes[item.edgeId]);
  let reply = await session.movePort(portId, 4);
  if (!reply.ok) reply = await session.movePort(portId, -4);
  assert.equal(reply.ok, true, reply.error);
  const drawn = session.scene(baseline).routes.find(item => item.id === port.edgeId);
  const ends = [drawn.points[0], drawn.points[drawn.points.length - 1]];
  const attached = session.document.curated.ports[portId].position;
  assert.ok(ends.some(end => Math.abs(end.x - attached.x) < 1e-6 && Math.abs(end.y - attached.y) < 1e-6));
});

test('re-routing every route leaves nothing pending, keeps routes on their ports, and is one undo step', async () => {
  const session = await CurationSession.create(engine, snapshot, 'state-tcp');
  const before = await session.canonical();
  const reply = await session.rerouteAll();
  assert.equal(reply.ok, true, reply.error);
  assert.deepEqual(session.document.curated.pendingRoutes, []);
  assert.equal(session.report.currentValidation, true);
  portsMeetTheirRoutes(session.document);
  assert.equal(session.undo(), true);
  assert.equal(await session.canonical(), before);
});

test('moving a node onto a route that it does not touch makes that route pending', async () => {
  const session = await CurationSession.create(engine, snapshot, 'state-tcp');
  const curated = session.document.curated;
  const incident = new Set(session.document.input.graph.edges.filter(edge => edge.target).flatMap(edge => [edge.source, edge.target]));
  // A root-level node that is not an endpoint of the route we target.
  const [routeId, route] = Object.entries(curated.routes).find(([id]) => {
    const edge = session.document.input.graph.edges.find(item => item.id === id);
    return edge && edge.source !== edge.target;
  });
  const edge = session.document.input.graph.edges.find(item => item.id === routeId);
  const nodeId = Object.keys(curated.nodes).find(id => id !== edge.source && id !== edge.target && incident.has(id));
  const box = curated.nodes[nodeId];
  const point = route[Math.floor(route.length / 2)];
  // Move the node so that its centre lands on the route point.
  const dx = point.x - (box.x + box.width / 2);
  const dy = point.y - (box.y + box.height / 2);
  const reply = await session.moveNode(nodeId, dx, dy);
  assert.equal(reply.ok, true, reply.error);
  assert.ok(session.document.curated.pendingRoutes.includes(routeId), 'the route that passes the moved node is pending');
});

test('an undo while a change is still being computed is not overwritten by that change', async () => {
  const session = await CurationSession.create(engine, snapshot, 'state-tcp');
  const {node, route} = leafWithRoute(session.document);
  const first = await session.moveNode(node, 5, 0);
  if (!first.ok) await session.moveNode(node, -5, 0);
  assert.equal(session.document.revision, 2);
  const other = Object.keys(session.document.curated.nodes).find(id => id !== node);
  const pending = session.moveNode(other, 3, 0);
  assert.equal(session.undo(), true, 'undo runs while the second change is pending');
  const result = await pending;
  assert.equal(result.ok, false, 'the pending change is discarded');
  assert.match(result.error, /undo or redo/);
  assert.equal(session.document.revision, 1, 'the undo result is kept');
  assert.ok(route);
});
