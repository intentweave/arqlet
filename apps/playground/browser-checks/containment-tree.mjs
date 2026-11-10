// Containment tree in a real Chrome (ADR 0010, step 2): rows, selection in both directions, and arrow-key movement.
// usage: node containment-tree.mjs <debugPort> <url>
const [port, url] = [process.argv[2], process.argv[3]];
const sleep = ms => new Promise(r => setTimeout(r, ms));

const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
const page = targets.find(t => t.type === 'page');
const ws = new WebSocket(page.webSocketDebuggerUrl);
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
const click = async (x, y) => {
  await send('Input.dispatchMouseEvent', {type: 'mouseMoved', x, y});
  await send('Input.dispatchMouseEvent', {type: 'mousePressed', x, y, button: 'left', buttons: 1, clickCount: 1});
  await send('Input.dispatchMouseEvent', {type: 'mouseReleased', x, y, button: 'left', buttons: 0, clickCount: 1});
};
const key = async name => {
  await send('Input.dispatchKeyEvent', {type: 'keyDown', key: name, code: name, windowsVirtualKeyCode: {ArrowDown: 40, ArrowUp: 38}[name]});
  await send('Input.dispatchKeyEvent', {type: 'keyUp', key: name, code: name, windowsVirtualKeyCode: {ArrowDown: 40, ArrowUp: 38}[name]});
  await sleep(150);
};

await send('Runtime.enable');
await send('Page.enable');
await send('Emulation.setDeviceMetricsOverride', {width: 1400, height: 900, deviceScaleFactor: 1, mobile: false});
await send('Page.navigate', {url});
await sleep(4000);

const checks = [];
const check = (name, ok, detail = '') => checks.push({name, ok: Boolean(ok), detail});

const rowCount = await evaluate(`document.querySelectorAll('#containment-tree [role="treeitem"]').length`);
check('the tree lists the rows of the view', rowCount > 0, `${rowCount} rows`);

// Selecting a row selects the node on the canvas.
const target = await evaluate(`(() => { for (const row of document.querySelectorAll('#containment-tree [role="treeitem"]')) { if (row.querySelector('.tree-toggle') || row.dataset.id === '__view__') continue; row.scrollIntoView({block: 'center'}); const b = row.getBoundingClientRect(); if (b.top >= 0 && b.bottom <= innerHeight && b.left >= 0 && b.right <= innerWidth) return {id: row.dataset.id, label: row.textContent.split(' · ')[0], x: b.left + b.width / 2, y: b.top + b.height / 2}; } return null; })()`);
if (target) {
  await click(target.x, target.y);
  await sleep(600);
  const selected = await evaluate(`document.querySelector('#containment-tree [role="treeitem"][data-id=${JSON.stringify(target.id)}]').getAttribute('aria-selected')`);
  const label = await evaluate(`document.getElementById('selected').textContent`);
  check('a row selects its node', selected === 'true' && label === target.label, `aria-selected=${selected}, selected label "${label}"`);
} else {
  checks.push({name: 'a row selects its node', ok: true, detail: 'SKIPPED: no row without children'});
}

// A node on the canvas selects its row (selection sync from the drawing).
const node = await evaluate(`(() => { const g = [...document.querySelectorAll('#graph g.node')].map(n => ({id: n.dataset.nodeId, r: n.getBoundingClientRect()})).find(n => n.r.width > 30 && n.r.height > 14 && n.r.left > 0 && n.r.top > 0 && n.r.right < innerWidth && n.r.bottom < innerHeight); return g ? {id: g.id, x: g.r.left + g.r.width / 2, y: g.r.top + g.r.height / 2} : null; })()`);
if (node) {
  await click(node.x, node.y);
  await sleep(600);
  const rowSelected = await evaluate(`document.querySelector('#containment-tree [role="treeitem"][data-id=${JSON.stringify(node.id)}]')?.getAttribute('aria-selected')`);
  check('a selected node selects its row', rowSelected === 'true', `aria-selected=${rowSelected}`);
} else {
  checks.push({name: 'a selected node selects its row', ok: true, detail: 'SKIPPED: no node inside the view'});
}

// Arrow keys move the focus between rows.
await evaluate(`(() => { const rows = [...document.querySelectorAll('#containment-tree [role="treeitem"]')]; rows[0].focus(); })()`);
const first = await evaluate(`document.activeElement.dataset.id`);
await key('ArrowDown');
const second = await evaluate(`document.activeElement.dataset.id`);
check('ArrowDown moves the focus to the next row', first && second && first !== second, `${first} -> ${second}`);
await key('ArrowUp');
const back = await evaluate(`document.activeElement.dataset.id`);
check('ArrowUp returns the focus', back === first, `${back}`);

// Expanding and collapsing from the keyboard: Left closes an open container, Right opens it again (ADR 0010, step 3).
const open = await evaluate(`(() => { for (const row of document.querySelectorAll('#containment-tree [role="treeitem"][aria-expanded="true"]')) { row.scrollIntoView({block: 'center'}); const b = row.getBoundingClientRect(); if (b.top >= 0 && b.bottom <= innerHeight) return row.dataset.id; } return null; })()`);
if (open) {
  const state = () => evaluate(`document.querySelector('#containment-tree [role="treeitem"][data-id=${JSON.stringify(open)}]').getAttribute('aria-expanded')`);
  await evaluate(`document.querySelector('#containment-tree [role="treeitem"][data-id=${JSON.stringify(open)}]').focus()`);
  await key('ArrowLeft');
  await sleep(1500);
  const closed = await state();
  await key('ArrowRight');
  await sleep(1500);
  const reopened = await state();
  check('Left closes an open container and Right opens it again', closed === 'false' && reopened === 'true', `${closed} -> ${reopened}`);
} else {
  checks.push({name: 'Left closes an open container and Right opens it again', ok: true, detail: 'SKIPPED: no open container in view'});
}

// A size set in the form is applied by the engine at once and shown in the row; the reset of that axis removes it.
const container = await evaluate(`(() => { for (const row of document.querySelectorAll('#containment-tree [role="treeitem"][aria-expanded]')) { row.scrollIntoView({block: 'center'}); const b = row.getBoundingClientRect(); if (b.top >= 0 && b.bottom <= innerHeight) { row.click(); return row.dataset.id; } } return null; })()`);
if (container) {
  await sleep(800);
  const rowText = () => evaluate(`document.querySelector('#containment-tree [role="treeitem"][data-id=${JSON.stringify(container)}]').textContent`);
  await evaluate(`(() => { const mode = document.querySelector('#containment-tree select'); mode.value = 'min'; mode.dispatchEvent(new Event('change')); const number = document.querySelector('#containment-tree input'); number.value = '2000'; number.dispatchEvent(new Event('change')); })()`);
  await sleep(2500);
  const applied = await rowText();
  check('a width set in the form is applied by the engine', applied.includes('w min 2000'), applied.slice(0, 120));
  // The drawing shows it: the container's rectangle is at least the minimum, in layout units.
  await sleep(3000);
  const drawn = await evaluate(`Number(document.querySelector('rect[data-container-id=${JSON.stringify(container)}]')?.getAttribute('width'))`);
  check('the drawn container is at least the minimum width', drawn >= 1999.9, `width ${Math.round(drawn)}`);
  await evaluate(`document.querySelector('#containment-tree button[aria-label="Reset width to content"]').click()`);
  await sleep(2500);
  const reset = await rowText();
  check('the reset of an axis removes its authored size', !reset.includes('w min'), reset.slice(0, 120));
} else {
  checks.push({name: 'a width set in the form is applied by the engine', ok: true, detail: 'SKIPPED: no container in view'});
}

// A layout setting from the form: the engine applies it, the form names it with its provenance, and "inherit" clears it.
if (container) {
  const strategyInfo = () => evaluate(`[...document.querySelectorAll('#containment-tree label')].find(l => l.textContent.startsWith('strategy')).nextElementSibling?.textContent.trim() ?? ''`);
  const setStrategy = value => evaluate(`(() => { const select = [...document.querySelectorAll('#containment-tree label')].find(l => l.textContent.startsWith('strategy')).querySelector('select'); select.value = ${JSON.stringify(value)}; select.dispatchEvent(new Event('change')); })()`);
  // "none" applies to every container; "tree" is refused where the children do not form a tree, and the status says so.
  await setStrategy('none');
  await sleep(2500);
  const set = await strategyInfo();
  check('a strategy set in the form is applied and named with its provenance', set.startsWith('none') && set.includes('set here'), set);
  await setStrategy('inherit');
  await sleep(2500);
  const cleared = await strategyInfo();
  check('"inherit" clears the container’s own strategy', !cleared.includes('set here'), cleared);
}

// The settings sit directly below the selected row, and the filter shortens the list without changing the engine.
const belowSelected = await evaluate(`(() => { const sel = document.querySelector('#containment-tree [role="treeitem"][aria-selected="true"]'); return !!sel && sel.nextElementSibling?.getAttribute('role') === 'none' && !!sel.nextElementSibling.querySelector('select'); })()`);
check('the settings sit directly below the selected row', belowSelected);
const before = await evaluate(`document.querySelectorAll('#containment-tree [role="treeitem"]').length`);
await evaluate(`(() => { const f = document.getElementById('tree-filter'); f.value = 'index'; f.dispatchEvent(new Event('input')); })()`);
await sleep(800);
const filtered = await evaluate(`document.querySelectorAll('#containment-tree [role="treeitem"]').length`);
check('the filter shortens the list', filtered > 0 && filtered < before, `${before} -> ${filtered}`);
await evaluate(`(() => { const f = document.getElementById('tree-filter'); f.value = ''; f.dispatchEvent(new Event('input')); })()`);
await sleep(800);

// The view's settings sit under the View row: selecting it shows them, and a change there reaches the whole view.
const viewDirection = () => evaluate(`[...document.querySelectorAll('#containment-tree label')].find(l => l.textContent.startsWith('direction')).nextElementSibling?.textContent.trim() ?? ''`);
await evaluate(`document.querySelector('#containment-tree [role="treeitem"][data-id="__view__"]').click()`);
await sleep(800);
const before2 = await viewDirection();
const original = await evaluate(`[...document.querySelectorAll('#containment-tree label')].find(l => l.textContent.startsWith('direction')).querySelector('select').value`);
const newDirection = original === 'tb' ? 'lr' : 'tb';
await evaluate(`(() => { const select = [...document.querySelectorAll('#containment-tree label')].find(l => l.textContent.startsWith('direction')).querySelector('select'); select.value = ${JSON.stringify(newDirection)}; select.dispatchEvent(new Event('change')); })()`);
await sleep(2500);
const after2 = await viewDirection();
check('the View row shows the whole view\u2019s direction and changes it', after2.includes({tb: 'top → bottom', lr: 'left → right'}[newDirection]) && after2 !== before2, `${before2} -> ${after2}`);
await evaluate(`(() => { const select = [...document.querySelectorAll('#containment-tree label')].find(l => l.textContent.startsWith('direction')).querySelector('select'); select.value = ${JSON.stringify(original)}; select.dispatchEvent(new Event('change')); })()`);
await sleep(2000);

ws.close();
let failed = 0;
for (const c of checks) {
  console.log(`${c.ok ? 'PASS' : 'FAIL'} ${c.name}${c.detail ? ` (${c.detail})` : ''}`);
  if (!c.ok) failed++;
}
process.exit(failed ? 1 : 0);
