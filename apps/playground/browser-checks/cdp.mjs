// A small DevTools-protocol client for the browser checks: open a page of a running Chrome and drive it with evaluate,
// real mouse and real key events. Not part of CI.
export async function connect(port) {
  const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
  const ws = new WebSocket(targets.find(t => t.type === 'page').webSocketDebuggerUrl);
  await new Promise((resolve, reject) => { ws.onopen = resolve; ws.onerror = reject; });
  let id = 0;
  const pending = new Map();
  ws.onmessage = event => {
    const msg = JSON.parse(event.data);
    if (msg.id && pending.has(msg.id)) {
      const {resolve, reject} = pending.get(msg.id);
      pending.delete(msg.id);
      msg.error ? reject(new Error(JSON.stringify(msg.error))) : resolve(msg.result);
    }
  };
  const send = (method, params = {}) => new Promise((resolve, reject) => {
    const req = ++id;
    pending.set(req, {resolve, reject});
    ws.send(JSON.stringify({id: req, method, params}));
  });
  const evaluate = async expression => {
    const result = await send('Runtime.evaluate', {expression, awaitPromise: true, returnByValue: true});
    if (result.exceptionDetails) throw new Error(result.exceptionDetails.exception?.description ?? 'evaluation failed');
    return result.result.value;
  };
  const mouse = (type, x, y, modifiers = 0) => send('Input.dispatchMouseEvent', {type, x, y, button: 'left', buttons: type === 'mouseReleased' ? 0 : 1, clickCount: 1, modifiers});
  // modifiers: 1 Alt, 2 Ctrl, 4 Meta, 8 Shift
  const key = async (k, modifiers = 0) => {
    const code = k.length === 1 ? `Key${k.toUpperCase()}` : k;
    await send('Input.dispatchKeyEvent', {type: 'rawKeyDown', key: k, code, modifiers, windowsVirtualKeyCode: k.length === 1 ? k.toUpperCase().charCodeAt(0) : 27});
    await send('Input.dispatchKeyEvent', {type: 'keyUp', key: k, code, modifiers});
  };
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  const checks = [];
  const check = (name, ok, detail = '') => { checks.push({name, ok: Boolean(ok), detail}); console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${detail ? ` (${detail})` : ''}`); };
  // The example list is fetched after the page loads, and an import takes longer on a busy machine: wait for the option before the
  // script selects it, and for the import after, so that a check never runs silently on the default model.
  const waitFor = async (expression, what, ms = 90000) => {
    const start = Date.now();
    while (Date.now() - start < ms) {
      if (await evaluate(expression)) return;
      await sleep(250);
    }
    throw new Error(`${what} did not happen within ${ms} ms`);
  };
  // The automatic background optimisation is switched off: a result that arrives in the middle of a check would move the nodes the
  // check has picked (the checks test the gestures and the edits, not the optimisation).
  const waitForExample = async name => {
    await waitFor(`!!document.querySelector('#example option[value=${JSON.stringify(name)}]')`, `the example ${name} is offered`);
    await evaluate(`(() => { const auto = document.getElementById('auto'); if (auto) { auto.checked = false; auto.dispatchEvent(new Event('change')); } })()`);
  };
  const requireExample = (name, atLeast = 6) => waitFor(`document.querySelectorAll('#graph g.node').length >= ${atLeast}`, `the example ${name} is drawn`);
  return {ws, send, evaluate, mouse, key, sleep, check, checks, waitFor, waitForExample, requireExample, close: () => ws.close()};
}
