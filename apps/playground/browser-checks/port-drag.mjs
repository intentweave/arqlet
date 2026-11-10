// A port is dragged along the border of its node with real mouse input in a real Chrome. The release places the end of the edge
// there as an explicit port; Cmd+Z hands it back to the layout.
// usage: node port-drag.mjs [example=state-order]
import {launch} from './launch.mjs';
import {connect} from './cdp.mjs';

const example = process.argv[2] ?? 'state-order';
const running = await launch({serverPort: 8769, debugPort: 9336});
const page = await connect(running.debugPort);
const {evaluate, mouse, key, sleep, check} = page;

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
// Zoom in so that a port is a target of a few pixels.
await evaluate(`document.getElementById('zoom').click(); document.getElementById('zoom').click(); document.getElementById('zoom').click(); document.getElementById('zoom').click()`);
await sleep(800);

const pickPort = () => evaluate(`(() => {
  // Only a port that the pointer really reaches: nothing else (a panel, a node) may cover it.
  const hits = [...document.querySelectorAll('[data-port-hit]')].map(h => ({id: h.dataset.portHit, el: h, r: h.getBoundingClientRect()}))
    .filter(p => p.id.startsWith('auto:') && p.r.width > 6 && document.elementFromPoint(p.r.left + p.r.width / 2, p.r.top + p.r.height / 2) === p.el);
  const p = hits[Math.floor(hits.length / 2)];
  return p ? {id: p.id, x: p.r.left + p.r.width / 2, y: p.r.top + p.r.height / 2} : null;
})()`);
const port = await pickPort();
if (!port) { console.log('SKIPPED: no automatic port inside the view'); page.close(); running.close(); process.exit(0); }
const [, edge, endpoint] = port.id.split(':').length === 3 ? port.id.split(':') : [null, port.id.split(':').slice(1, -1).join(':'), port.id.split(':').at(-1)];
const userId = `user:${edge}:${endpoint}`;

const orig = await evaluate(`(() => { const d = document.querySelector('[data-port-id=${JSON.stringify(port.id)}]'); return {cx: +d.getAttribute('cx'), cy: +d.getAttribute('cy')}; })()`);
// Drag it a long way up-left, outside its node: the nearest side of the node takes it.
await mouse('mouseMoved', port.x, port.y);
await mouse('mousePressed', port.x, port.y);
for (let k = 1; k <= 8; k++) await mouse('mouseMoved', port.x - 8 * k, port.y - 14 * k);
await sleep(200);
const ghost = await evaluate(`document.querySelectorAll('.port-ghost').length`);
check('a ghost marks the place while dragging', ghost === 1, String(ghost));
await mouse('mouseReleased', port.x - 64, port.y - 112);
await sleep(1800);
const placed = await evaluate(`(() => { const d = document.querySelector('[data-port-id=${JSON.stringify(userId)}]'); return d ? {cx: +d.getAttribute('cx'), cy: +d.getAttribute('cy')} : null; })()`);
check('the end of the edge has an explicit port of the user', placed !== null, userId);
check('the port sits somewhere else than before', placed && Math.hypot(placed.cx - orig.cx, placed.cy - orig.cy) > 5, JSON.stringify({orig, placed}));
check('the ghost is gone after the release', (await evaluate(`document.querySelectorAll('.port-ghost').length`)) === 0);
await key('z', 4); await sleep(1800);
const after = await evaluate(`document.querySelector('[data-port-id=${JSON.stringify(userId)}]') === null && document.querySelector('[data-port-id=${JSON.stringify(port.id)}]') !== null`);
check('Cmd+Z hands the end back to the layout', after, await evaluate(`document.getElementById('status').textContent`));
await key('z', 12); await sleep(1800);
check('Cmd+Shift+Z places it again', (await evaluate(`document.querySelector('[data-port-id=${JSON.stringify(userId)}]') !== null`)));

const failed = page.checks.filter(c => !c.ok).length;
page.close();
running.close();
console.log(failed ? `${failed} check(s) failed` : 'all checks passed');
process.exit(failed ? 1 : 0);
