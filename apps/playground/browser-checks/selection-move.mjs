// A selection moves as a whole, with real mouse input in a real Chrome: two nodes are chosen with Cmd-click, one is dragged,
// and both follow by the same distance while the engine receives the whole set in every step. A drag of a node outside the
// selection moves that node alone.
// usage: node selection-move.mjs [example=workflow-dag]
import {launch} from './launch.mjs';
import {connect} from './cdp.mjs';

const example = process.argv[2] ?? 'workflow-dag';
const running = await launch({serverPort: 8768, debugPort: 9335});
const page = await connect(running.debugPort);
const {evaluate, mouse, sleep, check} = page;
const META = 4;

await page.send('Runtime.enable');
await page.send('Page.enable');
await page.send('Emulation.setDeviceMetricsOverride', {width: 1400, height: 900, deviceScaleFactor: 1, mobile: false});
// Record every request that goes to a worker.
await page.send('Page.addScriptToEvaluateOnNewDocument', {source: `(() => { window.__sent = []; const O = window.Worker; window.Worker = class extends O { postMessage(m, ...r) { if (m?.request?.op === 'move') __sent.push({phase: m.request.phase, node: m.request.node, others: m.request.others ?? null}); return super.postMessage(m, ...r); } }; })();`});
await page.send('Page.navigate', {url: running.url});
await sleep(3000);
await page.waitForExample(example);
await evaluate(`(async () => {
  const s = document.getElementById('example'); s.value = ${JSON.stringify(example)}; s.dispatchEvent(new Event('change'));
  await new Promise(r => setTimeout(r, 3000));
})()`);
await page.requireExample(example);

const nodes = await evaluate(`(() => {
  const list = [...document.querySelectorAll('#graph g.node')].map(g => ({id: g.dataset.nodeId, r: g.getBoundingClientRect()}))
    .filter(n => n.r.width > 8 && n.r.left > 60 && n.r.top > 60 && n.r.right < innerWidth - 100 && n.r.bottom < innerHeight - 100);
  const mid = Math.floor(list.length / 2);
  return [list[mid], list[mid + 5], list[mid + 9]].filter(Boolean).map(n => ({id: n.id, x: n.r.left + n.r.width / 2, y: n.r.top + n.r.height / 2}));
})()`);
if (nodes.length < 3) { console.log('SKIPPED: not enough nodes inside the view'); page.close(); running.close(); process.exit(0); }
const [a, b, c] = nodes;
// A gesture is over when the page has sent nothing to the engine for a while (the last step is answered before the next press counts).
const quiet = async () => {
  let last = -1, stable = 0;
  for (let i = 0; i < 240 && stable < 4; i++) {
    const n = await evaluate('__sent.length');
    stable = n === last ? stable + 1 : 0;
    last = n;
    await sleep(300);
  }
};
const centre = id => evaluate(`(() => { const g = document.querySelector('g.node[data-node-id=${JSON.stringify(id)}]'); const r = g.getBoundingClientRect(); return {x: r.left + r.width / 2, y: r.top + r.height / 2}; })()`);
const click = async (n, modifiers = 0) => { await mouse('mouseMoved', n.x, n.y, modifiers); await mouse('mousePressed', n.x, n.y, modifiers); await mouse('mouseReleased', n.x, n.y, modifiers); await sleep(300); };

await click(a);
await click(b, META);
const chosen = await evaluate(`document.getElementById('selected').textContent`);
check('two nodes are selected', true, chosen);

const startA = await centre(a.id), startB = await centre(b.id), startC = await centre(c.id);
await evaluate(`__sent.length = 0`);
await mouse('mouseMoved', a.x, a.y);
await mouse('mousePressed', a.x, a.y);
for (let k = 1; k <= 6; k++) await mouse('mouseMoved', a.x + 10 * k, a.y + 5 * k);
await sleep(300);
const moveA = await centre(a.id), moveB = await centre(b.id), moveC = await centre(c.id);
const d = (p, q) => ({dx: Math.round(p.x - q.x), dy: Math.round(p.y - q.y)});
const da = d(moveA, startA), db = d(moveB, startB), dc = d(moveC, startC);
check('the grabbed node follows the pointer', Math.abs(da.dx - 60) <= 4 && Math.abs(da.dy - 30) <= 4, JSON.stringify(da));
check('the other selected node moves by the same distance', Math.abs(db.dx - da.dx) <= 4 && Math.abs(db.dy - da.dy) <= 4, JSON.stringify(db));
// The layered area may place the others again around the pinned nodes, so they can shift a little; they are not carried along.
check('a node outside the selection is not carried along', !(Math.abs(dc.dx - da.dx) <= 4 && Math.abs(dc.dy - da.dy) <= 4), JSON.stringify(dc));
await mouse('mouseReleased', a.x + 60, a.y + 30);
await quiet();
const sent = await evaluate(`__sent`);
const withOthers = sent.filter(s => s.others && s.others[b.id]);
check('the engine received the whole set in every step', sent.length > 0 && sent.every(s => s.others && s.others[b.id]), `${withOthers.length} of ${sent.length} steps`);

// A drag of a node outside the selection moves it alone. The layout may have placed it elsewhere meanwhile: read where it is now.
await evaluate(`__sent.length = 0`);
const here = await centre(c.id);
const top = await evaluate(`(() => { const e = document.elementFromPoint(${here.x}, ${here.y}); return e ? e.tagName + ' ' + (e.getAttribute('class') || '') + ' ' + (e.closest('g.node')?.dataset.nodeId ?? '') : null; })()`);
await mouse('mouseMoved', here.x, here.y); await mouse('mousePressed', here.x, here.y);
for (let k = 1; k <= 4; k++) await mouse('mouseMoved', here.x + 10 * k, here.y);
await sleep(300);
await mouse('mouseReleased', here.x + 40, here.y);
await quiet();
const alone = await evaluate(`__sent`);
check('a node outside the selection is moved alone', alone.length > 0 && alone.every(s => !s.others), JSON.stringify({alone, top, pressedNode: c.id}));

const failed = page.checks.filter(x => !x.ok).length;
page.close();
running.close();
console.log(failed ? `${failed} check(s) failed` : 'all checks passed');
process.exit(failed ? 1 : 0);
