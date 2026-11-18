// ADR 0019, step 4, in a real Chrome: the cluster overview (WASM worker) drawn with JSON arrays and with typed arrays (the
// "typed arrays" box). Checks that both draw the same picture (screenshots of the stage, byte for byte) and that the box
// does what it says (the numbers of the model are typed arrays or plain ones), and prints the times the page reports
// ("... s in this browser, including transfer") for a fresh layout and for "Lay out again" in both settings. Layout time
// dominates both, so a difference here is the page's receiving and drawing side, not the engine's.
// usage: node overview-columns.mjs [reps=3] [source=code-10000]
import {createHash} from 'node:crypto';
import {launch} from './launch.mjs';
import {connect} from './cdp.mjs';

const reps = Number(process.argv[2] ?? 3);
const wanted = process.argv[3] ?? 'code-10000';
const running = await launch({serverPort: 8780, debugPort: 9360});
const page = await connect(running.debugPort);
const {evaluate, sleep, check, waitFor} = page;

await page.send('Runtime.enable');
await page.send('Page.enable');
await page.send('Emulation.setDeviceMetricsOverride', {width: 1400, height: 900, deviceScaleFactor: 1, mobile: false});
await page.send('Page.navigate', {url: `${running.url}overview.html`});
await waitFor(`!!document.querySelector('#source option[value*=${JSON.stringify(wanted)}]')`, `the source ${wanted} is offered`);
await evaluate(`(() => { const s = document.getElementById('source'); s.value = [...s.options].find(o => o.value.includes(${JSON.stringify(wanted)})).value; s.dispatchEvent(new Event('change')); document.getElementById('auto').checked = false; })()`);

const status = () => evaluate(`document.getElementById('status').textContent`);
const seconds = text => Number(/· ([\d.]+) s in this browser/.exec(text)?.[1]);

// Presses a button and waits for the layout it starts: the "Lay out" button is disabled while one runs.
async function press(id) {
  await evaluate(`document.getElementById(${JSON.stringify(id)}).click()`);
  await waitFor(`document.getElementById('run').disabled`, `the layout after ${id} starts`, 10000);
  await waitFor(`!document.getElementById('run').disabled && document.getElementById('status').textContent.includes('in this browser')`, `the layout after ${id}`, 600000);
  await sleep(1500); // the animation of a continuation ends
  const took = seconds(await status());
  console.log(`  ${id}: ${took} s`);
  return took;
}

async function picture() {
  const rect = await evaluate(`(() => { const r = document.getElementById('stage').getBoundingClientRect(); return {x: r.left, y: r.top, width: r.width, height: r.height}; })()`);
  const shot = await page.send('Page.captureScreenshot', {format: 'png', clip: {...rect, scale: 1}});
  return createHash('sha256').update(shot.data).digest('hex');
}

const results = {};
for (const columns of [false, true]) {
  const name = columns ? 'typed arrays' : 'JSON arrays';
  console.log(name);
  await evaluate(`(() => { const c = document.getElementById('columns'); c.checked = ${columns}; c.dispatchEvent(new Event('change')); })()`);
  const fresh = await press('run');
  const kinds = await evaluate(`document.body.dataset.numbers`);
  check(`${name}: the model's numbers are ${columns ? 'typed arrays' : 'plain arrays'}`, kinds === (columns ? 'Float64Array' : 'Array'), kinds);
  const hash = await picture();
  const again = [];
  for (let i = 0; i < reps; i++) again.push(await press('relayout'));
  results[name] = {fresh, again, hash, kinds, afterAgain: await picture()};
  check(`${name}: the page reports a time for a fresh layout and for each layout again`, [fresh, ...again].every(Number.isFinite), JSON.stringify({fresh, again}));
}
check('both settings draw the same picture after a fresh layout', results['JSON arrays'].hash === results['typed arrays'].hash, `${results['JSON arrays'].hash.slice(0, 12)} / ${results['typed arrays'].hash.slice(0, 12)}`);
check('and the same after "Lay out again"', results['JSON arrays'].afterAgain === results['typed arrays'].afterAgain);
const median = v => [...v].sort((a, b) => a - b)[Math.floor(v.length / 2)];
console.log(JSON.stringify({source: wanted, reps, ...Object.fromEntries(Object.entries(results).map(([k, v]) => [k, {fresh: v.fresh, againMedian: median(v.again), again: v.again}]))}));
const failed = page.checks.filter(c => !c.ok).length;
page.close();
running.close();
process.exit(failed ? 1 : 0);
