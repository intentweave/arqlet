// A label is dragged to a new place with real mouse input in a real Chrome. It stays there after the engine's recomputation
// while the place is free, and Cmd+Z puts it back to where the layout had it.
// usage: node label-drag.mjs [example=state-order]   (a small example, so that a label is a sizeable target)
import {launch} from './launch.mjs';
import {connect} from './cdp.mjs';

const example = process.argv[2] ?? 'state-order';
const running = await launch({serverPort: 8770, debugPort: 9337});
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
  const box = document.getElementById('edge-labels'); if (!box.checked) { box.checked = true; box.dispatchEvent(new Event('change')); }
  await new Promise(r => setTimeout(r, 2500));
})()`);
await page.requireExample(example);

const pick = () => evaluate(`(() => {
  const labels = [...document.querySelectorAll('rect[data-label-id]')].map(el => ({id: el.dataset.labelId, el, r: el.getBoundingClientRect()}))
    .filter(l => l.r.width > 12 && document.elementFromPoint(l.r.left + l.r.width / 2, l.r.top + l.r.height / 2) === l.el);
  const l = labels[0];
  return l ? {id: l.id, x: l.r.left + l.r.width / 2, y: l.r.top + l.r.height / 2, left: l.r.left, top: l.r.top} : null;
})()`);
const label = await pick();
if (!label) { console.log('SKIPPED: no label that the pointer reaches'); page.close(); running.close(); process.exit(0); }
const position = id => evaluate(`(() => { const r = document.querySelector('rect[data-label-id=${JSON.stringify(id)}]')?.getBoundingClientRect(); return r ? {left: r.left, top: r.top} : null; })()`);

await mouse('mouseMoved', label.x, label.y);
await mouse('mousePressed', label.x, label.y);
for (let k = 1; k <= 6; k++) await mouse('mouseMoved', label.x + 10 * k, label.y + 8 * k);
await sleep(200);
check('a ghost marks the place while dragging', (await evaluate(`document.querySelectorAll('.label-ghost').length`)) === 1);
await mouse('mouseReleased', label.x + 60, label.y + 48);
await sleep(2000);
const moved = await position(label.id);
const shift = moved ? {dx: Math.round(moved.left - label.left), dy: Math.round(moved.top - label.top)} : null;
check('the label moved by about the distance of the drag', shift && Math.abs(shift.dx - 60) <= 12 && Math.abs(shift.dy - 48) <= 12, JSON.stringify(shift));
check('the ghost is gone', (await evaluate(`document.querySelectorAll('.label-ghost').length`)) === 0);
await key('z', 4); await sleep(2000);
const back = await position(label.id);
check('Cmd+Z puts the label back', back && Math.abs(back.left - label.left) <= 3 && Math.abs(back.top - label.top) <= 3, JSON.stringify(back));
await key('z', 12); await sleep(2000);
const again = await position(label.id);
check('Cmd+Shift+Z puts it where it was dragged', again && moved && Math.abs(again.left - moved.left) <= 3 && Math.abs(again.top - moved.top) <= 3, JSON.stringify(again));

const failed = page.checks.filter(c => !c.ok).length;
page.close();
running.close();
console.log(failed ? `${failed} check(s) failed` : 'all checks passed');
process.exit(failed ? 1 : 0);
