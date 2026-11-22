// ADR 0025, V4: the playground's large-view mode in a real Chrome with real wheel and mouse input. A large example opens
// in the scene view by itself (GPU from afar); zooming in at the pointer hands over to the SVG near layer, where a real
// click on a node selects it; zooming out hands back to the far layer, where a click selects what the picking finds;
// G returns to the plain SVG view. Starts its own server and Chrome. Not part of CI.
// usage: node large-view.mjs [example=calls-flat]
import {launch} from './launch.mjs';
import {connect} from './cdp.mjs';

const example = process.argv[2] ?? 'calls-flat';
const running = await launch({serverPort: 8787, debugPort: 9367});
let failed = true;
try {
  const page = await connect(running.debugPort);
  const {evaluate, sleep, check, mouse, key} = page;
  await page.send('Runtime.enable');
  await page.send('Page.enable');
  await page.send('Emulation.setDeviceMetricsOverride', {width: 1400, height: 900, deviceScaleFactor: 1, mobile: false});
  await page.send('Page.navigate', {url: running.url});
  await sleep(3000);
  await page.waitForExample(example);
  await evaluate(`(() => { const s = document.getElementById('example'); s.value = ${JSON.stringify(example)}; s.dispatchEvent(new Event('change')); })()`);
  await page.waitFor(`!!document.querySelector('#scene-view canvas.lw-far') && /far|near/.test(document.getElementById('layer-info').textContent)`, 'the scene view opens', 120000);
  const info = () => evaluate(`document.getElementById('layer-info').textContent`);
  const box = await evaluate(`(() => { const r = document.getElementById('scene-view').getBoundingClientRect(); return {x: r.left + r.width / 2, y: r.top + r.height / 2}; })()`);
  check('a large example opens in the scene view by itself', /far/.test(await info()) && await evaluate(`document.getElementById('graph').hidden`), await info());
  const wheel = deltaY => page.send('Input.dispatchMouseEvent', {type: 'mouseWheel', x: box.x, y: box.y, deltaX: 0, deltaY});
  let steps = 0;
  while (!/near/.test(await info()) && steps++ < 60) { await wheel(-240); await sleep(60); }
  check('zooming in hands over to the SVG near layer', /near/.test(await info()), `${await info()} after ${steps} wheel steps`);
  await sleep(400);
  // The view may hold only lines; a real drag then brings the nearest node of the near layer to the middle.
  const nearest = await evaluate(`(() => {
    const h = document.getElementById('scene-view').getBoundingClientRect(), cx = h.left + h.width / 2, cy = h.top + h.height / 2;
    let best = null;
    for (const g of document.querySelectorAll('#scene-view svg.lw-near g.node[data-node-id]')) {
      const r = g.querySelector('rect:not(.lw-hit), circle').getBoundingClientRect(), x = r.left + r.width / 2, y = r.top + r.height / 2;
      const d = Math.hypot(x - cx, y - cy);
      if (!best || d < best.d) best = {x, y, d, cx, cy};
    }
    return best;
  })()`);
  if (nearest && nearest.d > 100) {
    await mouse('mouseMoved', nearest.cx, nearest.cy); await mouse('mousePressed', nearest.cx, nearest.cy);
    for (let i = 1; i <= 10; i++) await mouse('mouseMoved', nearest.cx + (nearest.cx - nearest.x) * i / 10, nearest.cy + (nearest.cy - nearest.y) * i / 10);
    await mouse('mouseReleased', nearest.cx * 2 - nearest.x, nearest.cy * 2 - nearest.y);
    await sleep(500);
  }
  const target = await evaluate(`(() => {
    const host = document.getElementById('scene-view').getBoundingClientRect();
    for (const g of document.querySelectorAll('#scene-view svg.lw-near g.node[data-node-id]')) {
      const r = g.querySelector('rect:not(.lw-hit), circle').getBoundingClientRect();
      const x = r.left + r.width / 2, y = r.top + r.height / 2;
      if (x > host.left + 20 && x < host.right - 20 && y > host.top + 20 && y < host.bottom - 20 && document.elementFromPoint(x, y)?.closest('g.node') === g) return {x, y, id: g.getAttribute('data-node-id'), label: g.getAttribute('aria-label')};
    }
    return null;
  })()`);
  if (target) {
    await mouse('mouseMoved', target.x, target.y); await mouse('mousePressed', target.x, target.y); await mouse('mouseReleased', target.x, target.y);
    await sleep(500);
    const shown = await evaluate(`document.getElementById('selected').textContent`);
    check('a click in the near layer selects the node', shown === target.label, `${target.id}: shows "${shown}"`);
  } else {
    const seen = await evaluate(`(() => { const all = [...document.querySelectorAll('#scene-view svg.lw-near g.node[data-node-id]')]; const h = document.getElementById('scene-view').getBoundingClientRect(); const r = all[0]?.querySelector('rect:not(.lw-hit), circle')?.getBoundingClientRect(); const at = r && document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2); return all.length + ' nodes in the near layer; first at ' + (r ? [r.left, r.top, r.width, r.height].map(Math.round).join(',') : '-') + ' host ' + [h.left, h.top, h.width, h.height].map(Math.round).join(',') + ' hit ' + (at ? at.tagName + '.' + (at.getAttribute('class') ?? '') : '-'); })()`);
    check('a click in the near layer selects the node', false, seen);
  }
  steps = 0;
  while (!/far/.test(await info()) && steps++ < 60) { await wheel(240); await sleep(60); }
  check('zooming out hands back to the far layer', /far/.test(await info()), `${await info()} after ${steps} wheel steps`);
  await evaluate(`document.getElementById('fit').click()`);
  await sleep(300);
  // From afar a click may find a node or a line (5 pixels of tolerance); either shows in the details.
  const shownSelection = () => evaluate(`document.getElementById('selected').textContent + ' | ' + document.getElementById('route-info').textContent`);
  const before = await shownSelection();
  // A point on a drawn box: the far layer's index answers which one; the page must show it.
  const far = await evaluate(`(() => {
    const c = document.querySelector('#scene-view canvas.lw-far'), r = c.getBoundingClientRect();
    const copy = document.createElement('canvas'); copy.width = c.width; copy.height = c.height;
    return {left: r.left, top: r.top, width: r.width, height: r.height};
  })()`);
  let picked = false;
  for (let i = 1; i < 12 && !picked; i++) {
    const x = far.left + far.width * (0.2 + 0.05 * i), y = far.top + far.height * 0.5;
    await mouse('mouseMoved', x, y); await mouse('mousePressed', x, y); await mouse('mouseReleased', x, y);
    await sleep(400);
    picked = (await shownSelection()) !== before;
  }
  check('a click in the far layer selects what the picking finds', picked, (await shownSelection()).slice(0, 120));
  await key('g');
  await sleep(1500);
  const plain = await evaluate(`({hidden: document.getElementById('scene-view').hidden, nodes: document.querySelectorAll('#graph g.node').length})`);
  check('G returns to the plain SVG view', plain.hidden && plain.nodes > 0, `${plain.nodes} nodes drawn`);
  failed = page.checks.some(c => !c.ok);
} finally {
  running.close();
}
process.exit(failed ? 1 : 0);
