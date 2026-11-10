// A rounded routing style in a real Chrome: the drawn routes are the arcs the engine put in the scene (more points than the
// corners), and no drawn point lies inside a node. The viewer rounds nothing itself.
// usage: node rounded-routes.mjs [example=state-order]
import {launch} from './launch.mjs';
import {connect} from './cdp.mjs';

const example = process.argv[2] ?? 'state-order';
const running = await launch({serverPort: 8775, debugPort: 9348});
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

// The drawn routes: how many points each has, and how many lie inside a node (not within a unit of its border).
const routes = () => evaluate(`(() => {
  const nodes = [...document.querySelectorAll('#graph g.node')].map(g => {
    const r = g.querySelector('rect, ellipse, circle'); if (!r) return null;
    if (r.tagName === 'rect') return {x: +r.getAttribute('x'), y: +r.getAttribute('y'), w: +r.getAttribute('width'), h: +r.getAttribute('height')};
    return null;
  }).filter(Boolean);
  const lines = [...document.querySelectorAll('polyline[data-route-id]:not(.route-hit)')].filter(p => p.getAttribute('pointer-events') === 'none');
  let inside = 0, points = 0;
  for (const line of lines) {
    for (const pair of line.getAttribute('points').trim().split(/\\s+/)) {
      const [x, y] = pair.split(',').map(Number); points++;
      if (nodes.some(n => x > n.x + 1 && x < n.x + n.w - 1 && y > n.y + 1 && y < n.y + n.h - 1)) inside++;
    }
  }
  return {routes: lines.length, points, inside, nodes: nodes.length};
})()`);

const plain = await routes();
check('routes are drawn', plain.routes > 0, JSON.stringify(plain));
const chosen = await evaluate(`(() => {
  const select = [...document.querySelectorAll('#containment-tree select')].find(s => [...s.options].some(o => o.value === 'roundedOrthogonal'));
  if (!select) return false;
  select.value = 'roundedOrthogonal'; select.dispatchEvent(new Event('change'));
  return true;
})()`);
check('the routing select offers the rounded style', chosen);
await sleep(2500);
const rounded = await routes();
check('the same routes are drawn', rounded.routes === plain.routes, JSON.stringify({plain, rounded}));
check('the rounded routes have the points of their arcs', rounded.points > plain.points, `${plain.points} -> ${rounded.points}`);
check('no drawn point lies inside a node', rounded.inside === 0, `${rounded.inside} of ${rounded.points}`);

const failed = page.checks.filter(c => !c.ok).length;
page.close();
running.close();
console.log(failed ? `${failed} check(s) failed` : 'all checks passed');
process.exit(failed ? 1 : 0);
