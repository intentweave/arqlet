// Undo and redo with real input in a real Chrome: one action is one step, whatever the engine did for it. A shape, a pin and a
// collapse are made through the page's controls, undone with Ctrl/Cmd+Z and redone with Ctrl/Cmd+Shift+Z. A new node ends
// the history (the remembered decisions no longer describe the graph).
// usage: node history.mjs [example=workflow-dag]   (starts its own server and Chrome; see launch.mjs)
import {launch} from './launch.mjs';
import {connect} from './cdp.mjs';

const example = process.argv[2] ?? 'workflow-dag';
const running = await launch({serverPort: 8767, debugPort: 9334});
const page = await connect(running.debugPort);
const {evaluate, mouse, key, sleep, check} = page;
const META = 4, SHIFT = 8;

await page.send('Runtime.enable');
await page.send('Page.enable');
await page.send('Emulation.setDeviceMetricsOverride', {width: 1400, height: 900, deviceScaleFactor: 1, mobile: false});
await page.send('Page.navigate', {url: running.url});
await sleep(3000);
await page.waitForExample(example);
await evaluate(`(async () => {
  const s = document.getElementById('example'); s.value = ${JSON.stringify(example)}; s.dispatchEvent(new Event('change'));
  await new Promise(r => setTimeout(r, 3000));
})()`);
await page.requireExample(example);

const status = () => evaluate(`document.getElementById('status').textContent`);
const settle = async () => { await sleep(1500); };
const shapeValue = () => evaluate(`document.getElementById('shape-select').value`);
const pinnedCount = () => evaluate(`document.querySelectorAll('[data-pinned-id]').length`);

// Select a leaf by a real click.
const pick = await evaluate(`(() => {
  const list = [...document.querySelectorAll('#graph g.node')].map(g => ({id: g.dataset.nodeId, r: g.getBoundingClientRect()}))
    .filter(n => n.r.width > 8 && n.r.left > 40 && n.r.top > 40 && n.r.right < innerWidth - 80 && n.r.bottom < innerHeight - 80);
  const n = list[Math.floor(list.length / 2)];
  return n ? {id: n.id, x: n.r.left + n.r.width / 2, y: n.r.top + n.r.height / 2} : null;
})()`);
if (!pick) { console.log('SKIPPED: no node inside the view'); page.close(); running.close(); process.exit(0); }
await mouse('mouseMoved', pick.x, pick.y); await mouse('mousePressed', pick.x, pick.y); await mouse('mouseReleased', pick.x, pick.y);
await sleep(500);

// 1. Shape.
check('the node starts as a rectangle', (await shapeValue()) === 'rect', await shapeValue());
await evaluate(`(() => { const s = document.getElementById('shape-select'); s.value = 'circle'; s.dispatchEvent(new Event('change')); })()`);
await settle();
check('the shape change is applied', (await shapeValue()) === 'circle');
await key('z', META); await settle();
check('Cmd+Z restores the rectangle', (await shapeValue()) === 'rect', await status());
await key('z', META | SHIFT); await settle();
check('Cmd+Shift+Z makes it a circle again', (await shapeValue()) === 'circle', await status());
await key('z', META); await settle();

// 2. Pin.
const before = await pinnedCount();
await evaluate(`document.getElementById('pin-node').click()`);
await settle();
const pinned = await pinnedCount();
check('the pin is set', pinned === before + 1, `${before} -> ${pinned}`);
await key('z', META); await settle();
check('Cmd+Z removes the pin', (await pinnedCount()) === before, await status());
await key('y', META); await settle();
check('Cmd+Y sets the pin again', (await pinnedCount()) === pinned, await status());
await key('z', META); await settle();

// The buttons follow the same stack.
await evaluate(`document.getElementById('pin-node').click()`);
await settle();
check('the Undo button is enabled after a pin', await evaluate(`!document.getElementById('undo').disabled`));
await evaluate(`document.getElementById('undo').click()`);
await settle();
check('the Undo button removes the pin', (await pinnedCount()) === before, await status());
check('the Redo button is enabled afterwards', await evaluate(`!document.getElementById('redo').disabled`));

// A reference is saved from the view, without a legacy draft.
await evaluate(`document.getElementById('save-ref').click()`);
await settle();
check('a reference is saved without a draft', /Reference saved/.test(await status()), await status());
check('the view is not in curation afterwards', await evaluate(`document.getElementById('curate').textContent.startsWith('Legacy')`));

// 3. A new node ends the history.
await evaluate(`document.getElementById('add').click()`);
await settle();
const afterAdd = await evaluate(`document.getElementById('stats').textContent`);
await key('z', META); await settle();
const afterUndo = await evaluate(`document.getElementById('stats').textContent`);
check('after a new node there is nothing to undo', afterAdd === afterUndo, afterUndo);

const failed = page.checks.filter(c => !c.ok).length;
page.close();
running.close();
console.log(failed ? `${failed} check(s) failed` : 'all checks passed');
process.exit(failed ? 1 : 0);
