// ADR 0025, exits of V1 and V2, in a real Chrome. Starts its own server and Chrome (`launch.mjs`). Not part of CI.
//
// V1: the golden scenes of `packages/viewer/fixtures` and the scenes of the playground's examples, one or more per kind
// of drawing (EXAMPLES: tree and radial, organic and mixed strategies, ER rows, circles, bus lines, hulls, rounded routes,
// self-loops; taken from the playground's download after it laid them out, the scene computed by the WASM engine),
// drawn by `renderSvg` and by each GPU backend at the same transform, without text (the far layer's text is a 2D canvas and is not compared): the share of pixels whose colour
// differs by more than 48 in a channel must stay below 1 %. Writes both pictures and the difference to OUT
// (default: the system's temporary folder) for inspection.
// V2: the scene view zoomed in and out across the threshold in small steps: at every step the near layer's boxes must
// stand where the view puts them (within half a pixel), the far layer's box borders in its pixels too, and while near,
// the elements in view stay within the budget times the hysteresis.
// usage: node scene-gl.mjs
import {writeFileSync, mkdtempSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {launch} from './launch.mjs';
import {connect} from './cdp.mjs';

const out = process.env.OUT ?? mkdtempSync(join(tmpdir(), 'lw-scene-gl-'));
const running = await launch({serverPort: 8784, debugPort: 9364});
let failed = false;
try {
  const page = await connect(running.debugPort);
  const {evaluate, check} = page;
  await page.send('Runtime.enable');
  await page.send('Page.enable');
  await page.send('Emulation.setDeviceMetricsOverride', {width: 1400, height: 900, deviceScaleFactor: 1, mobile: false});
  await page.send('Page.navigate', {url: running.url});
  await page.waitFor(`document.readyState === 'complete'`, 'the page loads', 30000);
  await page.sleep(2000);
  // The examples' scenes as the playground lays them out: its download hands over the snapshot, the engine draws the scene.
  const EXAMPLES = (process.env.EXAMPLES ?? 'workflow-dag,nested-tree-radial,mixed-layouts,mixed-large,er-diagram,uml-class,petri-net-shapes,vehicle-buses,vehicle-buses-organic,vehicle-zonal,memberships,self-loops,cloud-architecture,network-vlan,org-chart').split(',');
  const SETUP = {
    // One edit at a time: the hulls first, then their shape.
    memberships: `(async () => { const c = document.getElementById('membership-hulls'); if (!c.checked) { c.checked = true; c.dispatchEvent(new Event('change')); }
      for (let i = 0; i < 100 && !document.querySelector('#graph .lw-hull'); i++) await new Promise(r => setTimeout(r, 100));
      const h = document.getElementById('membership-hull'); h.value = 'polygon'; h.dispatchEvent(new Event('change')); })()`,
    'network-vlan': `(() => { document.getElementById('options-json').value = JSON.stringify({hubRefs: 4}); document.getElementById('apply-options').click(); })()`,
  };
  for (const name of EXAMPLES) {
    await page.waitForExample(name);
    await evaluate(`(() => { const s = document.getElementById('example'); s.value = ${JSON.stringify(name)}; s.dispatchEvent(new Event('change')); })()`);
    await page.sleep(1500);
    await page.waitFor(`document.querySelectorAll('#graph g.node').length > 0 && !document.getElementById('save').disabled`, `the example ${name} is drawn`);
    await page.sleep(1000);
    // Two kinds need an option: membership hulls (as polygons) and hub references.
    if (SETUP[name]) { await evaluate(SETUP[name]); await page.sleep(2500); }
    const features = await evaluate(`(async () => {
      let blob = null;
      const create = URL.createObjectURL, click = HTMLAnchorElement.prototype.click;
      URL.createObjectURL = b => { blob = b; return 'blob:none'; };
      HTMLAnchorElement.prototype.click = function () {};
      try { document.getElementById('save').click(); } finally { URL.createObjectURL = create; HTMLAnchorElement.prototype.click = click; }
      const snapshot = JSON.parse(await blob.text());
      window.__engine ??= await (await import('/packages/engine-client/wasm-client.js')).createEngine(await (await fetch('/generated/arqlet.wasm')).arrayBuffer());
      const answer = window.__engine({op: 'scene', snapshot});
      if (!answer.ok) throw Error(answer.error);
      const scene = answer.scene;
      (window.__scenes ??= {})[${JSON.stringify(name)}] = scene;
      // Every strategy and routing style of the whole view, on one example, through the engine's operations.
      if (${JSON.stringify(name)} === 'workflow-dag') {
        for (const [op, value] of [['strategy', 'tree'], ['strategy', 'radial'], ['strategy', 'organic'], ['strategy', 'none'],
          ['routing', 'straight'], ['routing', 'roundedOrthogonal'], ['routing', 'polyline'], ['routing', 'roundedPolyline'], ['routing', 'octilinear'], ['routing', 'curve'], ['routing', 'bus'], ['bundling', 'complete']]) {
          const base = {snapshot, expectedRevision: snapshot.revision, expectedLayoutVersion: snapshot.layoutVersion ?? 0};
          const changed = op === 'bundling' ? window.__engine({op: 'configure', ...base, options: {bundling: value}}) : window.__engine({op, ...base, nodeId: null, [op]: value});
          if (!changed.ok) { window.__variantErrors = [...(window.__variantErrors ?? []), op + '=' + value + ': ' + changed.error]; continue; }
          const drawn = window.__engine({op: 'scene', snapshot: changed.snapshot});
          if (drawn.ok) window.__scenes['workflow-dag ' + op + '=' + value] = drawn.scene;
        }
      }
      const diagonal = scene.routes.some(r => r.points.some((p, i) => i && p.x !== r.points[i - 1].x && p.y !== r.points[i - 1].y));
      const f = {rows: scene.nodes.some(n => n.rows?.length), circles: scene.nodes.some(n => n.shape === 'circle'), hulls: !!scene.membershipHulls?.length,
        busLines: !!scene.busLines?.length, hubRefs: !!scene.hubRefs?.length, junctions: !!scene.junctions?.length, rounded: scene.routes.some(r => r.rounded),
        diagonal, labels: scene.labels.length > 0, ports: scene.showPorts, selfLoops: scene.routes.some(r => r.source === r.target), aggregated: scene.routes.some(r => r.backWeight != null)};
      return Object.keys(f).filter(k => f[k]).join(' ');
    })()`);
    console.log(`scene ${name}: ${features || 'boxes and routes'}`);
  }
  const variants = await evaluate(`JSON.stringify({names: Object.keys(window.__scenes).filter(k => k.includes(' ')), errors: window.__variantErrors ?? [],
    rounded: Object.entries(window.__scenes).filter(([, s]) => s.routes.some(r => r.rounded)).map(([k]) => k), junctions: Object.entries(window.__scenes).filter(([, s]) => s.junctions?.length).map(([k]) => k)})`);
  console.log(`variants ${variants}`);

  const golden = await evaluate(`(async () => {
    const {renderSvg} = await import('/packages/viewer/svg.js');
    const {createSceneRenderer} = await import('/packages/viewer/scene-gl.js');
    const {createSceneGpuRenderer} = await import('/packages/viewer/scene-gpu.js');
    const {sceneBackend} = await import('/packages/viewer/scene-renderer.js');
    const gpu = await sceneBackend();
    const W = 1200, H = 700, bg = '#0e1520';
    const results = [];
    for (const backend of [gpu.kind === 'webgpu' ? 'webgpu' : null, 'webgl2'].filter(Boolean)) {
    // The states both layers draw (renderSvg's options): a trace with fading, pins, a selected node, a selected route.
    const stated = [];
    for (const base of ['workflow-dag', 'bpmn-pools']) {
      const scene = window.__scenes?.[base] ?? await (await fetch('/packages/viewer/fixtures/' + base + '.scene.json')).json();
      const leaves = scene.nodes.filter(n => !n.container), route = scene.routes.find(r => r.points.length > 2) ?? scene.routes[0];
      const distance = Object.fromEntries(scene.nodes.map((n, i) => [n.id, i % 4]));
      stated.push([base + ' trace', scene, {trace: {nodes: [route.source, route.target], routes: [route.id], distance, fade: true}}]);
      stated.push([base + ' pins and selection', scene, {pinned: new Set(leaves.slice(0, 3).map(n => n.id)), selected: leaves[1].id}]);
      stated.push([base + ' selected route', scene, {selectedRoute: route.id}]);
    }
    const all = [];
    for (const name of ['state-tcp', 'bpmn-pools', ...Object.keys(window.__scenes ?? {})]) all.push([name, window.__scenes?.[name] ?? await (await fetch('/packages/viewer/fixtures/' + name + '.scene.json')).json(), {}]);
    all.push(...stated);
    for (const [name, scene, state] of all) {
      const b = scene.bounds, scale = Math.min(W / b.width, H / b.height) * 0.95;
      const view = {centreX: b.x + b.width / 2, centreY: b.y + b.height / 2, scale};
      const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
      svg.setAttribute('xmlns', 'http://www.w3.org/2000/svg');
      renderSvg(svg, scene, {scale, document, ...state});
      const style = document.createElementNS('http://www.w3.org/2000/svg', 'style');
      style.textContent = 'text{display:none}';
      svg.prepend(style);
      const img = new Image();
      img.src = 'data:image/svg+xml;charset=utf-8,' + encodeURIComponent(new XMLSerializer().serializeToString(svg));
      await img.decode();
      const a = document.createElement('canvas'); a.width = W; a.height = H;
      const ca = a.getContext('2d', {willReadFrequently: true});
      ca.fillStyle = bg; ca.fillRect(0, 0, W, H);
      ca.drawImage(img, W / 2 - b.width * scale / 2, H / 2 - b.height * scale / 2, b.width * scale, b.height * scale);
      const gl = document.createElement('canvas');
      gl.style.cssText = 'position:fixed;left:0;top:0;width:' + W + 'px;height:' + H + 'px';
      document.body.append(gl);
      const r = backend === 'webgpu' ? createSceneGpuRenderer(gl, scene, {view, device: gpu.device, ...state}) : createSceneRenderer(gl, scene, {view, preserveDrawingBuffer: true, ...state});
      r.resize();
      const c = document.createElement('canvas'); c.width = W; c.height = H;
      const cc = c.getContext('2d', {willReadFrequently: true});
      cc.fillStyle = bg; cc.fillRect(0, 0, W, H); cc.drawImage(gl, 0, 0);
      const pa = ca.getImageData(0, 0, W, H).data, pc = cc.getImageData(0, 0, W, H).data;
      const d = document.createElement('canvas'); d.width = W; d.height = H;
      const cd = d.getContext('2d'), diff = cd.createImageData(W, H);
      let off = 0, sum = 0;
      for (let i = 0; i < pa.length; i += 4) {
        const m = Math.max(Math.abs(pa[i] - pc[i]), Math.abs(pa[i + 1] - pc[i + 1]), Math.abs(pa[i + 2] - pc[i + 2]));
        sum += m;
        if (m > 48) { off++; diff.data[i] = 255; diff.data[i + 1] = 80; diff.data[i + 2] = 80; diff.data[i + 3] = 255; }
        else { diff.data[i] = diff.data[i + 1] = diff.data[i + 2] = pa[i] / 3; diff.data[i + 3] = 255; }
      }
      cd.putImageData(diff, 0, 0);
      // Hover: the outline of a leaf turns renderSvg's hover blue on its left border.
      let hover = null;
      if (name === 'bpmn-pools') {
        const leaf = scene.nodes.find(n => !n.container && n.shape !== 'circle');
        r.setHover(leaf.id); r.draw();
        const h = document.createElement('canvas'); h.width = W; h.height = H;
        const hc = h.getContext('2d', {willReadFrequently: true}); hc.drawImage(gl, 0, 0);
        const x = Math.round(W / 2 + (leaf.bounds.x - view.centreX) * scale), y = Math.round(H / 2 + (leaf.bounds.y + leaf.bounds.height / 2 - view.centreY) * scale);
        let best = 0;
        for (let i = x - 2; i <= x + 2; i++) { const p = hc.getImageData(i, y, 1, 1).data; best = Math.max(best, p[2] - p[0]); }
        hover = best;
      }
      results.push({backend, name, hover, share: off / (W * H), mean: sum / (W * H), svg: a.toDataURL(), gl: c.toDataURL(), diff: d.toDataURL()});
      r.destroy(); gl.remove();
    }
    }
    return results;
  })()`);
  for (const g of golden) {
    for (const k of ['svg', 'gl', 'diff']) writeFileSync(join(out, `${g.name}-${g.backend}-${k}.png`), Buffer.from(g[k].split(',')[1], 'base64'));
    check(`V1 ${g.backend} ${g.name}: the far layer matches renderSvg without text`, g.share < 0.01, `${(g.share * 100).toFixed(2)} % of pixels off, mean channel difference ${g.mean.toFixed(2)}`);
    if (g.hover !== null) check(`V1 ${g.backend}: a hovered leaf is outlined in the hover blue`, g.hover > 60, `blue minus red on the border: ${g.hover}`);
  }

  const switchings = await evaluate(`(async () => {
    const {createSceneView} = await import('/packages/viewer/scene-view.js');
    const {sceneBackend} = await import('/packages/viewer/scene-renderer.js');
    const gpu = await sceneBackend();
    const all = [];
    for (const backend of [gpu.kind === 'webgpu' ? gpu : null, {kind: 'webgl2'}].filter(Boolean)) {
    const scene = await (await fetch('/packages/viewer/fixtures/bpmn-pools.scene.json')).json();
    const host = document.createElement('div');
    host.style.cssText = 'position:fixed;left:50px;top:40px;width:900px;height:600px;background:#0e1520;z-index:99';
    document.body.append(host);
    const budget = 30;
    const v = createSceneView(host, scene, {budget, minPixels: 20, fade: 0, preserveDrawingBuffer: true, backend});
    const start = v.view, hostRect = host.getBoundingClientRect();
    const steps = [];
    let worst = 0, worstFar = 0, overBudget = 0, nearSteps = 0, farSteps = 0;
    const leaves = scene.nodes.filter(n => !n.container && n.kind !== 'attribute' && n.shape !== 'circle');
    const target = leaves[Math.floor(leaves.length / 2)];
    const cx = target.bounds.x + target.bounds.width / 2, cy = target.bounds.y + target.bounds.height / 2;
    const scales = [];
    for (let s = start.scale / 6; s < start.scale * 6; s *= 1.08) scales.push(s);
    for (const s of [...scales, ...scales.slice().reverse()]) {
      v.setView({centreX: cx, centreY: cy, scale: s});
      v.far.draw();
      const view = v.view, layer = v.layer;
      const screen = (x, y) => [hostRect.left + 450 + (x - view.centreX) * view.scale, hostRect.top + 300 + (y - view.centreY) * view.scale];
      if (layer === 'near') {
        nearSteps++;
        if (v.counts.elements > budget * 1.25) overBudget++;
        for (const g of v.near.querySelectorAll('g[data-node-id]')) {
          const n = scene.nodes.find(m => m.id === g.getAttribute('data-node-id'));
          if (!n || n.shape === 'circle' || n.kind === 'attribute') continue;
          const r = g.querySelector('rect:not(.lw-hit)').getBoundingClientRect();
          const [x, y] = screen(n.bounds.x, n.bounds.y);
          worst = Math.max(worst, Math.abs(r.left - x), Math.abs(r.top - y), Math.abs(r.width - n.bounds.width * view.scale));
        }
      } else {
        farSteps++;
        // The left border of the target in the far layer's pixels: the stroke's column on the row through its middle.
        const gl = v.far && host.querySelector('canvas.lw-far');
        const c = document.createElement('canvas'); c.width = 900; c.height = 600;
        const cc = c.getContext('2d', {willReadFrequently: true}); cc.drawImage(gl, 0, 0);
        const [x, y] = screen(target.bounds.x, cy);
        const row = cc.getImageData(0, Math.round(y - hostRect.top), 900, 1).data;
        const px = x - hostRect.left;
        // The first pixel from the left that differs from the background: the box's outer edge (its stroke is centred on
        // the border, half a pixel or less outside).
        const bgAt = Math.max(0, Math.floor(px) - 8) * 4, bg = row[bgAt] + row[bgAt + 1] + row[bgAt + 2];
        let found = -1;
        for (let i = Math.max(0, Math.floor(px) - 6); i <= Math.min(899, Math.ceil(px) + 6); i++) {
          if (Math.abs(row[i * 4] + row[i * 4 + 1] + row[i * 4 + 2] - bg) > 12) { found = i; break; }
        }
        if (view.scale * target.bounds.width > 8 && found >= 0) worstFar = Math.max(worstFar, Math.abs(found + 0.5 - px));
      }
      steps.push(layer + ':' + v.counts.elements + '/' + v.counts.boxPixels.toFixed(1));
    }
    const layers = steps.map(s => s.split(':')[0]); const switches = layers.filter((l, i) => i && l !== layers[i - 1]).length;
    // The sweep goes in and out over the same scales: a scale drawn far on the way in and near on the way out is the hysteresis.
    const held = layers.slice(0, scales.length).filter((l, i) => l === 'far' && layers[layers.length - 1 - i] === 'near').length;
    all.push({backend: v.backend, worst, worstFar, overBudget, nearSteps, farSteps, switches, held, steps: steps.join(' ')});
    v.destroy(); host.remove();
    }
    return all;
  })()`);
  check('V1 WebGPU is offered and checked', golden.some(g => g.backend === 'webgpu'), 'without it only the fallback was compared');
  for (const switching of switchings) {
  const b = switching.backend;
  check(`V2 ${b}: the view switches to near when zoomed in and back to far when zoomed out`, switching.nearSteps > 0 && switching.farSteps > 0 && switching.switches === 2, `${switching.switches} switches, ${switching.nearSteps} near and ${switching.farSteps} far steps`);
  check(`V2 ${b} hysteresis: the way out hands back later than the way in took over`, switching.held > 0, `${switching.held} scales far on the way in and near on the way out`);
  check(`V2 ${b} near layer: every box stands where the view puts it`, switching.worst <= 0.5, `worst ${switching.worst.toFixed(3)} px`);
  check(`V2 ${b} far layer: the box border stands where the view puts it`, switching.worstFar <= 1, `worst ${switching.worstFar.toFixed(2)} px`);
  check(`V2 ${b} near layer: elements in view within budget × hysteresis`, switching.overBudget === 0, `${switching.overBudget} steps over`);
  if (process.env.DEBUG) console.log(switching.steps);
  }
  console.log(`pictures in ${out}`);
  failed = page.checks?.some?.(c => !c.ok) ?? false;
} finally {
  running.close();
}
process.exit(failed ? 1 : 0);
