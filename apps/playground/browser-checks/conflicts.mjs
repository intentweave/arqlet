// A conflict in the drawing, in a real Chrome: two pins on the same place of two siblings overlap them. Both nodes are marked,
// the panel lists the conflict with both parties, and a click on the entry selects them.
// usage: node conflicts.mjs [example=state-order]
import {launch} from './launch.mjs';
import {connect} from './cdp.mjs';

const example = process.argv[2] ?? 'state-order';
const running = await launch({serverPort: 8778, debugPort: 9351});
const page = await connect(running.debugPort);
const {evaluate, sleep, check} = page;

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

check('a clean drawing shows no conflict panel', await evaluate(`document.getElementById('conflicts').hidden`));
const ids = await evaluate(`[...document.querySelectorAll('#graph g.node')].map(g => g.dataset.nodeId).slice(0, 2)`);
// Both pins on the same place of the same area: the nodes overlap.
const pins = Object.fromEntries(ids.map(id => [id, {x: 200, y: 200}]));
await evaluate(`(() => { document.getElementById('options-json').value = ${JSON.stringify(JSON.stringify({pins}))}; document.getElementById('apply-options').click(); })()`);
await sleep(3000);
const panel = await evaluate(`(() => { const p = document.getElementById('conflicts'); return {hidden: p.hidden, text: p.textContent, items: p.querySelectorAll('.conflict-item').length}; })()`);
check('the panel shows the conflicts', !panel.hidden && panel.items >= 1, panel.text.slice(0, 160));
check('the panel says how many are shown', /Conflicts: \d+ of \d+ shown/.test(panel.text));
const marks = await evaluate(`${JSON.stringify(ids)}.map(id => { const g = document.querySelector('g.node[data-node-id=' + JSON.stringify(id) + ']'); return g ? [...g.querySelectorAll('[stroke]')].some(e => e.getAttribute('stroke') === '#f08f80') : null; })`);
check('both nodes of the overlap are marked', marks.every(Boolean), JSON.stringify(marks));
await evaluate(`document.querySelector('#conflicts .conflict-item').click()`);
await sleep(800);
const selected = await evaluate(`document.getElementById('selected').textContent`);
check('a click on an entry selects a party', !/Select a node/.test(selected), JSON.stringify(selected));

const failed = page.checks.filter(c => !c.ok).length;
page.close();
running.close();
console.log(failed ? `${failed} check(s) failed` : 'all checks passed');
process.exit(failed ? 1 : 0);
