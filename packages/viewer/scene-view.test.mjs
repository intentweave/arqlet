import {test} from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {createSceneView, reactFlowViewport} from './scene-view.js';

const scene = JSON.parse(readFileSync(new URL('./fixtures/state-tcp.scene.json', import.meta.url)));

test('React Flow\'s viewport shows the same geometry as the view', () => {
  const view = {centreX: 120, centreY: -40, scale: 2.5}, vp = reactFlowViewport(view, 800, 600);
  // A world point at the view's centre lands in the pane's centre; one unit right lands 2.5 px right.
  assert.deepEqual([120 * vp.zoom + vp.x, -40 * vp.zoom + vp.y], [400, 300]);
  assert.equal((121 - 120) * vp.zoom, 2.5);
});

test('without a GPU backend the host\'s near layer draws the whole scene and follows the view', () => {
  const calls = [];
  const element = {style: {}, remove() { calls.push(['remove']); }};
  const host = {ownerDocument: {}, style: {}, clientWidth: 800, clientHeight: 400, children: [], append(e) { this.children.push(e); }};
  const near = {element, draw: (excerpt, frame) => calls.push(['draw', excerpt.nodes.length, frame.state.selected]), move: frame => calls.push(['move', frame.view.scale]), clear: () => calls.push(['clear'])};
  const v = createSceneView(host, scene, {backend: {kind: 'svg'}, near, state: {selected: 'a'}});
  assert.equal(v.layer, 'near');
  assert.equal(v.backend, 'svg');
  assert.deepEqual(calls[0], ['draw', scene.nodes.length, 'a']);
  assert.equal(element.style.visibility, 'visible');
  calls.length = 0;
  v.setView({...v.view, scale: v.view.scale * 2});
  assert.equal(calls[0][0], 'draw', 'a new scale draws again');
  calls.length = 0;
  v.setView({...v.view, centreX: v.view.centreX + 10});
  assert.deepEqual(calls.map(c => c[0]), ['move'], 'a pan only moves it');
  v.setState({selected: 'b', pinned: new Set()});
  assert.equal(calls.at(-2)[2], 'b');
  v.destroy();
  assert.deepEqual(calls.slice(-2), [['clear'], ['remove']]);
});
