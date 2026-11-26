import {test, mock} from 'node:test';
import assert from 'node:assert/strict';
import {bindOverviewNavigation, edgeFadeMask} from './overview-gl.js';

// A canvas and a renderer that only record what the navigation asks of them.
function fakes() {
  const listeners = new Map();
  const canvas = {
    addEventListener: (type, f) => listeners.set(type, f),
    removeEventListener: type => listeners.delete(type),
    getBoundingClientRect: () => ({left: 0, top: 0, width: 400, height: 300}),
    setPointerCapture() {},
  };
  const calls = [];
  const renderer = {
    view: {centreX: 0, centreY: 0, scale: 1},
    breathe: (settings, stop) => calls.push(['breathe', settings, stop]),
    setLens: lens => calls.push(['lens', lens]),
    setView() {},
    pick: () => -1,
    world: (x, y) => [x, y],
  };
  const fire = (type, e = {}) => listeners.get(type)?.({clientX: 10, clientY: 10, preventDefault() {}, ...e});
  return {canvas, renderer, calls, fire, listeners};
}

test('the discs breathe after the idle time, stop at input and wait again; unbinding stops them at once', () => {
  mock.timers.enable({apis: ['setTimeout']});
  try {
    const {canvas, renderer, calls, fire} = fakes();
    const unbind = bindOverviewNavigation(canvas, renderer, {idle: {after: 1000, breathe: {amplitude: 0.1}}});
    const breaths = () => calls.filter(c => c[0] === 'breathe' && c[1]);
    mock.timers.tick(999);
    assert.equal(breaths().length, 0);
    mock.timers.tick(1);
    assert.deepEqual(breaths().at(-1)[1], {amplitude: 0.1});
    fire('pointermove');
    assert.deepEqual(calls.at(-1), ['breathe', null, undefined], 'input fades the breathing out');
    mock.timers.tick(600);
    fire('wheel', {deltaY: 10});
    mock.timers.tick(600);
    assert.equal(breaths().length, 1, 'each input starts the wait again');
    mock.timers.tick(400);
    assert.equal(breaths().length, 2);
    unbind();
    assert.deepEqual(calls.at(-1), ['breathe', null, {fadeOut: 0}]);
    mock.timers.tick(5000);
    assert.equal(breaths().length, 2, 'nothing after unbinding');
  } finally {
    mock.timers.reset();
  }
});

test('no breathing without idle settings, and none under reduced motion', () => {
  mock.timers.enable({apis: ['setTimeout']});
  const media = globalThis.matchMedia;
  try {
    const plain = fakes();
    bindOverviewNavigation(plain.canvas, plain.renderer, {});
    mock.timers.tick(60000);
    assert.equal(plain.calls.filter(c => c[0] === 'breathe' && c[1]).length, 0);
    globalThis.matchMedia = query => ({matches: query.includes('reduce')});
    const still = fakes();
    bindOverviewNavigation(still.canvas, still.renderer, {idle: {after: 10}});
    mock.timers.tick(60000);
    assert.equal(still.calls.filter(c => c[0] === 'breathe' && c[1]).length, 0);
  } finally {
    globalThis.matchMedia = media;
    mock.timers.reset();
  }
});

test('a drag that starts on a leaf moves it; on a cluster or the background it pans', () => {
  const {canvas, renderer, fire} = fakes();
  // Node 1 is a leaf at (10, 10); node 0 is a cluster.
  renderer.model = {children: [[1], []]};
  let at = 1;
  renderer.pick = () => at;
  const moves = [], views = [];
  renderer.setView = v => views.push(v);
  bindOverviewNavigation(canvas, renderer, {onNodeDrag: (i, x, y, phase) => moves.push([i, x, y, phase])});
  fire('pointerdown');
  fire('pointermove', {clientX: 30, clientY: 25});
  fire('pointerup', {clientX: 40, clientY: 25});
  assert.deepEqual(moves, [[1, 30, 25, 'move'], [1, 40, 25, 'end']]);
  assert.equal(views.length, 0);
  // On the cluster: a pan.
  at = 0;
  fire('pointerdown');
  fire('pointermove', {clientX: 30, clientY: 25});
  fire('pointerup', {clientX: 30, clientY: 25});
  assert.equal(moves.length, 2);
  assert.equal(views.length, 1);
});

test('draggable chooses what a drag moves: a cluster when the page says so', () => {
  const {canvas, renderer, fire} = fakes();
  renderer.model = {children: [[1], []]};
  renderer.pick = () => 1;
  const moves = [], views = [];
  renderer.setView = v => views.push(v);
  // Over the leaf 1, the page moves its cluster 0.
  bindOverviewNavigation(canvas, renderer, {onNodeDrag: (i, x, y, phase) => moves.push([i, phase]), draggable: i => (i === 1 ? 0 : -1)});
  fire('pointerdown');
  fire('pointermove', {clientX: 30, clientY: 25});
  fire('pointerup', {clientX: 30, clientY: 25});
  assert.deepEqual(moves, [[0, 'move'], [0, 'end']]);
  assert.equal(views.length, 0);
});

test('under the orbit a drag turns the drawing and Shift-drag pans', () => {
  const {canvas, renderer, fire} = fakes();
  let orbit = {yaw: 0, pitch: 0, depth: 10};
  Object.defineProperty(renderer, 'orbit', {get: () => orbit && {...orbit}});
  renderer.setOrbit = next => { orbit = {...orbit, ...next}; };
  const views = [];
  renderer.setView = v => views.push(v);
  bindOverviewNavigation(canvas, renderer, {onNodeDrag: () => assert.fail('no node moves under the orbit')});
  fire('pointerdown');
  fire('pointermove', {clientX: 110, clientY: 60});
  fire('pointerup', {clientX: 110, clientY: 60});
  assert.ok(Math.abs(orbit.yaw - 0.6) < 1e-9 && Math.abs(orbit.pitch - 0.3) < 1e-9);
  assert.equal(views.length, 0);
  fire('pointerdown');
  fire('pointermove', {clientX: 40, clientY: 10, shiftKey: true});
  assert.equal(views.length, 1);
});

test('the edge fade is a mask of two eased gradients, transparent at the edges and opaque inside; none for 0', () => {
  assert.equal(edgeFadeMask(0), '');
  assert.equal(edgeFadeMask(-5), '');
  const mask = edgeFadeMask(80);
  assert.equal(mask.match(/linear-gradient\(/g).length, 2);
  assert.ok(mask.includes('linear-gradient(to right, rgba(0,0,0,0.000) 0.0px') && mask.includes('linear-gradient(to bottom, rgba(0,0,0,0.000) 0.0px'));
  assert.ok(mask.includes('rgba(0,0,0,1.000) 80.0px') && mask.includes('rgba(0,0,0,1.000) calc(100% - 80.0px)'));
  assert.ok(mask.includes('rgba(0,0,0,0.000) calc(100% - 0.0px))'));
  // Eased: the step at a fifth of the width is fainter than a fifth.
  assert.ok(mask.includes('rgba(0,0,0,0.104) 16.0px'));
});
