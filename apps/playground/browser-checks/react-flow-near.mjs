// ADR 0025, V4: the React Flow example with React Flow as the scene view's near layer, in a real Chrome. Starts the
// playground server and Chrome (`launch.mjs`) and the example's Vite dev server pointed at that server; the scene is the
// built snapshot (`build.sh`). The GPU far layer is switched on; zooming out shows it, zooming in hands over to React
// Flow, whose nodes stand where the far layer's index puts them, and a click on one selects it. Not part of CI.
// usage: node react-flow-near.mjs   (after `pnpm install` and `sh apps/playground/build.sh`)
import {spawn} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import {dirname, join} from 'node:path';
import {launch} from './launch.mjs';
import {connect} from './cdp.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const running = await launch({serverPort: 8791, debugPort: 9371});
const vite = spawn(process.execPath, [join(here, '../../../examples/react-flow/node_modules/vite/bin/vite.js'), '--host', '127.0.0.1', '--port', '5179', '--strictPort'],
  {cwd: join(here, '../../../examples/react-flow'), env: {...process.env, ARQLET_BACKEND: running.url.replace(/\/$/, '')}, stdio: 'ignore'});
let failed = true;
try {
  for (let i = 0; i < 100; i++) { try { if ((await fetch('http://127.0.0.1:5179/')).ok) break; } catch { /* not yet */ } await new Promise(r => setTimeout(r, 200)); }
  const page = await connect(running.debugPort);
  const {evaluate, sleep, check, mouse} = page;
  await page.send('Runtime.enable');
  await page.send('Page.enable');
  await page.send('Emulation.setDeviceMetricsOverride', {width: 1400, height: 900, deviceScaleFactor: 1, mobile: false});
  await page.send('Page.navigate', {url: 'http://127.0.0.1:5179/'});
  await page.waitFor(`!!document.querySelector('header input[type=checkbox]') && !document.querySelector('header input[type=checkbox]').disabled`, 'the example loads a scene', 60000);
  await evaluate(`(() => { const c = document.querySelector('header input[type=checkbox]'); if (!c.checked) c.click(); })()`);
  await page.waitFor(`/far|near/.test(document.querySelector('.far-info')?.textContent ?? '')`, 'the scene view opens', 30000);
  const info = () => evaluate(`document.querySelector('.far-info').textContent`);
  const box = await evaluate(`(() => { const r = document.querySelector('.far-host').getBoundingClientRect(); return {x: r.left + r.width / 2, y: r.top + r.height / 2}; })()`);
  const wheel = deltaY => page.send('Input.dispatchMouseEvent', {type: 'mouseWheel', x: box.x, y: box.y, deltaX: 0, deltaY});
  let steps = 0;
  while (!/far/.test(await info()) && steps++ < 40) { await wheel(400); await sleep(80); }
  check('zoomed out, the GPU far layer draws', /far/.test(await info()) && await evaluate(`!!document.querySelector('.far-host canvas.lw-far')`), `${await info()} after ${steps} steps`);
  steps = 0;
  while (!/near/.test(await info()) && steps++ < 60) { await wheel(-240); await sleep(80); }
  await sleep(500);
  const nodes = await evaluate(`document.querySelectorAll('.far-host .react-flow__node').length`);
  check("zoomed in, React Flow takes the near layer's place", /near \(React Flow\)/.test(await info()) && nodes > 0, `${await info()}, ${nodes} React Flow nodes`);
  const target = await evaluate(`(() => {
    const host = document.querySelector('.far-host').getBoundingClientRect();
    for (const el of document.querySelectorAll('.far-host .react-flow__node')) {
      if (el.querySelector('.lw-node.container')) continue;
      const r = el.getBoundingClientRect(), x = r.left + r.width / 2, y = r.top + r.height / 2;
      if (x > host.left + 10 && x < host.right - 10 && y > host.top + 10 && y < host.bottom - 10 && el.contains(document.elementFromPoint(x, y))) return {x, y, id: el.getAttribute('data-id')};
    }
    return null;
  })()`);
  if (target) {
    await mouse('mouseMoved', target.x, target.y); await mouse('mousePressed', target.x, target.y); await mouse('mouseReleased', target.x, target.y);
    await sleep(500);
    const header = await evaluate(`document.querySelector('header span:last-of-type').textContent`);
    check('a click on a React Flow node selects it', header.startsWith(target.id + ' '), header);
  } else check('a click on a React Flow node selects it', false, 'no leaf in view');
  failed = page.checks.some(c => !c.ok);
} finally {
  vite.kill();
  running.close();
}
process.exit(failed ? 1 : 0);
