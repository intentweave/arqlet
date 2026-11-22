// ADR 0025 O4: the continuous zoom from the cluster overview into the scene view and back, in a real Chrome with real
// wheel input. The overview lays out a graph; a click structures the cluster nearest the middle, zooming in there makes
// its disc cover the view and hands over to the scene view. At that moment the scene view and the overview
// under it must show the cluster in the same place (the two pictures compared); zooming out hands back. Starts its own
// server and Chrome. Not part of CI.
// usage: node overview-zoom.mjs [source=vehicle-network]
import {writeFileSync, mkdtempSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {launch} from './launch.mjs';
import {connect} from './cdp.mjs';

const wanted = process.argv[2] ?? 'vehicle-network';
const out = process.env.OUT ?? mkdtempSync(join(tmpdir(), 'lw-overview-zoom-'));
const running = await launch({serverPort: 8794, debugPort: 9374});
let failed = true;
try {
  const page = await connect(running.debugPort);
  const {evaluate, sleep, check, waitFor} = page;
  await page.send('Runtime.enable');
  await page.send('Page.enable');
  await page.send('Emulation.setDeviceMetricsOverride', {width: 1400, height: 900, deviceScaleFactor: 1, mobile: false});
  await page.send('Page.navigate', {url: `${running.url}overview.html`});
  await waitFor(`!!document.querySelector('#source option[value*=${JSON.stringify(wanted)}]')`, `the source ${wanted} is offered`);
  await evaluate(`(() => { const s = document.getElementById('source'); s.value = [...s.options].find(o => o.value.includes(${JSON.stringify(wanted)})).value; s.dispatchEvent(new Event('change')); })()`);
  await evaluate(`document.getElementById('run').click()`);
  await waitFor(`!document.getElementById('run').disabled && document.getElementById('status').textContent.includes('in this browser')`, 'the layout', 600000);
  await sleep(1500);
  const stage = await evaluate(`(() => { const r = document.getElementById('stage').getBoundingClientRect(); return {x: r.left, y: r.top, width: r.width, height: r.height}; })()`);
  // A drawn cluster near the middle: the coloured (node) pixel nearest to it in a picture of the stage; a double click there zooms
  // the overview onto that cluster (the page's own gesture), then the wheel goes on at the middle.
  const first = await page.send('Page.captureScreenshot', {format: 'png', clip: {...stage, scale: 1}});
  const spot = await evaluate(`(async () => {
    const img = new Image(); img.src = 'data:image/png;base64,' + ${JSON.stringify(first.data)}; await img.decode();
    const c = document.createElement('canvas'); c.width = img.width; c.height = img.height; const x = c.getContext('2d', {willReadFrequently: true}); x.drawImage(img, 0, 0);
    const d = x.getImageData(0, 0, c.width, c.height).data; let best = null;
    for (let y = 40; y < c.height - 40; y += 3) for (let i = 40; i < c.width - 40; i += 3) { const k = (y * c.width + i) * 4; if (Math.max(d[k], d[k + 1], d[k + 2]) - Math.min(d[k], d[k + 1], d[k + 2]) > 90 && Math.max(d[k], d[k + 1], d[k + 2]) > 150) { const dist = Math.hypot(i - c.width / 2, y - c.height / 2); if (!best || dist < best.dist) best = {x: i, y, dist}; } }
    return best;
  })()`);
  if (process.env.DEBUG) console.log('spot', JSON.stringify(spot));
  if (spot) {
    const sx = stage.x + spot.x, sy = stage.y + spot.y;
    // A click selects the cluster; the panel's button structures it; then the wheel zooms in at that point.
    await page.send('Input.dispatchMouseEvent', {type: 'mouseMoved', x: sx, y: sy});
    for (const type of ['mousePressed', 'mouseReleased']) await page.send('Input.dispatchMouseEvent', {type, x: sx, y: sy, button: 'left', clickCount: 1});
    await sleep(800);
    // The panel's button structures the selected cluster (its layered layout in place).
    await evaluate(`document.querySelector('button[data-rep="structured"]')?.click()`);
    await sleep(3000);
  }
  const cx = spot ? stage.x + spot.x : stage.x + stage.width / 2, cy = spot ? stage.y + spot.y : stage.y + stage.height / 2;
  const wheel = deltaY => page.send('Input.dispatchMouseEvent', {type: 'mouseWheel', x: cx, y: cy, deltaX: 0, deltaY});
  let steps = 0;
  const snap = async name => { if (!process.env.DEBUG) return; const s = await page.send('Page.captureScreenshot', {format: 'png', clip: {...stage, scale: 1}}); writeFileSync(join(out, name + '.png'), Buffer.from(s.data, 'base64')); console.log(name, await evaluate(`document.getElementById('status').textContent`)); };
  await snap('start');
  while (!(await evaluate(`!!document.getElementById('scene-zoom')`)) && steps++ < 80) { await wheel(-160); await sleep(250); if ([3, 8, 15, 25].includes(steps)) await snap('step-' + steps); }
  const handed = await evaluate(`!!document.getElementById('scene-zoom')`);
  if (!handed) { const s = await page.send('Page.captureScreenshot', {format: 'png', clip: {...stage, scale: 1}}); writeFileSync(join(out, 'no-hand-over.png'), Buffer.from(s.data, 'base64')); }
  check('zooming into a structured cluster hands over to the scene view', handed, `${steps} wheel steps; ${await evaluate(`document.getElementById('status').textContent`)}`);
  if (handed) {
    await sleep(800);
    const shoot = async name => { const s = await page.send('Page.captureScreenshot', {format: 'png', clip: {...stage, scale: 1}}); writeFileSync(join(out, `${name}.png`), Buffer.from(s.data, 'base64')); return s.data; };
    const scenePicture = await shoot('scene-view');
    await evaluate(`document.getElementById('scene-zoom').style.visibility = 'hidden'`);
    await sleep(300);
    const overviewPicture = await shoot('overview');
    await evaluate(`document.getElementById('scene-zoom').style.visibility = ''`);
    // Compare the two pictures in the page (decode both PNGs on a canvas).
    const share = await evaluate(`(async () => {
      const load = async b64 => { const img = new Image(); img.src = 'data:image/png;base64,' + b64; await img.decode(); const c = document.createElement('canvas'); c.width = img.width; c.height = img.height; const x = c.getContext('2d', {willReadFrequently: true}); x.drawImage(img, 0, 0); return x.getImageData(0, 0, c.width, c.height).data; };
      const a = await load(${JSON.stringify(scenePicture)}), b = await load(${JSON.stringify(overviewPicture)});
      // Where either picture draws something (not background): the share of those pixels that differ strongly.
      let drawn = 0, off = 0;
      for (let i = 0; i < a.length; i += 4) {
        const ink = p => p[i] + p[i + 1] + p[i + 2] > 3 * 40;
        if (!ink(a) && !ink(b)) continue;
        drawn++;
        if (Math.max(Math.abs(a[i] - b[i]), Math.abs(a[i + 1] - b[i + 1]), Math.abs(a[i + 2] - b[i + 2])) > 64) off++;
      }
      return {drawn, off};
    })()`);
    check('at the hand-over the scene view shows the cluster where the overview does', share.drawn > 1000 && share.off / share.drawn < 0.2, `${share.off} of ${share.drawn} drawn pixels differ strongly (${(100 * share.off / Math.max(share.drawn, 1)).toFixed(1)} %)`);
    steps = 0;
    while ((await evaluate(`!!document.getElementById('scene-zoom')`)) && steps++ < 60) { await wheel(200); await sleep(150); }
    await sleep(400);
    check('zooming out hands back to the overview', !(await evaluate(`!!document.getElementById('scene-zoom')`)) && /back in the overview/.test(await evaluate(`document.getElementById('status').textContent`)), `${steps} wheel steps`);
  }
  console.log(`pictures in ${out}`);
  failed = page.checks.some(c => !c.ok);
} finally {
  running.close();
}
process.exit(failed ? 1 : 0);
