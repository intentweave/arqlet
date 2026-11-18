// ADR 0019, step 4: drawing the scene with the SVG viewer in a real Chrome, from the JSON scene and from the columns.
// In the page (main thread, real DOM, engine called directly): the engine call, the viewer's `renderSvg`, and the time until
// the second animation frame after it (style, layout and paint). Per view: the JSON scene (`run({op: 'scene'})`, parsed) and
// the columns (`columns()`, views into the engine's memory, turned into the scene by `renderSvg` through `sceneOfColumns`).
// The parts are timed alone too: `sceneOfColumns`, and `renderSvg` on a scene object made beforehand. Writes
// benchmarks/results/scene-draw-<UTC>.jsonl (machine line, then one line per view and path).
// usage: node scene-draw.mjs [reps=5] [source=code-10000]
import {writeFileSync} from 'node:fs';
import {execSync} from 'node:child_process';
import os from 'node:os';
import {launch} from './launch.mjs';
import {connect} from './cdp.mjs';

const reps = Number(process.argv[2] ?? 5);
const wanted = process.argv[3] ?? 'code-10000';
// Waits (at most WAIT seconds, default 900) for a one-minute load average of MAXLOAD (default 4) or less, then measures anyway.
const maxLoad = Number(process.env.MAXLOAD ?? 4), until = Date.now() + Number(process.env.WAIT ?? 900) * 1000;
while (os.loadavg()[0] > maxLoad && Date.now() < until) await new Promise(r => setTimeout(r, 10000));
const loadAtStart = os.loadavg()[0];
const running = await launch({serverPort: 8782, debugPort: 9362});
const page = await connect(running.debugPort);
const {evaluate, check} = page;
await page.send('Runtime.enable');
await page.send('Page.enable');
await page.send('Emulation.setDeviceMetricsOverride', {width: 1400, height: 900, deviceScaleFactor: 1, mobile: false});
await page.send('Page.navigate', {url: `${running.url}overview.html`});
await page.waitFor(`document.readyState === 'complete'`, 'the page loads', 30000);
const browser = (await page.send('Browser.getVersion')).product;

const result = await evaluate(`(async () => {
  const reps = ${reps};
  const {renderSvg} = await import('/packages/viewer/svg.js');
  const {sceneOfColumns} = await import('/packages/viewer/scene-columns.js');
  const {createEngine} = await import('/packages/engine-client/wasm-client.js');
  const run = await createEngine(await (await fetch('/generated/arqlet.wasm')).arrayBuffer());
  const index = await (await fetch('/generated/overview/index.json')).json();
  const file = index.find(d => d.file.includes(${JSON.stringify(wanted)})).file;
  const graph = await (await fetch('/generated/overview/' + file)).json();
  const host = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
  host.style.cssText = 'position:fixed;left:0;top:0;width:1400px;height:900px;background:#0e1520;z-index:99';
  document.body.append(host);
  const frames = async n => { for (let i = 0; i < n; i++) await new Promise(r => requestAnimationFrame(() => r())); };
  const median = v => [...v].sort((a, b) => a - b)[Math.floor(v.length / 2)];
  const out = [];
  for (const view of ['lod', 'full']) {
    const session = run.openSession();
    const limit = Math.min(graph.nodes.length, 10000);
    const imported = session.run({op: 'import', graph, limit, scene: false, ...(view === 'lod' ? {options: {levelOfDetail: {depth: 1}}} : {})});
    if (!imported.ok) throw Error(JSON.stringify(imported).slice(0, 200));
    if (view === 'lod') {
      const snapshot = session.run({op: 'export'}).snapshot;
      const visible = new Set(snapshot.graph.nodes.map(n => n.id));
      const container = snapshot.collapsedIds.find(c => visible.has(c));
      session.run({op: 'toggle', nodeId: container, scene: false});
    }
    const shape = (() => { const s = session.run({op: 'scene'}).scene; return {boxes: s.nodes.length, routes: s.routes.length}; })();
    const times = {json: [], columns: []}, parts = {engineJson: [], engineColumns: [], materialize: [], drawObject: [], drawColumns: [], drawJson: []};
    const round = async (path) => {
      host.replaceChildren();
      await frames(1);
      const t0 = performance.now();
      const data = path === 'json' ? session.run({op: 'scene'}).scene : session.columns();
      const t1 = performance.now();
      renderSvg(host, data);
      const t2 = performance.now();
      await frames(2);
      const t3 = performance.now();
      return {engine: t1 - t0, draw: t2 - t1, paint: t3 - t2, total: t3 - t0, elements: host.querySelectorAll('*').length};
    };
    for (let i = 0; i < reps + 1; i++) {
      for (const path of ['json', 'columns']) {
        const r = await round(path);
        if (i === 0) continue; // a warm-up of each path
        times[path].push(r);
        parts[path === 'json' ? 'engineJson' : 'engineColumns'].push(r.engine);
        parts[path === 'json' ? 'drawJson' : 'drawColumns'].push(r.draw);
      }
      // The parts alone.
      const columns = session.columns();
      const t = performance.now();
      const scene = sceneOfColumns(columns);
      const m = performance.now() - t;
      host.replaceChildren();
      const d0 = performance.now();
      renderSvg(host, scene);
      const d = performance.now() - d0;
      if (i > 0) { parts.materialize.push(m); parts.drawObject.push(d); }
    }
    const pick = key => Object.fromEntries(['engine', 'draw', 'paint', 'total'].map(k => [k, median(times[key].map(r => r[k]))]));
    out.push({view, nodes: graph.nodes.length, ...shape, reps, json: pick('json'), columns: pick('columns'),
      parts: Object.fromEntries(Object.entries(parts).map(([k, v]) => [k, median(v)])),
      elements: {json: times.json[0].elements, columns: times.columns[0].elements}});
    session.close();
  }
  host.remove();
  return out;
})()`);

const stamp = new Date().toISOString().replace(/[-:]/g, '').replace(/\.\d+Z$/, 'Z');
const git = cmd => { try { return execSync(`git ${cmd}`, {encoding: 'utf8'}).trim(); } catch { return ''; } };
const machine = {machine: os.cpus()[0].model, os: `${os.type()} ${os.release()}`, cores: os.cpus().length, browser, node: process.version,
  revision: git('rev-parse --short HEAD'), dirty: Boolean(git('status --porcelain --untracked-files=no')), load: Number(loadAtStart.toFixed(1)), loadAtEnd: Number(os.loadavg()[0].toFixed(1)), utc: new Date().toISOString(), source: wanted, reps};
const lines = [JSON.stringify(machine), ...result.map(r => JSON.stringify(r))];
const out = new URL(`../../../benchmarks/results/scene-draw-${stamp}.jsonl`, import.meta.url);
writeFileSync(out, lines.join('\n') + '\n');
for (const r of result) {
  console.log(`${r.view}: ${r.boxes} boxes, ${r.routes} routes, ${r.elements.json} elements`);
  for (const path of ['json', 'columns']) console.log(`  ${path.padEnd(7)} engine ${r[path].engine.toFixed(1)}  renderSvg ${r[path].draw.toFixed(1)}  to paint ${r[path].paint.toFixed(1)}  total ${r[path].total.toFixed(1)} ms`);
  console.log(`  parts: sceneOfColumns ${r.parts.materialize.toFixed(1)} ms, renderSvg on a scene object ${r.parts.drawObject.toFixed(1)} ms`);
}
check('both paths drew the same number of elements', result.every(r => r.elements.json > 0 && r.elements.json === r.elements.columns), JSON.stringify(result.map(r => r.elements)));
console.log(out.pathname);
const failed = page.checks.filter(c => !c.ok).length;
page.close();
running.close();
process.exit(failed ? 1 : 0);
