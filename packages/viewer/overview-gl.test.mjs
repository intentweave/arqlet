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

// A renderer on a fake GPU: every backend (WebGPU, WebGL 2, WebGL 1) over objects that accept any call, counting the
// frames it draws (a WebGL clear, a WebGPU submit), with animation frames and the clock in the test's hands.
const anything = () => new Proxy(function () {}, {get: (_, k) => (k === 'then' ? undefined : k === Symbol.toPrimitive ? () => 0 : anything()), apply: () => anything()});
const accepting = own => new Proxy(own, {get: (t, k) => (k in t ? t[k] : anything())});
const overview = {
  schemaVersion: 1,
  ids: ['c', 'a', 'b', 'd', 'e'],
  parent: [-1, 0, 0, -1, 3],
  x: [-20, -24, -16, 20, 20], y: [0, 0, 0, 0, 0], r: [10, 4, 4, 8, 4],
  rootRadius: 30, edges: [1, 2, 2, 1, 1, 4], links: [1, 2, 2, 0, 3, 1],
};

function gpuFakes(kind) {
  const counts = {frames: 0};
  const listeners = new Map();
  let clock = 0, next = 1;
  const pending = new Map();
  const saved = {};
  for (const name of ['requestAnimationFrame', 'cancelAnimationFrame', 'WebGL2RenderingContext', 'GPUBufferUsage', 'GPUShaderStage', 'GPUTextureUsage']) saved[name] = globalThis[name];
  globalThis.requestAnimationFrame = f => { pending.set(next, f); return next++; };
  globalThis.cancelAnimationFrame = id => pending.delete(id);
  globalThis.WebGL2RenderingContext = class {};
  globalThis.GPUBufferUsage = {UNIFORM: 1, COPY_DST: 2, VERTEX: 4};
  globalThis.GPUShaderStage = {VERTEX: 1, FRAGMENT: 2};
  globalThis.GPUTextureUsage = {RENDER_ATTACHMENT: 1};
  const gpu = Object.getOwnPropertyDescriptor(globalThis.navigator, 'gpu');
  Object.defineProperty(globalThis.navigator, 'gpu', {value: {getPreferredCanvasFormat: () => 'bgra8unorm'}, configurable: true});
  const now = mock.method(performance, 'now', () => clock);
  const gl = proto => {
    const own = Object.create(proto);
    Object.assign(own, {getShaderParameter: () => true, getProgramParameter: () => true, getAttribLocation: () => 0, clear: () => counts.frames++});
    return accepting(own);
  };
  const contexts = {
    webgl2: kind === 'webgl2' ? gl(globalThis.WebGL2RenderingContext.prototype) : null,
    webgl: kind === 'webgl1' ? gl(Object.prototype) : null,
    webgpu: kind === 'webgpu' ? accepting({configure() {}, unconfigure() {}}) : null,
  };
  const canvas = {
    width: 400, height: 300, clientWidth: 400, clientHeight: 300,
    style: {setProperty() {}, removeProperty() {}},
    getContext: type => contexts[type] ?? null,
    getBoundingClientRect: () => ({left: 0, top: 0, width: 400, height: 300}),
    addEventListener: (type, f) => listeners.set(type, f),
    removeEventListener: type => listeners.delete(type),
    setPointerCapture() {},
  };
  const device = kind === 'webgpu' ? accepting({queue: accepting({writeBuffer() {}, submit: () => counts.frames++})}) : null;
  return {
    canvas, counts, device,
    options: kind === 'webgpu' ? {device} : kind === 'webgl1' ? {backend: 'webgl1'} : {},
    pending: () => pending.size,
    // Runs the animation frames requested so far.
    flush() { const now = [...pending.values()]; pending.clear(); for (const f of now) f(clock); },
    advance(ms) { clock += ms; },
    fire: (type, e = {}) => listeners.get(type)?.({clientX: 10, clientY: 10, deltaY: 10, preventDefault() {}, ...e}),
    restore() {
      now.mock.restore();
      for (const [name, value] of Object.entries(saved)) globalThis[name] = value;
      if (gpu) Object.defineProperty(globalThis.navigator, 'gpu', gpu); else delete globalThis.navigator.gpu;
    },
  };
}

for (const kind of ['webgpu', 'webgl2', 'webgl1']) {
  test(`${kind}: paused, the renderer draws no frame and requests none; resume draws one at once and goes on`, async () => {
    const {createOverviewRenderer} = await import('./overview-gl.js');
    const {overviewModel} = await import('./overview.js');
    const f = gpuFakes(kind);
    try {
      const model = overviewModel(overview);
      const views = [];
      const r = createOverviewRenderer(f.canvas, model, {...f.options, onView: v => views.push(v)});
      assert.equal(r.backend, kind === 'webgl1' ? 'webgl' : kind);
      r.breathe({fadeIn: 1});
      f.flush();
      assert.ok(f.counts.frames >= 1 && f.pending() === 1, 'breathing draws every frame');
      r.pause();
      r.pause();
      assert.equal(r.paused, true);
      assert.equal(f.pending(), 0, 'the requested frame is cancelled');
      const frames = f.counts.frames;
      // Calls that change the state are taken, not drawn.
      r.setView({centreX: 5, centreY: 0, scale: 2});
      r.setState({selected: 1});
      r.setState({collapsed: new Set([0])}, {fade: 200});
      r.setLens({x: 100, y: 100, radius: 50, magnification: 3});
      r.setOrbit({yaw: 0.3});
      r.setGalaxy(true);
      r.setBackground([1, 1, 1, 1]);
      r.setNames({nodes: true});
      r.resize();
      r.draw();
      r.morph(overviewModel({...overview, x: overview.x.map(v => v + 3)}), {duration: 300});
      f.advance(10000);
      f.flush();
      assert.equal(f.counts.frames, frames, 'no frame while paused');
      assert.equal(f.pending(), 0, 'no animation frame while paused');
      assert.deepEqual(r.view, {centreX: 5, centreY: 0, scale: 2});
      assert.ok(views.length >= 1, 'onView still reports the view');
      assert.equal(r.galaxy, true);
      assert.equal(r.lens.radius, 50);
      r.resume();
      assert.equal(r.paused, false);
      assert.equal(f.counts.frames, frames + 1, 'resume draws one frame at once');
      assert.equal(f.pending(), 1, 'and goes on animating');
      assert.ok(r.breathing && r.animating, 'breathing, the morph and the blend run on');
      r.resume();
      assert.equal(f.counts.frames, frames + 1, 'a second resume does nothing');
      r.destroy();
      assert.equal(f.pending(), 0);
    } finally {
      f.restore();
    }
  });
}

test('the animations clock stands still while paused: a morph goes on where it stood', async () => {
  const {createOverviewRenderer} = await import('./overview-gl.js');
  const {overviewModel} = await import('./overview.js');
  const f = gpuFakes('webgl2');
  try {
    const r = createOverviewRenderer(f.canvas, overviewModel(overview), f.options);
    r.morph(overviewModel({...overview, x: overview.x.map(v => v + 3)}), {duration: 1000});
    f.advance(400);
    f.flush();
    r.pause();
    f.advance(60000);
    r.resume();
    f.flush();
    assert.ok(r.animating, 'not finished by the time spent paused');
    f.advance(599);
    f.flush();
    assert.ok(r.animating);
    f.advance(1);
    f.flush();
    assert.ok(!r.animating, 'finished after its own duration');
    // Breathing started while paused starts at resume.
    r.pause();
    r.breathe({fadeIn: 1000});
    f.advance(5000);
    r.resume();
    assert.ok(r.breathing);
    assert.equal(f.pending(), 1);
    r.destroy();
  } finally {
    f.restore();
  }
});

test('destroy works paused and running; pause and resume after it do nothing', async () => {
  const {createOverviewRenderer} = await import('./overview-gl.js');
  const {overviewModel} = await import('./overview.js');
  for (const kind of ['webgpu', 'webgl2']) {
    const f = gpuFakes(kind);
    try {
      const paused = createOverviewRenderer(f.canvas, overviewModel(overview), f.options);
      paused.breathe({});
      paused.pause();
      paused.destroy();
      const frames = f.counts.frames;
      paused.resume();
      paused.destroy();
      assert.equal(f.counts.frames, frames);
      assert.equal(f.pending(), 0);
      const running = createOverviewRenderer(f.canvas, overviewModel(overview), f.options);
      running.breathe({});
      running.destroy();
      running.pause();
      running.resume();
      assert.equal(f.pending(), 0);
    } finally {
      f.restore();
    }
  }
});

test('a navigation binding takes no input while paused, holds its idle wait and starts it again at resume', async () => {
  const {createOverviewRenderer} = await import('./overview-gl.js');
  const {overviewModel} = await import('./overview.js');
  mock.timers.enable({apis: ['setTimeout']});
  const f = gpuFakes('webgl2');
  try {
    const r = createOverviewRenderer(f.canvas, overviewModel(overview), f.options);
    const hovers = [], drags = [];
    const unbind = bindOverviewNavigation(f.canvas, r, {idle: {after: 1000}, onHover: i => hovers.push(i), onNodeDrag: (i, x, y, phase) => drags.push([i, phase])});
    mock.timers.tick(500);
    r.pause();
    mock.timers.tick(5000);
    assert.equal(r.breathing, false, 'the idle wait is held');
    const view = r.view;
    f.fire('wheel');
    f.fire('pointermove');
    f.fire('pointerdown');
    f.fire('pointermove', {clientX: 60});
    assert.equal(r.view, view, 'no zoom or pan');
    assert.equal(hovers.length, 0, 'no hover');
    r.resume();
    mock.timers.tick(999);
    assert.equal(r.breathing, false, 'the wait starts again at resume');
    mock.timers.tick(1);
    assert.equal(r.breathing, true);
    // Breathing that ran before the pause goes on after it.
    r.pause();
    r.resume();
    assert.equal(r.breathing, true);
    f.flush();
    // A node drag in progress ends at the pause, at its last point.
    r.pick = () => 1;
    f.fire('pointerdown');
    f.fire('pointermove', {clientX: 40});
    r.pause();
    assert.deepEqual(drags, [[1, 'move'], [1, 'end']]);
    unbind();
    r.resume();
    r.destroy();
  } finally {
    f.restore();
    mock.timers.reset();
  }
});
