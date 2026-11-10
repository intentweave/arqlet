// The new catalogue models in a real Chrome: each loads, the page says what it is, where it comes from and how much is visible,
// and the self-loop model draws its routes. The placed ports are checked in the engine (explicit_constraints.rs).
// usage: node catalogue.mjs
import {launch} from './launch.mjs';
import {connect} from './cdp.mjs';

const running = await launch({serverPort: 8780, debugPort: 9353});
const page = await connect(running.debugPort);
const {evaluate, sleep, check} = page;

await page.send('Runtime.enable');
await page.send('Page.enable');
await page.send('Emulation.setDeviceMetricsOverride', {width: 1400, height: 900, deviceScaleFactor: 1, mobile: false});
await page.send('Page.navigate', {url: running.url});
await sleep(3000);
await page.waitForExample('mixed-large');

const load = async (name, atLeast = 6) => {
  await evaluate(`(async () => {
    const s = document.getElementById('example'); s.value = ${JSON.stringify(name)}; s.dispatchEvent(new Event('change'));
    await new Promise(r => setTimeout(r, 6000));
  })()`);
  await page.requireExample(name, atLeast);
};

for (const [name, nodes] of [['mixed-large', 96], ['self-loops', 9], ['fixed-ports', 5]]) {
  await load(name, Math.min(nodes, 5));
  const info = await evaluate(`document.getElementById('example-info').textContent`);
  check(`${name}: the page says what it is, its source and the counts`, info.includes('Source:') && info.includes(`${nodes} nodes in the model`) && /visible\./.test(info), info.slice(0, 200));
  const drawn = await evaluate(`document.querySelectorAll('#graph g.node, #graph [data-container-id]').length`);
  check(`${name}: the drawing has its nodes`, drawn >= nodes, String(drawn));
}

await load('self-loops');
const loops = await evaluate(`(() => {
  const routes = [...document.querySelectorAll('#graph polyline[data-route-id]')].filter(p => p.getAttribute('pointer-events') !== 'stroke' || true);
  return routes.length;
})()`);
check('self-loops: the routes are drawn', loops > 0, String(loops));

const failed = page.checks.filter(c => !c.ok).length;
page.close();
running.close();
console.log(failed ? `${failed} check(s) failed` : 'all checks passed');
process.exit(failed ? 1 : 0);
