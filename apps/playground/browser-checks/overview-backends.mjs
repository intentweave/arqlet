// ADR 0025 O2: the cluster overview on its three backends (WebGPU, WebGL 2, WebGL 1) draws the same picture in every
// mode: plain, galaxy, orbit, lens, a path, a frame of breathing and a frame of a morph (the clock held still). Each is
// compared with WebGL 1 pixel by pixel (or, when `packages/viewer/overview-gl-reference.js` exists, with that earlier
// renderer kept beside it for the comparison: copy the previous `overview-gl.js` there, never commit it). Pictures go to
// OUT (default: a temporary folder). Starts its own server and Chrome. Not part of CI.
// usage: node overview-backends.mjs [graph=vehicle-network]
import {writeFileSync, mkdtempSync, existsSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {launch} from './launch.mjs';
import {connect} from './cdp.mjs';

const graphName = process.argv[2] ?? 'vehicle-network';
const out = process.env.OUT ?? mkdtempSync(join(tmpdir(), 'lw-overview-backends-'));
const reference = existsSync(new URL('../../../packages/viewer/overview-gl-reference.js', import.meta.url));
const running = await launch({serverPort: 8793, debugPort: 9373});
let failed = true;
try {
  const page = await connect(running.debugPort);
  const {evaluate, check} = page;
  await page.send('Runtime.enable');
  await page.send('Page.enable');
  await page.send('Emulation.setDeviceMetricsOverride', {width: 1400, height: 900, deviceScaleFactor: 1, mobile: false});
  await page.send('Page.navigate', {url: `${running.url}overview.html`});
  await page.waitFor(`document.readyState === 'complete'`, 'the page loads', 30000);
  const results = await evaluate(`(async () => {
    const {overviewModel, resolvedEdgeIds, pathOf, structuredExcerpt} = await import('/packages/viewer/overview.js');
    const {createOverviewRenderer} = await import('/packages/viewer/overview-gl.js');
    const {sceneBackend} = await import('/packages/viewer/scene-renderer.js');
    const ref = ${reference} ? await import('/packages/viewer/overview-gl-reference.js') : null;
    const {createEngine} = await import('/packages/engine-client/wasm-client.js');
    const run = await createEngine(await (await fetch('/generated/arqlet.wasm')).arrayBuffer());
    const graph = await (await fetch('/generated/overview/${graphName}.json')).json();
    const answer = run({op: 'overview', graph, threads: 1});
    if (!answer.ok) throw Error(answer.error);
    const edges = resolvedEdgeIds(graph);
    const model = overviewModel(answer.overview, edges);
    const moved = overviewModel({...answer.overview, x: answer.overview.x.map(v => v + answer.overview.rootRadius * 0.2), y: answer.overview.y.map(v => v * 0.9)}, edges);
    const gpu = await sceneBackend();
    // The clock, held still while a frame is drawn.
    const realNow = performance.now.bind(performance);
    let fixed = null;
    performance.now = () => fixed ?? realNow();
    const W = 900, H = 600, bg = [0.047, 0.063, 0.094, 1];
    const root = model.tops?.[0] ?? 0;
    const view = {centreX: 0, centreY: 0, scale: Math.min(W, H) / (2.1 * (answer.overview.rootRadius || 1))};
    const leafIds = answer.overview.ids.filter((_, i) => !model.children[i].length).slice(0, 6);
    // A structured cluster (O3): the largest container of at most 60 nodes below it, laid out by the engine as the page
    // does, seen close up; drawn in its colour and as the scene view draws it.
    const below = i => model.children[i].reduce((n, c) => n + 1 + below(c), 0);
    const cluster = [...model.children.keys()].filter(i => model.children[i].length && below(i) <= 60).sort((a, b) => below(b) - below(a))[0];
    const excerpt = structuredExcerpt(graph, model, answer.overview.ids[cluster], {});
    const imported = run({op: 'import', graph: excerpt.graph, limit: excerpt.graph.nodes.length, options: excerpt.options});
    const clusterScene = run({op: 'scene', snapshot: imported.snapshot}).scene;
    const close = {centreX: answer.overview.x[cluster], centreY: answer.overview.y[cluster], scale: Math.min(W, H) / (2.2 * answer.overview.r[cluster])};
    const structure = (r, look) => { r.setView(close); r.setState({expanded: new Map([[cluster, {scene: clusterScene}]])}); if (look) r.setStructured(look); };
    const modes = {
      plain: r => {},
      galaxy: r => r.setGalaxy(true),
      orbit: r => r.setOrbit({yaw: 0.5, pitch: 0.35}),
      lens: r => r.setLens({x: W * 0.55, y: H * 0.45, radius: 160, magnification: 3}),
      path: r => r.setState({trace: pathOf(model, {nodes: leafIds, edges: []}), selected: model.children.findIndex(c => c.length)}),
      breathing: r => { fixed = 1000; r.breathe({amplitude: 0.12, period: 4000, wavelength: 500, fadeIn: 1}); fixed = 2234; },
      morph: r => { fixed = 1000; r.morph(moved, {duration: 1000}); fixed = 1500; },
      'structured, cluster look': r => structure(r, null),
      'structured, scene look': r => structure(r, 'scene'),
    };
    // The earlier renderer has no scene look: that mode is compared with the WebGL 1 backend.
    const own = mode => mode === 'structured, scene look' || !ref;
    const make = (kind, mode) => {
      const canvas = document.createElement('canvas');
      canvas.style.cssText = 'position:fixed;left:0;top:0;width:' + W + 'px;height:' + H + 'px';
      document.body.append(canvas);
      const options = {view, background: bg};
      const r = kind === 'reference' ? ref.createOverviewRenderer(canvas, model, options)
        : createOverviewRenderer(canvas, model, {...options, ...(kind === 'webgpu' ? {device: gpu.device} : kind === 'webgl1' ? {backend: 'webgl1'} : {})});
      r.setView(view);
      r.resize?.();
      modes[mode](r);
      r.draw({labels: false});
      const copy = document.createElement('canvas'); copy.width = W; copy.height = H;
      const c = copy.getContext('2d', {willReadFrequently: true});
      c.fillStyle = 'rgb(12,16,24)'; c.fillRect(0, 0, W, H); c.drawImage(canvas, 0, 0);
      const backend = r.backend ?? 'webgl (reference)';
      r.destroy(); canvas.remove(); fixed = null;
      return {pixels: c.getImageData(0, 0, W, H).data, url: copy.toDataURL(), backend};
    };
    const kinds = [gpu.kind === 'webgpu' ? 'webgpu' : null, 'webgl2', ref ? 'webgl1' : null].filter(Boolean);
    const out = [];
    for (const mode of Object.keys(modes)) {
      const base = own(mode) ? make('webgl1', mode) : make('reference', mode);
      out.push({mode, kind: own(mode) ? 'webgl1' : 'reference', backend: base.backend, url: base.url, share: 0, base: true});
      for (const kind of kinds.filter(k => !(own(mode) && k === 'webgl1'))) {
        const r = make(kind, mode);
        let off = 0;
        for (let i = 0; i < r.pixels.length; i += 4) if (Math.max(Math.abs(r.pixels[i] - base.pixels[i]), Math.abs(r.pixels[i + 1] - base.pixels[i + 1]), Math.abs(r.pixels[i + 2] - base.pixels[i + 2])) > 48) off++;
        out.push({mode, kind, backend: r.backend, url: r.url, share: off / (W * H)});
      }
    }
    performance.now = realNow;
    return {nodes: answer.overview.ids.length, out};
  })()`);
  console.log(`${graphName}: ${results.nodes} nodes, compared with ${reference ? 'the earlier WebGL 1 renderer' : 'the WebGL 1 backend'}`);
  for (const r of results.out) {
    writeFileSync(join(out, `${r.mode.replace(/[^a-z]+/g, '-')}-${r.kind}.png`), Buffer.from(r.url.split(',')[1], 'base64'));
    if (r.base) continue;
    check(`O2 ${r.mode}: ${r.kind} (${r.backend}) draws as WebGL 1`, r.share < 0.01, `${(r.share * 100).toFixed(2)} % of pixels off`);
  }
  console.log(`pictures in ${out}`);
  failed = page.checks.some(c => !c.ok);
} finally {
  running.close();
}
process.exit(failed ? 1 : 0);
