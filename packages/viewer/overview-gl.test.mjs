import {test, mock} from 'node:test';
import assert from 'node:assert/strict';
import {bindOverviewNavigation} from './overview-gl.js';

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
