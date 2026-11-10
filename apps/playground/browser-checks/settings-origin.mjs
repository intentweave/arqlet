// The layout settings of the view in a real Chrome: where each value comes from, what a free direction chose, and a value the
// engine refuses shown at its own field with the engine's reason.
// usage: node settings-origin.mjs [example=state-tcp]    (a graph with cycles: the strategy `tree` is refused)
import {launch} from './launch.mjs';
import {connect} from './cdp.mjs';

const example = process.argv[2] ?? 'state-tcp';
const running = await launch({serverPort: 8779, debugPort: 9352});
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
// The view's settings are shown when nothing is selected: select the view row.
await evaluate(`(() => { const row = document.querySelector('#containment-tree [data-id="__view__"]'); if (row) row.click(); })()`);
await sleep(800);

const infoOf = field => evaluate(`(() => {
  const label = [...document.querySelectorAll('#containment-tree label')].find(l => l.firstChild && l.firstChild.textContent.trim() === ${JSON.stringify(field)});
  const info = label?.nextElementSibling;
  return info ? {text: info.textContent.trim(), origin: info.dataset.origin ?? null} : null;
})()`);
const choose = (field, value) => evaluate(`(() => {
  const label = [...document.querySelectorAll('#containment-tree label')].find(l => l.firstChild && l.firstChild.textContent.trim() === ${JSON.stringify(field)});
  const select = label?.querySelector('select'); if (!select) return false;
  select.value = ${JSON.stringify(value)}; select.dispatchEvent(new Event('change')); return true;
})()`);

const before = await infoOf('direction');
check('the direction line names its origin', before && before.origin !== null, JSON.stringify(before));
check('the strategy line names its origin', (await infoOf('strategy'))?.origin !== null);

// 1. A refused value is shown at its field.
check('the strategy select takes the value', await choose('strategy', 'tree'));
await sleep(2500);
const problem = await evaluate(`(() => { const e = document.querySelector('#containment-tree .field-error'); return e ? {field: e.dataset.field, text: e.textContent.trim()} : null; })()`);
check('the refusal is shown at the strategy field with a reason', problem && problem.field === 'strategy' && problem.text.length > 10, JSON.stringify(problem));

// 2. A free direction is automatic, and says what it chose.
check('the direction select takes auto', await choose('direction', 'auto'));
await sleep(2500);
const after = await infoOf('direction');
check('the direction is automatic and fixed nowhere else', after && after.origin === 'auto', JSON.stringify(after));
check('the line names the orientation the engine chose', after && /chose (lr|tb)/.test(after.text), after?.text);
check('the refusal is gone after a value was accepted', (await evaluate(`document.querySelector('#containment-tree .field-error') === null`)));


// 3. The shape is chosen in the tree row of the selected node.
const leafId = await evaluate(`(() => { const g = [...document.querySelectorAll('#graph g.node')][2]; return g ? g.dataset.nodeId : null; })()`);
await evaluate(`(() => { const row = document.querySelector('#containment-tree [role=treeitem][data-id=' + JSON.stringify(${JSON.stringify(leafId)}) + ']'); row?.click(); })()`);
await sleep(600);
check('the selected row has a shape select', await evaluate(`document.querySelectorAll('#containment-tree select.tree-shape').length === 1`));
await evaluate(`(() => { const s = document.querySelector('#containment-tree select.tree-shape'); s.value = 'circle'; s.dispatchEvent(new Event('change')); })()`);
await sleep(3000);
const shaped = await evaluate(`(() => { const g = document.querySelector('g.node[data-node-id=' + JSON.stringify(${JSON.stringify(leafId)}) + ']'); return g ? g.querySelectorAll('circle, ellipse').length : 0; })()`);
check('the node is drawn as a circle after the choice', shaped > 0, String(shaped));

const failed = page.checks.filter(c => !c.ok).length;
page.close();
running.close();
console.log(failed ? `${failed} check(s) failed` : 'all checks passed');
process.exit(failed ? 1 : 0);
