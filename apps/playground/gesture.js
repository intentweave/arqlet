// Pointer tracking for one gesture (node drag, leaf resize, container resize). The listeners sit on the window for the
// duration of the gesture, so a release outside the drawing, outside the window or after a lost capture still ends it,
// and Escape reaches it wherever the focus is. A gesture ends exactly once, with one reason:
//   up           the primary button was released: the gesture commits (the release position is its last probe)
//   cancel       the browser cancelled the pointer (touch interrupted, window switched)
//   lostcapture  the capture was taken away before a release arrived
//   escape       the user pressed Escape
// `cancel`, `lostcapture` and `escape` abandon the gesture; the caller restores the engine's state.
// A caller ends a gesture itself (`viewchange`, `modelchange`, `error`) with `end(reason)`.

export const COMMIT_REASONS = new Set(['up']);

/**
 * @param {object} options
 * @param {EventTarget} options.win window, or any event target that receives pointer and key events
 * @param {number} options.pointerId the pointer that started the gesture; other pointers are ignored
 * @param {(event: Event) => void} options.onMove
 * @param {(reason: string, event?: Event) => void} options.onEnd called once
 * @returns {{end: (reason: string, event?: Event) => void, readonly ended: boolean}}
 */
export function trackGesture({win, pointerId, onMove, onEnd}) {
  let ended = false;
  const mine = event => event.pointerId === undefined || event.pointerId === pointerId;
  const finish = (reason, event) => {
    if (ended) return;
    ended = true;
    for (const [type, listener] of listeners) win.removeEventListener(type, listener, {capture: true});
    onEnd(reason, event);
  };
  const listeners = [
    ['pointermove', event => { if (mine(event)) onMove(event); }],
    ['pointerup', event => { if (mine(event)) finish('up', event); }],
    ['pointercancel', event => { if (mine(event)) finish('cancel', event); }],
    ['lostpointercapture', event => { if (mine(event)) finish('lostcapture', event); }],
    ['keydown', event => { if (event.key === 'Escape') finish('escape', event); }],
  ];
  for (const [type, listener] of listeners) win.addEventListener(type, listener, {capture: true});
  return {
    end: (reason, event) => finish(reason, event),
    get ended() { return ended; },
  };
}
