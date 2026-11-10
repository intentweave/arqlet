// After a drag in an organic area with force-directed behaviour the other nodes settle: a few bounded engine calls, and the
// status says whether the placement converged. Real mouse input in a real Chrome.
// usage: node settle.mjs [example=mixed-layouts] [node=cache]
import {launch} from './launch.mjs';
import {connect} from './cdp.mjs';

const example = process.argv[2] ?? 'mixed-layouts';
const nodeId = process.argv[3] ?? 'cache';
const running = await launch({serverPort: 8774, debugPort: 9347});
const page = await connect(running.debugPort);
const {evaluate, mouse, sleep, check} = page;

await page.send('Runtime.enable');
await page.send('Page.enable');
await page.send('Emulation.setDeviceMetricsOverride', {width: 1400, height: 900, deviceScaleFactor: 1, mobile: false});
await page.send('Page.addScriptToEvaluateOnNewDocument', {source: `(() => { window.__ops = []; const O = window.Worker; window.Worker = class extends O { postMessage(m, ...r) { if (m?.request?.op) __ops.push(m.request.op); return super.postMessage(m, ...r); } }; })();`});
await page.send('Page.navigate', {url: running.url});
await sleep(3000);
await page.waitForExample(example);
await evaluate(`(async () => {
  const s = document.getElementById('example'); s.value = ${JSON.stringify(example)}; s.dispatchEvent(new Event('change'));
  await new Promise(r => setTimeout(r, 3000));
  const f = document.getElementById('force-directed'); f.checked = true; f.dispatchEvent(new Event('change'));
  await new Promise(r => setTimeout(r, 2500));
})()`);
await page.requireExample(example);

const pick = await evaluate(`(() => {
  const g = document.querySelector('g.node[data-node-id=${JSON.stringify(nodeId)}]');
  if (!g) return null;
  const r = g.getBoundingClientRect();
  return {x: r.left + r.width / 2, y: r.top + r.height / 2, covered: document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2)?.closest('g.node')?.dataset.nodeId};
})()`);
if (!pick || pick.covered !== nodeId) { console.log('SKIPPED: the node is not reachable', JSON.stringify(pick)); page.close(); running.close(); process.exit(0); }
check('force-directed is on', await evaluate(`document.getElementById('force-directed').checked`));
await evaluate(`__ops.length = 0`);
await mouse('mouseMoved', pick.x, pick.y);
await mouse('mousePressed', pick.x, pick.y);
for (let k = 1; k <= 6; k++) await mouse('mouseMoved', pick.x + 12 * k, pick.y + 6 * k);
await mouse('mouseReleased', pick.x + 72, pick.y + 36);
await sleep(4000);
const status = await evaluate(`document.getElementById('status').textContent`);
const ops = await evaluate(`__ops`);
check('the engine was asked to settle', ops.includes('settle'), ops.filter(o => o !== 'move').join(','));
check('the status reports the outcome', /^Settled: (converged|refined)/.test(status), status);
const settleCalls = ops.filter(o => o === 'settle').length;
check('at most three settling calls', settleCalls >= 1 && settleCalls <= 3, String(settleCalls));

const failed = page.checks.filter(c => !c.ok).length;
page.close();
running.close();
console.log(failed ? `${failed} check(s) failed` : 'all checks passed');
process.exit(failed ? 1 : 0);
