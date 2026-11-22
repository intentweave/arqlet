// ADR 0025, the measurement of V1: frame times while panning and zooming a large scene, SVG against the WebGL far layer,
// in a real Chrome on this machine. Starts its own server and Chrome (`launch.mjs`). Not part of CI.
//
// Per view size (the generated hierarchical graph `code-10000`, imported with that many nodes and laid out by the WASM
// engine in the page): the same scripted motion of FRAMES animation frames (a zoom from the whole drawing to 8 times in,
// a pan, and back) drawn by
// - svg: `renderSvg` once, then a CSS transform per frame (the cheapest way to move an SVG drawing; the playground
//   draws the SVG again on every zoom step, which costs more);
// - webgpu and webgl2: the far layer with either backend, `setView` and `draw` per frame, texts on the 2D canvas included.
// The time between animation frames is recorded (median, 95th percentile, maximum, frames over 33 ms), with the
// browser, the GPU string and the machine. Writes benchmarks/results/scene-frames-<UTC>.jsonl.
// usage: node scene-frames.mjs [sizes=1000,3000,10000] [frames=240]
import {writeFileSync} from 'node:fs';
import {execSync} from 'node:child_process';
import os from 'node:os';
import {launch} from './launch.mjs';
import {connect} from './cdp.mjs';

const sizes = (process.argv[2] ?? '1000,3000,10000').split(',').map(Number);
const frames = Number(process.argv[3] ?? 240);
const maxLoad = Number(process.env.MAXLOAD ?? 4), until = Date.now() + Number(process.env.WAIT ?? 900) * 1000;
while (os.loadavg()[0] > maxLoad && Date.now() < until) await new Promise(r => setTimeout(r, 10000));
const loadAtStart = os.loadavg()[0];
const running = await launch({serverPort: 8785, debugPort: 9365});
try {
  const page = await connect(running.debugPort);
  await page.send('Runtime.enable');
  await page.send('Page.enable');
  await page.send('Emulation.setDeviceMetricsOverride', {width: 1400, height: 900, deviceScaleFactor: 1, mobile: false});
  await page.send('Page.navigate', {url: `${running.url}overview.html`});
  await page.waitFor(`document.readyState === 'complete'`, 'the page loads', 30000);
  const browser = (await page.send('Browser.getVersion')).product;
  const result = await page.evaluate(`(async () => {
    const sizes = ${JSON.stringify(sizes)}, FRAMES = ${frames};
    const {renderSvg} = await import('/packages/viewer/svg.js');
    const {createSceneRenderer} = await import('/packages/viewer/scene-gl.js');
    const {createSceneGpuRenderer} = await import('/packages/viewer/scene-gpu.js');
    const {sceneBackend} = await import('/packages/viewer/scene-renderer.js');
    const backend = await sceneBackend();
    const adapterInfo = backend.device?.adapterInfo ? JSON.stringify({vendor: backend.device.adapterInfo.vendor, architecture: backend.device.adapterInfo.architecture}) : null;
    const {createEngine} = await import('/packages/engine-client/wasm-client.js');
    const run = await createEngine(await (await fetch('/generated/arqlet.wasm')).arrayBuffer());
    const graph = await (await fetch('/generated/overview/code-10000.json')).json();
    const probe = document.createElement('canvas').getContext('webgl2');
    const info = probe.getExtension('WEBGL_debug_renderer_info');
    const gpu = info ? probe.getParameter(info.UNMASKED_RENDERER_WEBGL) : probe.getParameter(probe.RENDERER);
    const W = 1400, H = 900;
    const host = document.createElement('div');
    host.style.cssText = 'position:fixed;left:0;top:0;width:' + W + 'px;height:' + H + 'px;overflow:hidden;background:#0e1520;z-index:99';
    document.body.append(host);
    const nextFrame = () => new Promise(r => requestAnimationFrame(r));
    const stats = times => {
      const s = [...times].sort((a, b) => a - b), at = q => s[Math.min(s.length - 1, Math.floor(q * s.length))];
      return {median: +at(0.5).toFixed(2), p95: +at(0.95).toFixed(2), max: +s[s.length - 1].toFixed(2), over33: times.filter(t => t > 33.4).length};
    };
    // The motion: frame i of FRAMES, as a view relative to the whole drawing.
    const motion = (b, fit, i) => {
      const t = i / (FRAMES - 1), zoomT = t < 0.5 ? t * 2 : (1 - t) * 2;
      const scale = fit * Math.pow(8, zoomT);
      const angle = t * Math.PI * 2;
      return {centreX: b.x + b.width / 2 + Math.cos(angle) * b.width * 0.2 * zoomT, centreY: b.y + b.height / 2 + Math.sin(angle) * b.height * 0.2 * zoomT, scale};
    };
    const out = [];
    for (const size of sizes) {
      const session = run.openSession();
      const imported = session.run({op: 'import', graph, limit: size, scene: false});
      if (!imported.ok) throw Error(JSON.stringify(imported).slice(0, 200));
      const scene = session.run({op: 'scene'}).scene;
      const b = scene.bounds, fit = Math.min(W / b.width, H / b.height) * 0.95;
      const shape = {boxes: scene.nodes.length, routes: scene.routes.length, segments: scene.routes.reduce((s, r) => s + r.points.length - 1, 0)};
      // SVG
      host.replaceChildren();
      const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
      svg.style.cssText = 'position:absolute;left:0;top:0;transform-origin:0 0';
      host.append(svg);
      let t0 = performance.now();
      renderSvg(svg, scene, {scale: 1, document});
      await nextFrame(); await nextFrame();
      const svgFirst = performance.now() - t0;
      const svgTimes = [];
      let last = performance.now();
      for (let i = 0; i < FRAMES; i++) {
        const v = motion(b, fit, i);
        svg.style.transform = 'translate(' + (W / 2 - (v.centreX - b.x) * v.scale) + 'px,' + (H / 2 - (v.centreY - b.y) * v.scale) + 'px) scale(' + v.scale + ')';
        await nextFrame();
        const now = performance.now(); svgTimes.push(now - last); last = now;
      }
      const far = {};
      for (const kind of [backend.kind === 'webgpu' ? 'webgpu' : null, 'webgl2'].filter(Boolean)) {
        host.replaceChildren();
        const canvas = document.createElement('canvas'), labels = document.createElement('canvas');
        for (const c of [canvas, labels]) { c.style.cssText = 'position:absolute;left:0;top:0;width:100%;height:100%'; host.append(c); }
        t0 = performance.now();
        const options = {labels, view: motion(b, fit, 0)};
        const r = kind === 'webgpu' ? createSceneGpuRenderer(canvas, scene, {...options, device: backend.device}) : createSceneRenderer(canvas, scene, options);
        r.resize();
        await nextFrame(); await nextFrame();
        const first = performance.now() - t0;
        const times = [], drawTimes = [];
        last = performance.now();
        for (let i = 0; i < FRAMES; i++) {
          const s = performance.now();
          r.setView(motion(b, fit, i)); r.draw();
          drawTimes.push(performance.now() - s);
          await nextFrame();
          const now = performance.now(); times.push(now - last); last = now;
        }
        r.destroy();
        far[kind] = {first: +first.toFixed(1), frames: stats(times.slice(1)), draw: stats(drawTimes)};
      }
      out.push({size, ...shape, svg: {first: +svgFirst.toFixed(1), frames: stats(svgTimes.slice(1))}, ...far});
    }
    host.remove();
    return {gpu, adapterInfo, out};
  })()`);
  const stamp = new Date().toISOString().replace(/[-:]/g, '').replace(/\..*/, 'Z');
  const revision = execSync('git rev-parse --short HEAD').toString().trim();
  const machine = {machine: `${os.cpus()[0].model}, ${os.totalmem() / 2 ** 30} GiB, ${os.platform()} ${os.release()}`, browser, gpu: result.gpu, webgpuAdapter: result.adapterInfo, revision, frames, viewport: '1400x900 at device pixel ratio 1', headless: true, loadAtStart};
  const lines = [JSON.stringify(machine), ...result.out.map(r => JSON.stringify(r))];
  const file = new URL(`../../../benchmarks/results/scene-frames-${stamp}.jsonl`, import.meta.url);
  writeFileSync(file, lines.join('\n') + '\n');
  for (const l of lines) console.log(l);
  console.log(`wrote ${file.pathname}`);
} finally {
  running.close();
}
