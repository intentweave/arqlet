// The metro style's stations (style.js `glyph: 'tick'` and `'pill'`), in a real Chrome. Starts its own server and
// Chrome (`launch.mjs`). Not part of CI.
//
// Scenes: a small hand-made metro map (lines across, down and diagonal with stations running through, an interchange of
// three lines, a line that branches) and the playground's example vehicle-buses with its buses as shared lines, also
// routed octilinear (the scenes computed by the WASM engine). Each is drawn by `renderSvg` with the `metro` preset as
// it was before ticks and pills (every station a dot: BEFORE below) and as it is, both with their names, for a visual
// comparison; and the preset as it is by each GPU backend at the same transform, without text, against `renderSvg`
// without text: the share of pixels whose colour differs by more than 48 in a channel must stay below 1 % (as in
// scene-gl.mjs). Writes the pictures and an index.html that shows them side by side to OUT (default: a temporary folder).
// usage: node metro-glyphs.mjs
import {writeFileSync, mkdtempSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {launch} from './launch.mjs';
import {connect} from './cdp.mjs';

const out = process.env.OUT ?? mkdtempSync(join(tmpdir(), 'lw-metro-glyphs-'));
const running = await launch({serverPort: 8796, debugPort: 9376});
let failed = true;
try {
  const page = await connect(running.debugPort);
  const {evaluate, check} = page;
  await page.send('Runtime.enable');
  await page.send('Page.enable');
  await page.send('Emulation.setDeviceMetricsOverride', {width: 1400, height: 900, deviceScaleFactor: 1, mobile: false});
  await page.send('Page.navigate', {url: running.url});
  await page.waitFor(`document.readyState === 'complete'`, 'the page loads', 30000);
  // vehicle-buses as the playground lays it out (its download hands over the snapshot), its buses as shared lines, and
  // those routed octilinear.
  await page.waitForExample('vehicle-buses');
  await evaluate(`(() => { const s = document.getElementById('example'); s.value = 'vehicle-buses'; s.dispatchEvent(new Event('change')); })()`);
  await page.sleep(1500);
  await page.waitFor(`document.querySelectorAll('#graph g.node').length > 0 && !document.getElementById('save').disabled`, 'the example vehicle-buses is drawn');
  await page.sleep(1000);
  const taken = await evaluate(`(async () => {
    let blob = null;
    const create = URL.createObjectURL, click = HTMLAnchorElement.prototype.click;
    URL.createObjectURL = b => { blob = b; return 'blob:none'; };
    HTMLAnchorElement.prototype.click = function () {};
    try { document.getElementById('save').click(); } finally { URL.createObjectURL = create; HTMLAnchorElement.prototype.click = click; }
    const snapshot = JSON.parse(await blob.text());
    const engine = await (await import('/packages/engine-client/wasm-client.js')).createEngine(await (await fetch('/generated/arqlet.wasm')).arrayBuffer());
    const at = s => ({snapshot: s, expectedRevision: s.revision, expectedLayoutVersion: s.layoutVersion ?? 0});
    const lined = engine({op: 'configure', ...at(snapshot), options: {busStyle: 'lines'}});
    if (!lined.ok) throw Error('busStyle lines: ' + lined.error);
    const oct = engine({op: 'routing', ...at(lined.snapshot), nodeId: null, routing: 'octilinear'});
    if (!oct.ok) throw Error('octilinear: ' + oct.error);
    window.__scenes = {'vehicle-buses lines': engine({op: 'scene', snapshot: lined.snapshot}).scene, 'vehicle-buses lines octilinear': engine({op: 'scene', snapshot: oct.snapshot}).scene};
    return Object.keys(window.__scenes).join(', ');
  })()`);
  console.log(`scenes from the engine: ${taken}`);

  const results = await evaluate(`(async () => {
    const {renderSvg} = await import('/packages/viewer/svg.js');
    const {createSceneRenderer} = await import('/packages/viewer/scene-gl.js');
    const {createSceneGpuRenderer} = await import('/packages/viewer/scene-gpu.js');
    const {sceneBackend} = await import('/packages/viewer/scene-renderer.js');
    const {STYLE_PRESETS} = await import('/packages/viewer/style.js');
    const gpu = await sceneBackend();
    const W = 1200, H = 760, bg = '#fbfbf8';
    // The preset before ticks and pills: a station on one line a dot in its line's colour, an interchange a larger ring.
    const metro = STYLE_PRESETS.metro;
    const before = {...metro, rules: [...metro.rules.filter(r => r.selector.startsWith('edge')),
      {selector: 'leaf[bus]', style: {fill: '#ffffff', stroke: 'auto(bus)', strokeWidth: 3, text: '#1b1b1b', radius: 'pill', glyph: 'dot', glyphSize: 7}},
      {selector: 'leaf[interchange]', style: {fill: '#ffffff', stroke: '#1b1b1b', strokeWidth: 4, text: '#1b1b1b', radius: 'pill', glyph: 'dot', glyphSize: 10}}]};
    // A small metro map: red across (through the interchange X), blue down (through X, branching at B3), green from A3
    // down and then diagonal. Stations are 24-unit boxes; routes run between their middles.
    const stations = {A1: [100, 200], A2: [220, 200], X: [340, 200], A3: [460, 200], A4: [580, 200], A5: [700, 200],
      B1: [340, 40], B2: [340, 120], B3: [340, 320], B4: [340, 440], B5: [440, 420], B6: [520, 500],
      G1: [460, 300], G2: [540, 380], G3: [620, 460], G4: [700, 540]};
    const names = {A1: 'Westkreuz', A2: 'Messe', X: 'Zentrum', A3: 'Hafen', A4: 'Ostpark', A5: 'Endstation Ost', B1: 'Nordring', B2: 'Uni',
      B3: 'Gabelung', B4: 'Südfriedhof', B5: 'Klinik', B6: 'Gewerbehof', G1: 'Kanal', G2: 'Werft', G3: 'Insel', G4: 'Leuchtturm'};
    const lines = {red: ['A1', 'A2', 'X', 'A3', 'A4', 'A5'], blue: ['B1', 'B2', 'X', 'B3', 'B4'], blue2: ['B3', 'B5', 'B6'], green: ['A3', 'G1', 'G2', 'G3', 'G4']};
    const routes = [], buses = [];
    for (const [line, ids] of Object.entries(lines)) {
      const bus = [];
      for (let k = 0; k + 1 < ids.length; k++) {
        const id = line + ':' + ids[k] + '-' + ids[k + 1], [ax, ay] = stations[ids[k]], [bx, by] = stations[ids[k + 1]];
        routes.push({id, source: ids[k], target: ids[k + 1], points: [{x: ax, y: ay}, {x: bx, y: by}], weight: 1, invalid: false});
        bus.push(id);
      }
      buses.push(bus);
    }
    // The branch belongs to the blue line: one bus.
    buses[1].push(...buses.splice(2, 1)[0]);
    const sample = {schemaVersion: 1, revision: 0, layoutVersion: 1, bounds: {x: 0, y: 0, width: 860, height: 600}, showPorts: false, ports: [], labels: [], crossings: [],
      nodes: Object.entries(stations).map(([id, [x, y]]) => ({id, label: names[id], kind: 'station', container: false, collapsed: false, aggregate: false, invalid: false, bounds: {x: x - 12, y: y - 12, width: 24, height: 24}})),
      routes, buses};
    const scenes = [['metro sample', sample], ...Object.entries(window.__scenes)];
    // A scene drawn by renderSvg into W×H (with or without its texts), as pixels and a picture.
    const svgPicture = async (scene, style, texts) => {
      const b = scene.bounds, scale = Math.min(W / b.width, H / b.height) * 0.92;
      const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
      svg.setAttribute('xmlns', 'http://www.w3.org/2000/svg');
      renderSvg(svg, scene, {scale, document, style});
      const css = document.createElementNS('http://www.w3.org/2000/svg', 'style');
      css.textContent = texts ? 'text{font-family:system-ui,sans-serif}' : 'text{display:none}';
      svg.prepend(css);
      const img = new Image();
      img.src = 'data:image/svg+xml;charset=utf-8,' + encodeURIComponent(new XMLSerializer().serializeToString(svg));
      await img.decode();
      const c = document.createElement('canvas'); c.width = W; c.height = H;
      const cx = c.getContext('2d', {willReadFrequently: true});
      cx.fillStyle = bg; cx.fillRect(0, 0, W, H);
      cx.drawImage(img, W / 2 - b.width * scale / 2, H / 2 - b.height * scale / 2, b.width * scale, b.height * scale);
      return {canvas: c, scale, view: {centreX: b.x + b.width / 2, centreY: b.y + b.height / 2, scale}};
    };
    const counts = scene => {
      const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
      renderSvg(svg, scene, {scale: 1, document, style: metro});
      return {ticks: svg.querySelectorAll('g.node > line').length, pills: svg.querySelectorAll('g.node > rect[transform]').length, dots: svg.querySelectorAll('g.node > circle').length};
    };
    const out = [];
    for (const [name, scene] of scenes) {
      const was = await svgPicture(scene, before, true), now = await svgPicture(scene, metro, true), plain = await svgPicture(scene, metro, false);
      const row = {name, counts: counts(scene), before: was.canvas.toDataURL(), after: now.canvas.toDataURL(), gpu: []};
      for (const backend of [gpu.kind === 'webgpu' ? 'webgpu' : null, 'webgl2'].filter(Boolean)) {
        const gl = document.createElement('canvas');
        gl.style.cssText = 'position:fixed;left:0;top:0;width:' + W + 'px;height:' + H + 'px';
        document.body.append(gl);
        const r = backend === 'webgpu' ? createSceneGpuRenderer(gl, scene, {view: plain.view, device: gpu.device, style: metro, background: [0.984, 0.984, 0.973, 1]})
          : createSceneRenderer(gl, scene, {view: plain.view, preserveDrawingBuffer: true, style: metro, background: [0.984, 0.984, 0.973, 1]});
        r.resize();
        const c = document.createElement('canvas'); c.width = W; c.height = H;
        const cc = c.getContext('2d', {willReadFrequently: true});
        cc.fillStyle = bg; cc.fillRect(0, 0, W, H); cc.drawImage(gl, 0, 0);
        const pa = plain.canvas.getContext('2d').getImageData(0, 0, W, H).data, pc = cc.getImageData(0, 0, W, H).data;
        const d = document.createElement('canvas'); d.width = W; d.height = H;
        const cd = d.getContext('2d'), diff = cd.createImageData(W, H);
        let off = 0;
        for (let i = 0; i < pa.length; i += 4) {
          const m = Math.max(Math.abs(pa[i] - pc[i]), Math.abs(pa[i + 1] - pc[i + 1]), Math.abs(pa[i + 2] - pc[i + 2]));
          if (m > 48) { off++; diff.data[i] = 255; diff.data[i + 1] = 60; diff.data[i + 2] = 60; }
          else { diff.data[i] = diff.data[i + 1] = diff.data[i + 2] = 255 - (255 - pa[i]) / 3; }
          diff.data[i + 3] = 255;
        }
        cd.putImageData(diff, 0, 0);
        row.gpu.push({backend, share: off / (W * H), picture: c.toDataURL(), diff: d.toDataURL()});
        r.destroy(); gl.remove();
      }
      out.push(row);
    }
    return out;
  })()`);
  const slug = name => name.replace(/[^a-z0-9]+/gi, '-');
  const cells = [];
  for (const r of results) {
    writeFileSync(join(out, `${slug(r.name)}-before.png`), Buffer.from(r.before.split(',')[1], 'base64'));
    writeFileSync(join(out, `${slug(r.name)}-after.png`), Buffer.from(r.after.split(',')[1], 'base64'));
    console.log(`${r.name}: ${r.counts.ticks} ticks, ${r.counts.pills} pills, ${r.counts.dots} dots`);
    check(`${r.name}: stations are drawn as ticks and pills, none as a dot`, r.counts.ticks + r.counts.pills > 0 && r.counts.dots === 0, JSON.stringify(r.counts));
    for (const g of r.gpu) {
      writeFileSync(join(out, `${slug(r.name)}-${g.backend}.png`), Buffer.from(g.picture.split(',')[1], 'base64'));
      writeFileSync(join(out, `${slug(r.name)}-${g.backend}-diff.png`), Buffer.from(g.diff.split(',')[1], 'base64'));
      check(`${r.name}: ${g.backend} draws the ticks and pills as renderSvg (without text)`, g.share < 0.01, `${(g.share * 100).toFixed(2)} % of pixels off`);
    }
    cells.push(`<h2>${r.name}</h2><p>${r.counts.ticks} ticks, ${r.counts.pills} pills; ${r.gpu.map(g => `${g.backend} ${(g.share * 100).toFixed(2)} % off`).join(', ')}</p>
<div class="row"><figure><img src="${slug(r.name)}-before.png"><figcaption>before (dots)</figcaption></figure><figure><img src="${slug(r.name)}-after.png"><figcaption>after (ticks and pills)</figcaption></figure></div>
<div class="row">${r.gpu.map(g => `<figure><img src="${slug(r.name)}-${g.backend}.png"><figcaption>${g.backend}</figcaption></figure><figure><img src="${slug(r.name)}-${g.backend}-diff.png"><figcaption>${g.backend} against renderSvg (red: off)</figcaption></figure>`).join('')}</div>`);
  }
  writeFileSync(join(out, 'index.html'), `<!doctype html><meta charset="utf-8"><title>Metro glyphs</title><style>body{font:14px system-ui;margin:16px;background:#fff;color:#1b1b1b}.row{display:flex;gap:12px;flex-wrap:wrap}figure{margin:0;flex:1 1 480px}img{width:100%;border:1px solid #ddd}figcaption{color:#666;font-size:12px}</style>
<h1>Metro glyphs: before and after</h1>${cells.join('\n')}`);
  console.log(`pictures in ${out} (index.html)`);
  failed = page.checks.some(c => !c.ok);
} finally {
  running.close();
}
process.exit(failed ? 1 : 0);
