// ADR 0028, step B0: what hierarchical edge bundling would draw on the engine's organic views, without changing the engine.
// Every gate view is imported with its root and containers organic and routing straight; the curves of sections 4 and 5
// are computed from the engine's scene (`bundling.mjs`) with centre, free-place and gate control points, measured, and
// drawn. Needs target/release/layout-command; Google Chrome for the PNG pictures (`CHROME` names another binary).
//
//   node benchmarks/eval/bundling-probe.mjs --out benchmarks/results/NAME.jsonl --drawings DIR [--cache DIR] [VIEW ...]
//
// Writes NAME.jsonl and NAME.md (the summary), and the drawings into DIR (keep it outside the repository). `--cache`
// keeps the engine's scenes, so that a second run can skip the layouts. Definitions (fixed before the first run):
//   - gate view passes a variant when it has at least 50 eligible edges and, at strength 0.85: at least 60 % of them drawn
//     bundled (full or weakened), no new node entry, ink ratio at most 0.85 and mean distortion at most 1.2;
//   - ink ratio: cells of a 2-unit raster that the eligible edges pass through, as drawn (bundled or kept) over unbundled;
//   - distortion: drawn length over unbundled length, the mean over all eligible edges (kept ones count 1);
//   - go for a variant: it passes on at least three gate views.
// The other readings (ink over all routes, other raster cells, distortion over the bundled edges only) are reported
// beside these and decide nothing; so are the structure of each hierarchy and a synthetic control (a deep hierarchy with
// its leaves on a circle, the picture of Holten's paper; no engine), which shows what the ink measure can reach at all.
import {spawn, spawnSync, execFileSync} from 'node:child_process';
import {readFileSync, writeFileSync, mkdirSync, existsSync} from 'node:fs';
import {createHash} from 'node:crypto';
import {cpus, loadavg, totalmem, release, platform, arch} from 'node:os';
import {resolve} from 'node:path';
import {pathToFileURL, fileURLToPath} from 'node:url';
import {viewOf, structureOf, eligibility, parallelOffsets, bundleEdge, controlPolygon, straighten, segmentCount, bspline, cut, refusal, inkCells, lengthOf, enters, VARIANTS, RULES} from './bundling.mjs';

const ROOT = fileURLToPath(new URL('../../', import.meta.url));
const COMMAND = process.env.LAYOUT_COMMAND ?? `${ROOT}target/release/layout-command`;
const CHROME = process.env.CHROME ?? '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const VIEWS = {
  'org-chart': 'benchmarks/inputs/org-chart.json',
  'layerweave-code': 'benchmarks/inputs/layerweave-code.json',
  'mixed-large': 'examples/fixtures/phase0/mixed-large.json',
  'workflow-dag': 'benchmarks/inputs/workflow-dag.json',
  'phase-width-feedback': 'examples/fixtures/phase-width-feedback.json',
  'synthetic-2k': 'benchmarks/inputs/synthetic-2k.json',
};
const GATE = {eligible: 50, bundled: 0.60, ink: 0.85, distortion: 1.2, views: 3, strength: 0.85};
const STRENGTHS = [0.5, 0.85, 1];

const args = process.argv.slice(2);
const option = (flag, fallback) => { const i = args.indexOf(flag); return i < 0 ? fallback : args.splice(i, 2)[1]; };
const out = resolve(option('--out', 'bundling-probe.jsonl'));
const drawings = resolve(option('--drawings', 'bundling-drawings'));
const cache = option('--cache', null);
const names = args.length ? args : Object.keys(VIEWS);

// ---------- The engine: one process per request, restarted when it hangs before running ----------
const retries = [];
function once(input, limitMs, stallMs) {
  return new Promise((done, fail) => {
    const child = spawn(COMMAND, [], {stdio: ['pipe', 'pipe', 'pipe']});
    const chunks = [], errors = [];
    let reason = null;
    const kill = why => { reason = why; child.kill('SIGKILL'); };
    const limit = setTimeout(() => kill('limit'), limitMs);
    // A process that has taken no processor time after `stallMs` has not started to run.
    const stall = setTimeout(() => {
      const ps = spawnSync('ps', ['-o', 'time=', '-p', String(child.pid)], {encoding: 'utf8'});
      if (/^\s*0:00\.0[01]\s*$/.test(ps.stdout ?? '')) kill('stall');
    }, stallMs);
    child.stdout.on('data', d => chunks.push(d));
    child.stderr.on('data', d => errors.push(d));
    child.stdin.on('error', () => {});
    child.on('close', code => {
      clearTimeout(limit); clearTimeout(stall);
      if (reason) fail(Object.assign(new Error(`layout-command: ${reason}`), {reason}));
      else if (code !== 0) fail(new Error(Buffer.concat(errors).toString()));
      else done(JSON.parse(Buffer.concat(chunks).toString()));
    });
    child.stdin.end(input);
  });
}
async function engine(request, {limitMs = 60_000, stallMs = 15_000} = {}) {
  const input = JSON.stringify(request);
  for (let attempt = 1; ; attempt++) {
    try {
      const response = await once(input, limitMs, stallMs);
      if (!response.ok) throw new Error(response.error);
      return response;
    } catch (e) {
      if (!e.reason || attempt >= 4) throw e;
      retries.push({op: request.op, reason: e.reason, attempt});
      console.error(`retry ${request.op} after ${e.reason}`);
    }
  }
}

/** The view with its root and containers organic and routing straight: the authored directions, strategies and routing
 * are removed, nothing else. */
function organicStraight(graph) {
  return {...graph, defaultDirection: 'auto', containerDirections: {}, defaultStrategy: 'organic', containerStrategies: {}, defaultRouting: 'straight', containerRouting: {}};
}

async function load(name) {
  const file = `${ROOT}${VIEWS[name]}`;
  const text = readFileSync(file);
  const inputSha256 = createHash('sha256').update(text).digest('hex');
  const stored = cache && `${cache}/${name}.json`;
  if (stored && existsSync(stored)) return {...JSON.parse(readFileSync(stored, 'utf8')), inputSha256, cached: true};
  const raw = JSON.parse(text);
  const graph = organicStraight(raw.graph ?? raw);
  const {snapshot} = await engine({op: 'import', graph, limit: graph.nodes.length}, {limitMs: 3_600_000});
  const {scene} = await engine({op: 'scene', snapshot}, {limitMs: 600_000});
  if (stored) { mkdirSync(cache, {recursive: true}); writeFileSync(stored, JSON.stringify({snapshot, scene, nodes: graph.nodes.length, edges: graph.edges.length})); }
  return {snapshot, scene, nodes: graph.nodes.length, edges: graph.edges.length, inputSha256, cached: false};
}

// ---------- Measures ----------
const mean = list => list.length ? list.reduce((a, b) => a + b, 0) / list.length : null;
const median = list => { if (!list.length) return null; const s = [...list].sort((a, b) => a - b), m = s.length >> 1; return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2; };
const stats = list => ({mean: mean(list), median: median(list), max: list.length ? list.reduce((a, b) => Math.max(a, b)) : null});

const borderDistance = (p, node) => {
  const r = node.bounds;
  if (node.shape === 'circle') return Math.abs(Math.hypot(p.x - r.x - r.width / 2, p.y - r.y - r.height / 2) - Math.min(r.width, r.height) / 2);
  const inside = p.x >= r.x && p.x <= r.x + r.width && p.y >= r.y && p.y <= r.y + r.height;
  if (inside) return Math.min(p.x - r.x, r.x + r.width - p.x, p.y - r.y, r.y + r.height - p.y);
  return Math.hypot(Math.max(r.x - p.x, 0, p.x - r.x - r.width), Math.max(r.y - p.y, 0, p.y - r.y - r.height));
};

/** The two hard measures, without the guard's grid: every node is tested; every bundled curve is complete. */
function verify(view, eligible, results) {
  let guardViolations = 0, incomplete = 0;
  const all = [...view.nodes.values()];
  eligible.forEach((edge, i) => {
    const {level, points} = results[i];
    if (level === 'kept') { if (points !== edge.base) incomplete++; return; }
    const finite = points.every(p => Number.isFinite(p.x) && Number.isFinite(p.y));
    const ends = borderDistance(points[0], view.nodes.get(edge.source)) < 1e-6 && borderDistance(points.at(-1), view.nodes.get(edge.target)) < 1e-6;
    if (!finite || !ends || points.length < 2) incomplete++;
    if (refusal(view, edge, points, all)) guardViolations++;
  });
  return {guardViolations, incomplete: incomplete + Math.abs(eligible.length - results.length)};
}

/** The engine's own count (`routeConflicts`): nodes a bundled curve enters that its unbundled route does not. */
async function engineEntries(view, eligible, results) {
  const nodes = [...view.nodes.values()];
  const containers = new Set(nodes.map(n => n.parentId).filter(Boolean));
  const drawNode = n => ({id: n.id, parentId: n.parentId ?? null, container: containers.has(n.id), ...n.bounds, ...(n.shape && n.shape !== 'rect' ? {shape: n.shape} : {})});
  const jobs = eligible.map((edge, i) => ({edge, result: results[i]})).filter(j => j.result.level !== 'kept');
  let checked = 0, entries = 0, next = 0;
  const examples = [];
  async function work() {
    for (let j = jobs[next++]; j; j = jobs[next++]) {
      const {edge, result} = j;
      const all = [...edge.base, ...result.points];
      const box = {x0: Math.min(...all.map(p => p.x)) - 24, x1: Math.max(...all.map(p => p.x)) + 24, y0: Math.min(...all.map(p => p.y)) - 24, y1: Math.max(...all.map(p => p.y)) + 24};
      // Only the nodes near either route, with their containers: a node outside the box cannot be entered.
      const wanted = new Set([edge.source, edge.target]);
      for (const n of nodes) { const r = n.bounds; if (r.x <= box.x1 && r.x + r.width >= box.x0 && r.y <= box.y1 && r.y + r.height >= box.y0) wanted.add(n.id); }
      for (const id of [...wanted]) for (const p of view.chain(id)) wanted.add(p);
      const drawing = points => ({nodes: nodes.filter(n => wanted.has(n.id)).map(drawNode), edges: [{id: edge.id, source: edge.source, target: edge.target, points, sourcePort: null, targetPort: null}]});
      const hits = async points => {
        const {conflicts} = await engine({op: 'routeConflicts', drawing: drawing(points), edgeId: edge.id}, {stallMs: 5_000});
        return [...conflicts.lineNodeHits, ...conflicts.containerTraversals];
      };
      const drawn = await hits(result.points);
      if (drawn.length) {
        const base = new Set(await hits(edge.base));
        const added = drawn.filter(id => !base.has(id));
        entries += added.length;
        if (added.length && examples.length < 5) examples.push({edge: edge.id, nodes: added});
      }
      checked++;
    }
  }
  await Promise.all([work(), work()]);
  return {checked, entries, examples};
}

const rasters = width => [['2', 2], ['4', 4], ['8', 8], ['16', 16], ['32', 32], ['screen', width / 1600]];

function measure(view, eligible, results, others) {
  const counts = {full: 0, weakened: 0, kept: 0};
  const refusedFull = {}, refusedKept = {};
  for (const r of results) {
    counts[r.level]++;
    if (r.refused) refusedFull[r.refused] = (refusedFull[r.refused] ?? 0) + 1;
    if (r.level === 'kept') refusedKept[r.refused] = (refusedKept[r.refused] ?? 0) + 1;
  }
  const base = eligible.map(e => e.base), drawn = results.map(r => r.points);
  const ratios = results.map((r, i) => lengthOf(r.points) / lengthOf(eligible[i].base));
  const bundled = ratios.filter((_, i) => results[i].level !== 'kept');
  const width = view.scene.bounds.width;
  const ink = {};
  for (const [label, size] of rasters(width)) {
    ink[label] = {cell: size,
      eligible: inkCells(drawn, size) / inkCells(base, size),
      all: inkCells([...drawn, ...others], size) / inkCells([...base, ...others], size)};
  }
  return {...counts, share: eligible.length ? (counts.full + counts.weakened) / eligible.length : null,
    refusedFull, refusedKept, distortion: stats(ratios), distortionBundled: stats(bundled), ink,
    points: drawn.reduce((s, p) => s + p.length, 0), basePoints: base.reduce((s, p) => s + p.length, 0)};
}

/** Not a gate: every curve drawn at full strength through whatever it meets, to tell the method's limit from the guard's. */
function unguarded(view, eligible, offsets, variant, strength) {
  return eligible.map(edge => {
    const offset = offsets.get(edge.id);
    const polygon = straighten(controlPolygon(view, edge, variant), strength).map(p => ({x: p.x + offset.x, y: p.y + offset.y}));
    const curve = cut(bspline(polygon, segmentCount(polygon)), view.nodes.get(edge.source), view.nodes.get(edge.target));
    if (!curve) return {level: 'kept', points: edge.base, refused: 'end'};
    return {level: 'full', points: curve, refused: null};
  });
}

// ---------- Drawings ----------
const escape = text => String(text).replace(/[&<>"]/g, c => ({'&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;'})[c]);
const COLOURS = {plain: '#1f5fd1', full: '#1f5fd1', weakened: '#e07b00', kept: '#d01c3a', other: '#9aa3ad'};
function svg(view, eligible, results, {title, diagnostic = false, width = 2400}) {
  const {scene} = view;
  const b = scene.bounds;
  const scale = Math.min(width / b.width, 2400 / b.height);
  const w = Math.max(1, Math.round(b.width * scale)), h = Math.max(1, Math.round(b.height * scale));
  const unit = 1 / scale;
  const depth = id => view.chain(id).length;
  const containers = scene.nodes.filter(n => n.container).sort((p, q) => depth(p.id) - depth(q.id));
  const parts = [`<rect x="${b.x}" y="${b.y}" width="${b.width}" height="${b.height}" fill="#ffffff"/>`];
  for (const n of containers) {
    const r = n.bounds;
    const style = `fill="#6b7a8c" fill-opacity="0.07" stroke="#8b97a5" stroke-width="${unit}"`;
    parts.push(n.shape === 'circle' ? `<circle cx="${r.x + r.width / 2}" cy="${r.y + r.height / 2}" r="${Math.min(r.width, r.height) / 2}" ${style}/>` : `<rect x="${r.x}" y="${r.y}" width="${r.width}" height="${r.height}" ${style}/>`);
  }
  for (const n of scene.nodes.filter(n => !n.container)) {
    const r = n.bounds;
    const style = `fill="#ffffff" stroke="#3d4650" stroke-width="${unit}"`;
    parts.push(n.shape === 'circle' ? `<circle cx="${r.x + r.width / 2}" cy="${r.y + r.height / 2}" r="${Math.min(r.width, r.height) / 2}" ${style}/>` : `<rect x="${r.x}" y="${r.y}" width="${r.width}" height="${r.height}" ${style}/>`);
  }
  const index = new Map(eligible.map((e, i) => [e.id, i]));
  const many = scene.routes.length > 1000;
  for (const route of scene.routes) {
    const i = index.get(route.id);
    const result = i === undefined ? null : results?.[i];
    const points = result ? result.points : route.points;
    if (points.length < 2) continue;
    const colour = i === undefined ? COLOURS.other : diagnostic && result ? COLOURS[result.level] : COLOURS.plain;
    const d = points.map((p, k) => `${k ? 'L' : 'M'}${+p.x.toFixed(2)},${+p.y.toFixed(2)}`).join('');
    parts.push(`<path d="${d}" fill="none" stroke="${colour}" stroke-opacity="${many ? 0.28 : 0.7}" stroke-width="${(many ? 0.8 : 1.2) * unit}"/>`);
    if (!many) {
      const end = points.at(-1), from = [...points].reverse().find(p => Math.hypot(p.x - end.x, p.y - end.y) > 1e-6);
      if (from) {
        const len = Math.hypot(end.x - from.x, end.y - from.y), ux = (end.x - from.x) / len, uy = (end.y - from.y) / len, s = 7 * unit;
        parts.push(`<path d="M${end.x},${end.y}L${end.x - ux * s - uy * s / 2},${end.y - uy * s + ux * s / 2}L${end.x - ux * s + uy * s / 2},${end.y - uy * s - ux * s / 2}z" fill="${colour}" fill-opacity="0.8"/>`);
      }
    }
  }
  parts.push(`<text x="${b.x + 8 * unit}" y="${b.y + 18 * unit}" font-family="sans-serif" font-size="${15 * unit}" fill="#16191d" stroke="#ffffff" stroke-width="${3 * unit}" paint-order="stroke">${escape(title)}</text>`);
  return {text: `<svg xmlns="http://www.w3.org/2000/svg" width="${w}" height="${h}" viewBox="${b.x} ${b.y} ${b.width} ${b.height}">\n${parts.join('\n')}\n</svg>\n`, w, h};
}
function picture(base, drawing) {
  writeFileSync(`${base}.svg`, drawing.text);
  if (!existsSync(CHROME)) return false;
  for (let attempt = 1; attempt <= 3; attempt++) {
    const r = spawnSync(CHROME, ['--headless=new', '--disable-gpu', '--hide-scrollbars', `--user-data-dir=${drawings}/.chrome`, `--window-size=${drawing.w},${drawing.h}`, `--screenshot=${base}.png`, pathToFileURL(`${base}.svg`).href], {timeout: 120_000, killSignal: 'SIGKILL', stdio: 'ignore'});
    if (r.status === 0 && existsSync(`${base}.png`)) return true;
  }
  console.error(`no picture: ${base}`);
  return false;
}

// ---------- The control: Holten's picture, without the engine ----------
/** 128 leaves on a circle of radius 1000 in 4 groups of 4 subgroups; group and subgroup centres at a third and two thirds
 * of the radius; 200 seeded relations between different groups, straight as the base. No boxes are in the way. */
function control() {
  let seed = 133;
  const random = () => ((seed = (seed * 1664525 + 1013904223) >>> 0) / 4294967296);
  const at = (r, turn) => ({x: r * Math.cos(2 * Math.PI * turn), y: r * Math.sin(2 * Math.PI * turn)});
  const box = (id, p, parentId) => ({id, parentId, bounds: {x: p.x - 1, y: p.y - 1, width: 2, height: 2}});
  const nodes = [];
  for (let g = 0; g < 4; g++) {
    nodes.push(box(`g${g}`, at(333, (g + 0.5) / 4), null));
    for (let s = 0; s < 4; s++) {
      nodes.push(box(`g${g}s${s}`, at(667, (g * 4 + s + 0.5) / 16), `g${g}`));
      for (let l = 0; l < 8; l++) nodes.push(box(`n${g * 32 + s * 8 + l}`, at(1000, (g * 32 + s * 8 + l + 0.5) / 128), `g${g}s${s}`));
    }
  }
  const byId = new Map(nodes.map(n => [n.id, n]));
  const routes = [];
  while (routes.length < 200) {
    const a = Math.floor(random() * 128), b = Math.floor(random() * 128);
    if (Math.floor(a / 32) === Math.floor(b / 32)) continue;
    const ends = [a, b].map(i => { const r = byId.get(`n${i}`).bounds; return {x: r.x + 1, y: r.y + 1}; });
    routes.push({id: `c${String(routes.length).padStart(3, '0')}`, source: `n${a}`, target: `n${b}`, points: ends});
  }
  const view = viewOf({nodes, routes}, {straightEdges: routes.map(r => r.id), graph: {}, options: {}});
  const {eligible} = eligibility(view);
  const base = eligible.map(e => e.base);
  return STRENGTHS.map(strength => {
    const drawn = eligible.map(e => {
      const polygon = straighten(controlPolygon(view, e, 'centre'), strength);
      return bspline(polygon, segmentCount(polygon));
    });
    const ratios = drawn.map((p, i) => lengthOf(p) / lengthOf(base[i]));
    return {type: 'control', strength, edges: eligible.length, structure: structureOf(view, eligible), distortion: stats(ratios),
      ink: Object.fromEntries(rasters(2000).map(([label, size]) => [label, {cell: size, ratio: inkCells(drawn, size) / inkCells(base, size)}])),
      drawn, base, view};
  });
}

// ---------- Run ----------
const git = (...a) => execFileSync('git', a, {cwd: ROOT, encoding: 'utf8'}).trim();
const meta = {
  type: 'meta', adr: '0028', step: 'B0', startedAt: new Date().toISOString(),
  machine: {cpu: cpus()[0].model, logicalCpus: cpus().length, memoryBytes: totalmem(), os: `${platform()} ${release()}`, arch: arch()},
  revision: git('rev-parse', 'HEAD'),
  engineSourcesDirty: git('status', '--porcelain', '--', 'crates', 'Cargo.toml', 'Cargo.lock') !== '',
  probeDirty: git('status', '--porcelain', '--', 'benchmarks/eval') !== '',
  build: 'cargo build --release -p arqlet-runtime --bin layout-command --locked',
  rustc: execFileSync('rustc', ['-V'], {encoding: 'utf8'}).trim(), node: process.version,
  binarySha256: createHash('sha256').update(readFileSync(COMMAND)).digest('hex'),
  command: `node benchmarks/eval/bundling-probe.mjs ${process.argv.slice(2).join(' ')}`,
  loadAverageAtStart: loadavg(), gate: GATE, rules: RULES, views: names,
};
const lines = [meta];
const write = () => { mkdirSync(resolve(out, '..'), {recursive: true}); writeFileSync(out, lines.map(l => JSON.stringify(l)).join('\n') + '\n'); };
mkdirSync(drawings, {recursive: true});
const pages = [];

for (const {drawn, base, view, ...row} of control()) {
  lines.push(row);
  if (row.strength !== 0.85 && row.strength !== 1) continue;
  const box = {x: -1050, y: -1050, width: 2100, height: 2100};
  const shot = (file, paths, title) => {
    const text = `<svg xmlns="http://www.w3.org/2000/svg" width="1400" height="1400" viewBox="${box.x} ${box.y} ${box.width} ${box.height}"><rect x="${box.x}" y="${box.y}" width="${box.width}" height="${box.height}" fill="#ffffff"/>${paths.map(p => `<path d="${p.map((q, k) => `${k ? 'L' : 'M'}${q.x.toFixed(1)},${q.y.toFixed(1)}`).join('')}" fill="none" stroke="#1f5fd1" stroke-opacity="0.5" stroke-width="1.8"/>`).join('')}<text x="-1030" y="-1010" font-family="sans-serif" font-size="28">${escape(title)}</text></svg>\n`;
    picture(`${drawings}/${file}`, {text, w: 1400, h: 1400});
  };
  if (row.strength === 0.85) shot('control-unbundled', base, 'control (synthetic, no engine): unbundled');
  shot(`control-${row.strength}`, drawn, `control (synthetic, no engine): strength ${row.strength}`);
}

for (const name of names) {
  console.error(`== ${name}`);
  let loaded;
  try { loaded = await load(name); } catch (e) {
    lines.push({type: 'view', view: name, error: e.message}); write();
    console.error(`${name}: ${e.message}`);
    continue;
  }
  const {snapshot, scene} = loaded;
  const view = viewOf(scene, snapshot);
  const {eligible, excluded} = eligibility(view);
  const offsets = parallelOffsets(view, eligible);
  const eligibleIds = new Set(eligible.map(e => e.id));
  const others = scene.routes.filter(r => !eligibleIds.has(r.id)).map(r => r.points);
  const leaves = scene.nodes.filter(n => !n.container);
  const blockedBase = eligible.filter(e => leaves.some(n => n.id !== e.source && n.id !== e.target && enters(e.base[0], e.base[1], n))).length;
  const depths = scene.nodes.map(n => view.chain(n.id).length);
  lines.push({type: 'view', view: name, input: VIEWS[name], inputSha256: loaded.inputSha256, cachedScene: loaded.cached,
    nodes: loaded.nodes, edges: loaded.edges, routes: scene.routes.length, containers: scene.nodes.filter(n => n.container).length,
    depth: depths.reduce((a, b) => Math.max(a, b), 0), bounds: scene.bounds, requiredClearance: view.clearance,
    structure: structureOf(view, eligible),
    eligible: eligible.length, excluded, eligibleBaseThroughLeaf: blockedBase,
    routingIssues: (snapshot.routingIssues ?? []).length, geometryIssues: (snapshot.geometryIssues ?? []).length});
  write();
  const shots = [];
  const draw = (file, results, title, diagnostic = false) => {
    picture(`${drawings}/${name}-${file}`, svg(view, eligible, results, {title: `${name}: ${title}`, diagnostic}));
    shots.push({file: `${name}-${file}`, title});
  };
  draw('unbundled', null, 'unbundled');
  for (const variant of VARIANTS) {
    for (const strength of STRENGTHS) {
      const results = eligible.map(e => bundleEdge(view, e, offsets.get(e.id), variant, strength));
      const row = {type: 'run', view: name, variant, strength, guard: true, eligible: eligible.length, ...measure(view, eligible, results, others), ...verify(view, eligible, results)};
      if (strength === GATE.strength) {
        row.engine = await engineEntries(view, eligible, results);
        const fails = [];
        if (eligible.length < GATE.eligible) fails.push('fewer than 50 eligible');
        if (!(row.share >= GATE.bundled)) fails.push('bundled share');
        if (row.guardViolations || row.engine.entries) fails.push('new entry');
        if (row.incomplete) fails.push('incomplete');
        if (!(row.ink['2'].eligible <= GATE.ink)) fails.push('ink');
        if (!(row.distortion.mean <= GATE.distortion)) fails.push('distortion');
        row.gate = {pass: fails.length === 0, fails};
        draw(`${variant}-${strength}`, results, `${variant} control points, strength ${strength}, guard`);
        draw(`${variant}-${strength}-levels`, results, `${variant}, strength ${strength}: blue full, orange weakened, red kept, grey not eligible`, true);
      } else if (variant === 'centre') {
        draw(`${variant}-${strength}`, results, `${variant} control points, strength ${strength}, guard`);
      }
      lines.push(row); write();
      console.error(`${name} ${variant} ${strength}: bundled ${row.full}+${row.weakened}/${eligible.length}, ink ${row.ink['2'].eligible?.toFixed(3)}, distortion ${row.distortion.mean?.toFixed(3)}`);
      const free = unguarded(view, eligible, offsets, variant, strength);
      lines.push({type: 'run', view: name, variant, strength, guard: false, eligible: eligible.length, ...measure(view, eligible, free, others),
        entering: eligible.filter((e, i) => free[i].level !== 'kept' && refusal(view, e, free[i].points)).length});
      if (strength === GATE.strength) draw(`${variant}-${strength}-unguarded`, free, `${variant}, strength ${strength}, WITHOUT the guard (not the design)`);
      write();
    }
  }
  pages.push({name, shots});
}

pages.unshift({name: 'control', shots: [['control-unbundled', 'unbundled'], ['control-0.85', 'strength 0.85'], ['control-1', 'strength 1']].map(([file, title]) => ({file, title}))});
writeFileSync(`${drawings}/index.html`, `<!doctype html><meta charset="utf-8"><title>Bundling probe drawings</title>
<style>body{font:14px system-ui,sans-serif;margin:16px;background:#fff;color:#16191d}img{max-width:100%;border:1px solid #ccc}</style>
${pages.map(p => `<h2>${p.name}</h2>${p.shots.map(s => `<h3>${escape(s.title)}</h3><a href="${s.file}.svg"><img loading="lazy" src="${s.file}.svg" alt="${escape(s.title)}"></a>`).join('\n')}`).join('\n')}
`);

// ---------- Go or no-go ----------
const verdicts = VARIANTS.map(variant => {
  const rows = lines.filter(l => l.type === 'run' && l.guard && l.variant === variant && l.gate);
  const passing = rows.filter(r => r.gate.pass).map(r => r.view);
  return {variant, passing, counted: rows.filter(r => r.eligible >= GATE.eligible).length, go: passing.length >= GATE.views};
});
lines.push({type: 'verdict', verdicts, retries, loadAverageAtEnd: loadavg(), finishedAt: new Date().toISOString()});
write();

// ---------- Summary ----------
const f = (v, d = 3) => v === null || v === undefined || Number.isNaN(v) ? '–' : v.toFixed(d);
const pct = v => v === null || v === undefined ? '–' : `${(100 * v).toFixed(0)} %`;
const views = lines.filter(l => l.type === 'view');
const runs = lines.filter(l => l.type === 'run');
const md = [];
md.push('# ADR 0028 step B0: hierarchical edge bundling, probed outside the engine', '',
  `Data: \`${out.slice(ROOT.length)}\`. Machine: ${meta.machine.cpu}, ${meta.machine.logicalCpus} logical CPUs, ${Math.round(meta.machine.memoryBytes / 2 ** 30)} GiB, ${meta.machine.os} ${meta.machine.arch}.`,
  `Revision \`${meta.revision}\`${meta.engineSourcesDirty ? ' (engine sources modified)' : ''}${meta.probeDirty ? ' (probe modified)' : ''}; \`${meta.build}\`; ${meta.rustc}; Node ${meta.node}.`,
  `Command: \`${meta.command}\`.`,
  'No timing is reported: the measures are geometric and do not depend on the load.', '',
  '## Views', '',
  '| view | nodes | routes | containers | depth | eligible | not eligible, by first reason | eligible routes through a leaf |', '|---|---|---|---|---|---|---|---|');
for (const v of views) md.push(v.error ? `| ${v.view} | not measured: ${v.error} | | | | | | |` : `| ${v.view} | ${v.nodes} | ${v.routes} | ${v.containers} | ${v.depth} | ${v.eligible} | ${Object.entries(v.excluded).filter(e => e[1]).map(([k, n]) => `${k} ${n}`).join(', ') || 'none'} | ${v.eligibleBaseThroughLeaf} |`);
md.push('', '## What the hierarchies offer', '',
  'A section of a cubic spline is the same for two edges only where they share four consecutive control points. "Companion": another eligible edge between the same two outermost boxes.', '',
  '| view | eligible | control points per edge (points: edges) | with a companion | with a shared section | containers on paths | centre covered by a child | free place differs from centre |', '|---|---|---|---|---|---|---|---|');
for (const v of views.filter(v => !v.error)) md.push(`| ${v.view} | ${v.eligible} | ${Object.entries(v.structure.controlPoints).map(([k, n]) => `${k}: ${n}`).join(', ')} | ${v.structure.withCompanion} | ${v.structure.withSharedSection} | ${v.structure.containersOnPaths} | ${v.structure.centreCovered} | ${v.structure.freeMoved} |`);
md.push('', `## Gate (strength ${GATE.strength}, with the guard)`, '',
  `Pass: ${GATE.eligible} eligible edges or more, bundled ≥ ${pct(GATE.bundled)}, no new entry, ink ≤ ${GATE.ink}, mean distortion ≤ ${GATE.distortion}. Ink: eligible edges, 2-unit raster. Distortion: all eligible edges.`, '',
  '| view | control points | eligible | full | weakened | kept | bundled | new entries (probe / engine) | ink | distortion mean / median / max | verdict |', '|---|---|---|---|---|---|---|---|---|---|---|');
for (const r of runs.filter(r => r.guard && r.gate)) md.push(`| ${r.view} | ${r.variant} | ${r.eligible} | ${r.full} | ${r.weakened} | ${r.kept} | ${pct(r.share)} | ${r.guardViolations} / ${r.engine.entries} | ${f(r.ink['2'].eligible)} | ${f(r.distortion.mean)} / ${f(r.distortion.median)} / ${f(r.distortion.max, 2)} | ${r.gate.pass ? 'pass' : `fail: ${r.gate.fails.join(', ')}`} |`);
md.push('', '| control points | gate views passing | go |', '|---|---|---|');
for (const v of verdicts) md.push(`| ${v.variant} | ${v.passing.length} of ${v.counted} with ${GATE.eligible}+ eligible${v.passing.length ? ` (${v.passing.join(', ')})` : ''} | ${v.go ? '**go**' : '**no-go**'} |`);
md.push('', '## Other readings of the gate measures (they decide nothing)', '',
  '| view | control points | ink, eligible: 4 / 8 / 16 / 32 / screen cell | ink, all routes: 2 / 4 / 8 / screen | distortion of the bundled only, mean | refused at full strength | points, bundled / unbundled |', '|---|---|---|---|---|---|---|');
for (const r of runs.filter(r => r.guard && r.gate)) md.push(`| ${r.view} | ${r.variant} | ${f(r.ink['4'].eligible)} / ${f(r.ink['8'].eligible)} / ${f(r.ink['16'].eligible)} / ${f(r.ink['32'].eligible)} / ${f(r.ink.screen.eligible)} (${f(r.ink.screen.cell, 1)} u) | ${f(r.ink['2'].all)} / ${f(r.ink['4'].all)} / ${f(r.ink['8'].all)} / ${f(r.ink.screen.all)} | ${f(r.distortionBundled.mean)} | ${Object.entries(r.refusedFull).map(([k, n]) => `${k} ${n}`).join(', ') || 'none'} | ${r.points} / ${r.basePoints} |`);
md.push('', '## Control: a deep hierarchy on a circle (synthetic, no engine, no guard)', '',
  '128 leaves on a circle in 4 × 4 groups, 200 relations between groups, centre control points. It is not a gate view; it shows what the ink measure reaches where the method has what it was made for.', '',
  '| strength | edges with a shared section | ink: 2 / 4 / 8 / 16 / 32 / screen (1.3 u) | distortion, mean |', '|---|---|---|---|');
for (const c of lines.filter(l => l.type === 'control')) md.push(`| ${c.strength} | ${c.structure.withSharedSection} of ${c.edges} | ${['2', '4', '8', '16', '32', 'screen'].map(k => f(c.ink[k].ratio)).join(' / ')} | ${f(c.distortion.mean)} |`);
md.push('', '## Strengths, with and without the guard', '', 'Without the guard every curve is drawn at the full strength through whatever it meets. It is not the design; it shows what the guard costs and what the method could reach.', '',
  '| view | control points | strength | bundled, guard | ink, guard (2 / screen) | distortion, guard | ink, no guard (2 / screen) | distortion, no guard | curves adding a hit, no guard |', '|---|---|---|---|---|---|---|---|---|');
for (const r of runs.filter(r => r.guard)) {
  const u = runs.find(x => !x.guard && x.view === r.view && x.variant === r.variant && x.strength === r.strength);
  md.push(`| ${r.view} | ${r.variant} | ${r.strength} | ${pct(r.share)} | ${f(r.ink['2'].eligible)} / ${f(r.ink.screen.eligible)} | ${f(r.distortion.mean)} | ${f(u.ink['2'].eligible)} / ${f(u.ink.screen.eligible)} | ${f(u.distortion.mean)} | ${u.entering} of ${u.eligible} |`);
}
writeFileSync(out.replace(/\.jsonl$/, '') + '.md', md.join('\n') + '\n');
console.log(md.join('\n'));
console.error(`drawings: ${drawings}/index.html; retries: ${retries.length}`);
