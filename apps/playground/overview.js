// Cluster overview page (ADR 0018): the `overview` operation in the browser (WASM worker) or the backend, drawn with
// WebGL. Every cluster is shown collapsed, organic or structured. The representation follows the user's decisions first
// (the panel's buttons, shift-click), then the rules of the data (by node kind, from the data's config set), then the
// zoom (`representationsOf`). Only a decision of the user makes a structured cluster grow and its neighbours make room;
// what the zoom or a rule structures is fitted into the disc it has, so that zooming never makes the drawing jump.
// Stale replies are dropped.
import {overviewModel, viewOf, structuredExcerpt, resolvedEdgeIds, frameOf, representationsOf, flowDirectionOf, displayName} from '@arqlet/viewer/overview.js';
import {createOverviewRenderer, bindOverviewNavigation} from '@arqlet/viewer/overview-gl.js';
import {createEngineWorker} from '@arqlet/engine-client/engine.js';

const $ = id => document.getElementById(id);
/** Milliseconds of a move to a new layout, and of a blend between representations. */
const MORPH = 480, FADE = 240;
const status = text => { $('status').textContent = text; };
let worker = null, requests = 0;
const pending = new Map();
let graph = null, model = null, renderer = null, unbind = null, focus = -1, selected = -1;
// `epoch` changes with every new overview: replies for an older one are dropped.
let epoch = 0;
/** The user's decisions (node id → collapsed, organic or structured) and flow directions (node id → lr, rl, tb, bt). */
const decisions = new Map(), directions = new Map();
/** Layered layouts by `id|direction`, and the keys whose layout failed. */
const scenes = new Map(), failed = new Set();
/** Clusters drawn structured now (id → layout key), clusters the overview gave the room of their layout (ids), clusters
 * collapsed now (indices). */
let shown = new Map(), grown = new Set(), collapsed = new Set();
/** The data's config set (its `overview` part holds the rules), the rules now, and the undo and redo stacks. */
let baseConfig = null, rules = {};
const undo = [], redo = [];

// Settings of the panel, remembered in this browser (a convenience; the page works without).
const LAYOUT = ['node-radius', 'gap', 'padding', 'charge', 'gravity', 'communities'];
const ZOOM = ['collapse-px', 'structure-share', 'limit', 'scene-unit'];
const DEFAULTS = {'node-radius': 4, gap: 2, padding: 6, charge: 0.05, gravity: 0.1, communities: 120,
  'collapse-px': 14, 'structure-share': 35, limit: 300, 'scene-unit': 0.2};
const STORE = 'arqlet-overview-settings';
function loadSettings() {
  try {
    const saved = JSON.parse(localStorage.getItem(STORE) ?? '{}');
    for (const id of [...LAYOUT, ...ZOOM]) if (saved[id] !== undefined) $(id).value = saved[id];
    if (saved.auto !== undefined) $('auto').checked = saved.auto;
  } catch { /* no storage: the defaults */ }
}
function saveSettings() {
  try {
    const out = {auto: $('auto').checked};
    for (const id of [...LAYOUT, ...ZOOM]) out[id] = $(id).value;
    localStorage.setItem(STORE, JSON.stringify(out));
  } catch { /* no storage */ }
}
const number = id => Number($(id).value);
const limit = () => number('limit');

function engineWorker() {
  if (worker) return worker;
  worker = createEngineWorker('/generated/arqlet.wasm');
  worker.onmessage = ({data}) => {
    if (data.publication !== undefined) return;
    pending.get(data.requestId)?.(data);
    pending.delete(data.requestId);
  };
  return worker;
}

/** Runs one engine request where the mode says: in the worker or through the backend. */
async function execute(request) {
  if ($('mode').value === 'backend') {
    const r = await fetch('/api/op', {method: 'POST', headers: {'Content-Type': 'application/json'}, body: JSON.stringify(request)});
    return r.json();
  }
  const requestId = ++requests;
  return new Promise(resolve => { pending.set(requestId, resolve); engineWorker().postMessage({requestId, request}); });
}

async function sources() {
  const list = [{label: 'Demo graph', url: '/generated/graph.json'}];
  try {
    const index = await (await fetch('/generated/overview/index.json')).json();
    for (const d of index) {
      list.push({label: `${d.title} (${d.nodes.toLocaleString()} nodes)`, url: `/generated/overview/${d.file}`,
        config: d.configSet ? `/generated/overview/${d.configSet}` : null});
    }
  } catch { /* no generated datasets: the demo graph only */ }
  $('source').replaceChildren(...list.map(s => {
    const option = new Option(s.label, s.url);
    option.dataset.config = s.config ?? '';
    return option;
  }));
  if (list.length > 1) $('source').selectedIndex = 1;
}

/** The data's config set: the one the dataset names, else the shipped preset `auto`. Its rules fill the rules table. */
async function loadConfig(url = $('source').selectedOptions[0]?.dataset.config) {
  try {
    baseConfig = url ? await (await fetch(url)).json()
      : (await (await fetch('/generated/presets/index.json')).json()).find(p => p.id === 'auto')?.configSet ?? null;
  } catch { baseConfig = null; }
  rules = {...(baseConfig?.overview?.rules ?? {})};
  if (baseConfig?.overview?.auto !== undefined) $('auto').checked = baseConfig.overview.auto;
}

/** The layout options, the config set and the intent a request carries; the engine checks them (ADR 0015). */
function request(extra = {}) {
  const options = {nodeRadius: number('node-radius'), gap: number('gap'), padding: number('padding'),
    charge: number('charge'), gravity: number('gravity'), communities: number('communities')};
  const values = {op: 'overview', graph, threads: 4, options, intent: {decisions: Object.fromEntries(decisions)}, ...extra};
  if (baseConfig) values.configSet = {...baseConfig, overview: {rules, auto: $('auto').checked}};
  return values;
}

/** A new layout of the graph. The user's decisions are kept unless `fresh` (a new graph). */
async function layOut(fresh = true) {
  const ticket = ++epoch;
  $('run').disabled = $('relayout').disabled = true;
  scenes.clear(); failed.clear(); shown = new Map(); grown = new Set(); collapsed = new Set(); roomKey = '';
  if (fresh) { decisions.clear(); directions.clear(); undo.length = 0; redo.length = 0; selected = -1; }
  try {
    if (!graph) { graph = await (await fetch($('source').value)).json(); await loadConfig(); }
    status(`Laying out ${graph.nodes.length.toLocaleString()} nodes, ${graph.edges.length.toLocaleString()} edges …`);
    const started = performance.now();
    const result = await execute(request());
    if (ticket !== epoch) return;
    if (!result.ok) throw Error(result.error);
    const elapsed = performance.now() - started;
    show(result.overview);
    const s = result.overview.stats;
    status(`${s.nodes.toLocaleString()} nodes in ${s.containers.toLocaleString()} clusters` +
      (s.communities ? ` (${s.communities} found as communities)` : '') + `, ${s.edges.toLocaleString()} edges` +
      ` (${s.unresolvedEdges} unresolved left out) · ${(elapsed / 1000).toFixed(2)} s in this browser, including transfer`);
    const unknown = result.overview.representation?.unknownKinds ?? [];
    if (unknown.length) status(`Rules for kinds this graph does not have: ${unknown.join(', ')}`);
  } catch (e) {
    if (ticket === epoch) status(e.message);
  } finally {
    if (ticket === epoch) $('run').disabled = $('relayout').disabled = false;
  }
}

/**
 * Draws an overview. A continuation (`keep`) of the same graph is animated: the drawing moves to its new places, and the
 * view moves with `anchor` (a node index), so that this cluster stays where it is on screen; the zoom stays as the user
 * set it.
 */
function show(overview, keep = false, anchor = -1, showWhole = -1) {
  const next = overviewModel(overview, resolvedEdgeIds(graph));
  if (keep && renderer) {
    const view = renderer.view, o = model.overview, c = $('canvas');
    // One move: to the new layout and, for a cluster the user just structured, to the view that shows it whole.
    const target = showWhole >= 0 ? viewOf(next, showWhole, c.clientWidth, c.clientHeight)
      : anchor >= 0
        ? {...view, centreX: view.centreX + overview.x[anchor] - o.x[anchor], centreY: view.centreY + overview.y[anchor] - o.y[anchor]}
        : view;
    model = next;
    renderer.morph(next, {duration: MORPH, view: target});
    renderRules();
    renderPanel();
    crumbs();
    return;
  }
  unbind?.(); renderer?.destroy();
  model = next;
  renderer = createOverviewRenderer($('canvas'), model, {labels: $('labels'), onView: schedule});
  unbind = bindOverviewNavigation($('canvas'), renderer, {onClick: select, onHover: hover, onDoubleClick: zoomInto, lens: lensSettings, idle: idleSettings()});
  focus = -1;
  zoomTo(-1);
  renderRules();
  renderPanel();
  crumbs();
}

/** The key of a cluster's layered layout: its id and the flow it is laid out in. */
function sceneKey(id) {
  const direction = directions.get(id) ?? flowDirectionOf(model, model.index.get(id)) ?? '';
  return `${id}|${direction}`;
}

/** The overview's radius for a cluster the user structured: its layered layout's box at the panel's size. */
function radiusOf(id, key) {
  const frame = frameOf(model, model.index.get(id), scenes.get(key));
  return (Math.hypot(frame.width, frame.height) / 2) * number('scene-unit') / 0.94 + 2;
}

// The policy runs after the view has rested for a moment, and one change at a time; a change meanwhile runs it again.
let timer = 0, busy = false, dirty = false, pointer = null, zoomAfter = null, roomKey = '';
function schedule() { clearTimeout(timer); timer = setTimeout(evaluate, 140); }

async function evaluate() {
  if (!model) return;
  if (busy) { dirty = true; return; }
  const c = $('canvas');
  const current = new Set([...shown.keys()].map(id => model.index.get(id)));
  // The focus is where the pointer was when the view last changed under it; else the view's centre.
  const focusPoint = pointer ? renderer.world(...pointer) : null;
  const wanted = representationsOf(model, renderer.view, c.clientWidth, c.clientHeight, {
    decisions, rules, auto: $('auto').checked, limit: limit(), current, focus: focusPoint,
    collapsePixels: number('collapse-px'), structureShare: number('structure-share') / 100,
  });
  if (!sameSet(wanted.collapsed, collapsed)) { collapsed = wanted.collapsed; redraw(FADE); }
  const want = new Map([...wanted.structured].map(i => [model.overview.ids[i], sceneKey(model.overview.ids[i])]));
  const add = [...want].filter(([id, key]) => shown.get(id) !== key && !failed.has(key)).map(([id]) => id);
  const remove = [...shown.keys()].filter(id => !want.has(id));
  // Only what the user structured gets the room of its layout; the key records which layouts and at which size.
  const user = id => decisions.get(id) === 'structured';
  const growKey = () => [...shown].filter(([id]) => user(id)).map(([id, key]) => `${key}@${number('scene-unit')}`).sort().join(',');
  if (!add.length && !remove.length && growKey() === roomKey) { renderPanel(); return; }
  busy = true;
  try {
    const ticket = epoch;
    const next = new Map([...shown].filter(([id]) => want.has(id) && !add.includes(id)));
    // At most four new layered layouts per step; the next evaluation takes the rest.
    for (const id of add.slice(0, 4)) {
      const key = want.get(id);
      if (!scenes.has(key)) {
        status(`${displayName(id)}: layered layout …`);
        const scene = await layered(id, key);
        if (ticket !== epoch) return;
        if (!scene) { failed.add(key); continue; }
        scenes.set(key, scene);
      }
      next.set(id, key);
    }
    if (add.length > 4) dirty = true;
    shown = next;
    // The change of representation blends in first (a new layout in the disc it has), then the room changes, moving.
    redraw(FADE);
    const nextRoom = growKey();
    if (nextRoom !== roomKey) {
      // A decision of the user changed the room a cluster needs: the overview makes room, nothing else moves.
      const anchor = model.index.get([...shown.keys()].find(user) ?? add[0] ?? remove[0]);
      const o = model.overview;
      const radii = Object.fromEntries([...shown].filter(([id]) => user(id)).map(([id, key]) => [id, radiusOf(id, key)]));
      // A cluster that gives room back lets its neighbours settle into it; the move is animated, so they may.
      const settle = [...grown].some(id => !(id in radii) || radii[id] < model.overview.r[model.index.get(id)] - 0.01);
      const result = await execute(request({radii, start: {x: o.x, y: o.y, r: o.r, settle}}));
      if (ticket !== epoch) return;
      if (!result.ok) { status(result.error); return; }
      roomKey = nextRoom;
      grown = new Set(Object.keys(radii));
      const into = zoomAfter && grown.has(zoomAfter) ? model.index.get(zoomAfter) : -1;
      if (into >= 0) { focus = into; pointer = null; }
      show(result.overview, true, anchor, into);
    }
    zoomAfter = null;
    const changes = [...add.slice(0, 4).map(id => `${displayName(id)}: structured`), ...remove.map(id => `${displayName(id)}: open`)];
    if (changes.length) status(changes.join(' · '));
    renderPanel();
  } finally {
    busy = false;
    if (dirty) { dirty = false; schedule(); }
  }
}

/** The layered layout of a cluster's excerpt (with border ports, in its flow), or null with the error in the status line. */
async function layered(id, key) {
  const direction = key.split('|').at(-1) || null;
  const {graph: excerpt} = structuredExcerpt(graph, model, id, {direction});
  // isolatedGrid: a file's unrelated functions stand in a grid instead of one long column.
  const imported = await execute({op: 'import', graph: excerpt, limit: excerpt.nodes.length, options: {isolatedGrid: true}});
  const scene = imported.ok ? await execute({op: 'scene', snapshot: imported.snapshot}) : imported;
  if (!scene.ok) { status(`${displayName(id)}: ${scene.error}`); return null; }
  return scene.scene;
}

function sameSet(a, b) { return a.size === b.size && [...a].every(x => b.has(x)); }

/** Draws the representations now; with `fade` (milliseconds) the change blends in. */
function redraw(fade = 0) {
  const expanded = new Map();
  for (const [id, key] of shown) if (scenes.has(key)) expanded.set(model.index.get(id), {scene: scenes.get(key)});
  renderer.setState({expanded, collapsed, selected}, {fade});
}

/** Shows cluster `i` (-1: everything); `smooth` moves there over the morph's time. */
function zoomTo(i, smooth = false) {
  // A zoom to a cluster focuses its centre, not where the pointer last was.
  pointer = null;
  const c = $('canvas');
  const view = viewOf(model, i, c.clientWidth, c.clientHeight);
  if (smooth) renderer.morph(model, {duration: MORPH, view});
  else { renderer.view = view; renderer.resize(); }
  schedule();
}

function label(i) { return model.overview.ids[i]; }
function kindOf(i) { const o = model.overview; return o.kinds && o.kind ? o.kinds[o.kind[i]] : null; }
function clusterOf(i) { return i < 0 ? -1 : model.children[i].length ? i : model.overview.parent[i]; }

/** How cluster `i` is shown now, and why. */
function representationOf(i) {
  const id = label(i);
  const now = shown.has(id) ? 'structured' : collapsed.has(i) ? 'collapsed' : 'open';
  const why = decisions.has(id) ? 'your choice' : rules[kindOf(i)] ? `rule for ${kindOf(i)}` : $('auto').checked ? 'by zoom' : 'default';
  return {now, why};
}

function hover(i, e) {
  const tip = $('tip');
  const scene = renderer.pickScene(e.clientX, e.clientY);
  let text = null;
  if (scene?.node) text = `${scene.node.label || scene.node.id} · ${scene.node.kind}`;
  else if (i >= 0) {
    const kids = model.children[i].length;
    const name = label(i).startsWith('community:') ? `community ${displayName(label(i))}` : label(i);
    const {now, why} = kids ? representationOf(i) : {};
    text = kids ? `${name} · ${model.size[i].toLocaleString()} nodes · ${now}, ${why}` : label(i);
  }
  if (!text) { tip.style.display = 'none'; return; }
  tip.textContent = text;
  const rect = $('stage').getBoundingClientRect();
  tip.style.left = `${e.clientX - rect.left + 12}px`; tip.style.top = `${e.clientY - rect.top + 12}px`;
  tip.style.display = 'block';
}

/** A click selects a cluster (a leaf: its cluster); shift-click collapses it or opens it again. */
function select(i, event) {
  const cluster = clusterOf(i);
  if (event?.shiftKey && cluster >= 0) {
    decide(cluster, collapsed.has(cluster) || decisions.get(label(cluster)) === 'collapsed' ? 'organic' : 'collapsed');
    return;
  }
  selected = cluster;
  redraw();
  renderPanel();
}

/** A double-click zooms into a cluster. */
function zoomInto(i) {
  const cluster = clusterOf(i);
  if (cluster < 0) return;
  selected = focus = cluster;
  redraw();
  zoomTo(cluster, true);
  crumbs();
  renderPanel();
}

/** Records the decisions before a change, for undo. */
function remember() {
  undo.push({decisions: new Map(decisions), directions: new Map(directions)});
  if (undo.length > 50) undo.shift();
  redo.length = 0;
  updateHistory();
}

function updateHistory() { $('undo').disabled = !undo.length; $('redo').disabled = !redo.length; }

/** The user's decision for cluster `i` ('': automatic again). */
function decide(i, representation) {
  const id = label(i);
  if ((decisions.get(id) ?? '') === representation) return;
  remember();
  if (representation) decisions.set(id, representation); else decisions.delete(id);
  // A cluster the user structures is shown whole once it has made its room.
  if (representation === 'structured') zoomAfter = id;
  selected = i;
  schedule();
  renderPanel();
}

function renderPanel() {
  const i = selected, buttons = [...document.querySelectorAll('[data-rep]')];
  $('zoom-cluster').disabled = $('direction').disabled = i < 0 || !model;
  updateHistory();
  if (i < 0 || !model) {
    $('cluster-name').textContent = 'Click a cluster to select it.';
    $('cluster-info').textContent = 'Double-click zooms into it. Shift-click collapses or opens it.';
    for (const b of buttons) { b.disabled = true; b.setAttribute('aria-pressed', 'false'); }
    return;
  }
  const id = label(i), {now, why} = representationOf(i), decided = decisions.get(id) ?? '';
  $('cluster-name').textContent = id.startsWith('community:') ? `Community ${displayName(id)}` : id;
  $('cluster-info').textContent = `${model.size[i].toLocaleString()} nodes, ${model.children[i].length} children` +
    (kindOf(i) ? `, kind ${kindOf(i)}` : '') + ` · now ${now}, ${why}`;
  for (const b of buttons) {
    b.setAttribute('aria-pressed', String(b.dataset.rep === decided));
    b.disabled = b.dataset.rep === 'structured' && model.size[i] > limit();
  }
  $('direction').value = directions.get(id) ?? '';
}

/** One row per kind of container in the graph: automatic, or a representation for every cluster of that kind. */
function renderRules() {
  const o = model.overview, kinds = new Set();
  for (let i = 0; i < model.n; i++) if (model.children[i].length && o.kinds) kinds.add(o.kinds[o.kind[i]]);
  for (const kind of Object.keys(rules)) kinds.add(kind);
  $('rules').replaceChildren(...[...kinds].sort().map(kind => {
    const row = document.createElement('tr'), name = document.createElement('td'), cell = document.createElement('td');
    name.textContent = kind;
    const choice = document.createElement('select');
    choice.setAttribute('aria-label', `Rule for ${kind}`);
    for (const [value, text] of [['', 'automatic'], ['collapsed', 'collapsed'], ['organic', 'open'], ['structured', 'structured']]) choice.add(new Option(text, value));
    choice.value = rules[kind] ?? '';
    choice.onchange = () => { if (choice.value) rules[kind] = choice.value; else delete rules[kind]; schedule(); renderPanel(); };
    cell.append(choice);
    row.append(name, cell);
    return row;
  }));
}

function crumbs() {
  const path = [];
  for (let i = focus; i >= 0; i = model.overview.parent[i]) path.unshift(i);
  const button = (text, i) => { const b = document.createElement('button'); b.textContent = text; b.onclick = () => { focus = i; zoomTo(i, true); crumbs(); }; return b; };
  $('crumbs').replaceChildren(button('All', -1), ...path.map(i => button(displayName(label(i)), i)));
}

function step(from, to) {
  if (!from.length || !model) return;
  to.push({decisions: new Map(decisions), directions: new Map(directions)});
  const back = from.pop();
  decisions.clear(); directions.clear();
  for (const [id, value] of back.decisions) decisions.set(id, value);
  for (const [id, value] of back.directions) directions.set(id, value);
  schedule();
  renderPanel();
}

$('run').onclick = () => layOut(true);
$('relayout').onclick = () => { saveSettings(); layOut(false); };
$('defaults').onclick = () => {
  for (const [id, value] of Object.entries(DEFAULTS)) $(id).value = value;
  saveSettings();
  schedule();
};
for (const id of ZOOM) $(id).onchange = () => { saveSettings(); schedule(); renderPanel(); };
$('auto').onchange = () => { saveSettings(); schedule(); renderPanel(); };
for (const b of document.querySelectorAll('[data-rep]')) b.onclick = () => { if (selected >= 0) decide(selected, b.dataset.rep); };
$('direction').onchange = () => {
  if (selected < 0) return;
  remember();
  const id = label(selected);
  if ($('direction').value) directions.set(id, $('direction').value); else directions.delete(id);
  schedule();
};
$('zoom-cluster').onclick = () => { if (selected >= 0) zoomInto(selected); };
$('reset').onclick = () => { remember(); decisions.clear(); directions.clear(); schedule(); renderPanel(); };
$('undo').onclick = () => step(undo, redo);
$('redo').onclick = () => step(redo, undo);
$('toggle-panel').onclick = () => {
  const hidden = document.body.classList.toggle('hide-panel');
  $('toggle-panel').setAttribute('aria-pressed', String(!hidden));
  renderer?.resize();
};
$('source').onchange = () => { graph = null; };
$('file').onchange = async e => {
  const f = e.target.files?.[0];
  if (!f) return;
  graph = JSON.parse(await f.text());
  // A file brings no config set of its own: the shipped preset's.
  await loadConfig(null);
  layOut(true);
};
$('canvas').addEventListener('wheel', e => { pointer = [e.clientX, e.clientY]; });
$('canvas').addEventListener('pointerleave', () => { pointer = null; });
addEventListener('keydown', e => {
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'z') { e.preventDefault(); if (e.shiftKey) step(redo, undo); else step(undo, redo); return; }
  if (e.key === 'Escape' && model && focus >= 0) { focus = model.overview.parent[focus]; zoomTo(focus); crumbs(); }
});
addEventListener('resize', () => { renderer?.resize(); schedule(); });
loadSettings();
await sources();
renderPanel();
status('Choose a graph and lay it out.');

/** The lens of the panel (radius in CSS pixels, magnification), or null when it is off. */
function lensSettings() {
  if (!$('lens').checked) return null;
  return {radius: Number($('lens-radius').value) || 180, magnification: Math.max(Number($('lens-magnification').value) || 3, 1.01)};
}
$('lens').addEventListener('change', () => { if (!$('lens').checked) renderer?.setLens(null); });
// L switches the lens, unless a field has the keyboard.
document.addEventListener('keydown', e => {
  if (e.key.toLowerCase() !== 'l' || e.metaKey || e.ctrlKey || e.altKey || /INPUT|SELECT|TEXTAREA/.test(document.activeElement?.tagName ?? '')) return;
  $('lens').checked = !$('lens').checked;
  if (!$('lens').checked) renderer?.setLens(null);
});

/** Breathing when idle, from the panel; null when it is off. */
function idleSettings() {
  if (!$('breathe').checked) return null;
  return {after: 1000 * (Number($('breathe-after').value) || 8), breathe: {amplitude: (Number($('breathe-amplitude').value) || 6) / 100}};
}
// A change of the idle settings takes effect at once: the navigation is bound again with them.
for (const id of ['breathe', 'breathe-after', 'breathe-amplitude']) {
  $(id).addEventListener('change', () => {
    if (!renderer) return;
    unbind?.();
    unbind = bindOverviewNavigation($('canvas'), renderer, {onClick: select, onHover: hover, onDoubleClick: zoomInto, lens: lensSettings, idle: idleSettings()});
  });
}
