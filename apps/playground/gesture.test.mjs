import test from 'node:test';
import assert from 'node:assert/strict';
import {trackGesture} from './gesture.js';

const fire = (win, type, props = {}) => win.dispatchEvent(Object.assign(new Event(type), {pointerId: 1}, props));

function setup() {
  const win = new EventTarget();
  const log = [];
  const track = trackGesture({win, pointerId: 1, onMove: () => log.push('move'), onEnd: (reason) => log.push(`end:${reason}`)});
  return {win, log, track};
}

test('a release ends the gesture once, with the reason up', () => {
  const {win, log} = setup();
  fire(win, 'pointermove');
  fire(win, 'pointerup');
  fire(win, 'lostpointercapture'); // the browser sends this after every release
  fire(win, 'pointermove');
  assert.deepEqual(log, ['move', 'end:up']);
});

test('pointercancel, lost capture and Escape abandon the gesture', () => {
  for (const [type, props, reason] of [['pointercancel', {}, 'cancel'], ['lostpointercapture', {}, 'lostcapture'], ['keydown', {key: 'Escape'}, 'escape']]) {
    const {win, log} = setup();
    fire(win, type, props);
    assert.deepEqual(log, [`end:${reason}`], type);
  }
});

test('other pointers and other keys are ignored', () => {
  const {win, log, track} = setup();
  fire(win, 'pointermove', {pointerId: 2});
  fire(win, 'pointerup', {pointerId: 2});
  fire(win, 'keydown', {key: 'a'});
  assert.deepEqual(log, []);
  assert.equal(track.ended, false);
});

test('the caller can end the gesture; later events change nothing, and the next gesture starts clean', () => {
  const {win, log, track} = setup();
  track.end('modelchange');
  fire(win, 'pointerup');
  assert.deepEqual(log, ['end:modelchange']);
  assert.equal(track.ended, true);
  const second = [];
  trackGesture({win, pointerId: 1, onMove: () => second.push('move'), onEnd: reason => second.push(`end:${reason}`)});
  fire(win, 'pointermove');
  fire(win, 'pointerup');
  assert.deepEqual(second, ['move', 'end:up']);
  assert.deepEqual(log, ['end:modelchange']);
});
