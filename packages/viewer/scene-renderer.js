// Provisional (ADR 0025): what both GPU backends of the far layer share. A backend (`scene-gpu.js` with WebGPU, the
// first choice; `scene-gl.js` with WebGL 2, the fallback) only uploads the buffers of `sceneBuffers` and draws them for a
// view; the shell here holds the view, the state, the texts on the 2D canvas, picking and the frame scheduling.
import {sceneBuffers, hoverBuffers, morphStarts, viewRect, parseColour} from './scene-buffers.js';

/**
 * The backend a browser offers for the far layer: WebGPU (with its device), else WebGL 2, else none (SVG alone).
 * `prefer: 'webgl2'` skips WebGPU.
 */
export async function sceneBackend({prefer, doc = globalThis.document} = {}) {
  if (prefer !== 'webgl2' && prefer !== 'svg' && globalThis.navigator?.gpu) {
    try {
      const adapter = await navigator.gpu.requestAdapter();
      const device = adapter && await adapter.requestDevice();
      if (device) return {kind: 'webgpu', device};
    } catch { /* fall back */ }
  }
  if (prefer !== 'svg') {
    try { if (doc?.createElement('canvas').getContext('webgl2')) return {kind: 'webgl2'}; } catch { /* none */ }
  }
  return {kind: 'svg'};
}

// The labels of a fixed screen size (CSS pixels) where a text is too small at its own size: container titles inside
// their boxes, then the names of leaves over their boxes (they may reach past a small box, up to LEAF_ROOM pixels), each
// only where it does not overlap a label drawn before. A thin halo in the contrasting tone keeps them readable over lines.
const LEAF_ROOM = 140;
const LOD_PIXELS = {container: 11, leaf: 10};
const MAX_LOD_LABELS = 3000;

function drawLodLabels(ctx, texts, view, w, h, rect, {minTextPixels, font}) {
  const s = view.scale, toScreen = (x, y) => [w / 2 + (x - view.centreX) * s, h / 2 + (y - view.centreY) * s];
  const taken = new Map(), CELL = 48;
  const free = (x0, y0, x1, y1) => {
    for (let gy = Math.floor(y0 / CELL); gy <= Math.floor(y1 / CELL); gy++) for (let gx = Math.floor(x0 / CELL); gx <= Math.floor(x1 / CELL); gx++)
      for (const r of taken.get(gx * 100003 + gy) ?? []) if (x0 < r[2] && r[0] < x1 && y0 < r[3] && r[1] < y1) return false;
    return true;
  };
  const take = r => {
    for (let gy = Math.floor(r[1] / CELL); gy <= Math.floor(r[3] / CELL); gy++) for (let gx = Math.floor(r[0] / CELL); gx <= Math.floor(r[2] / CELL); gx++) {
      const key = gx * 100003 + gy;
      if (!taken.has(key)) taken.set(key, []);
      taken.get(key).push(r);
    }
  };
  // A text too small at its own size; and a container's title whose own place is out of view (it follows the view).
  const outside = t => t.x < rect.x || t.x > rect.x + rect.width || t.y < rect.y || t.y > rect.y + rect.height;
  const candidates = texts.filter(t => t.lod && (t.size * s < minTextPixels || (t.lod.kind === 'container' && outside(t)))).filter(t => {
    const b = t.lod.box;
    return b.x <= rect.x + rect.width && b.x + b.width >= rect.x && b.y <= rect.y + rect.height && b.y + b.height >= rect.y;
  });
  // Containers first, the larger before the smaller; then leaves.
  candidates.sort((a, b) => (a.lod.kind === b.lod.kind ? b.lod.box.width * b.lod.box.height - a.lod.box.width * a.lod.box.height : a.lod.kind === 'container' ? -1 : 1));
  ctx.textBaseline = 'middle';
  let drawn = 0;
  for (const t of candidates) {
    if (drawn >= MAX_LOD_LABELS) break;
    const px = LOD_PIXELS[t.lod.kind], b = t.lod.box, [x0, y0] = toScreen(b.x, b.y), bw = b.width * s, bh = b.height * s;
    const container = t.lod.kind === 'container';
    if (container ? bh < px + 2 || bw < px * 2 : bw < 6 || bh < 3) continue;
    ctx.font = `${t.lod.kind === 'container' ? 600 : 400} ${px}px ${font}`;
    let label = t.text, width = ctx.measureText(label).width;
    const room = container ? bw - 6 : Math.max(bw - 4, LEAF_ROOM);
    // Too long for its box: shortened with an ellipsis, down to four characters.
    while (width > room && label.length > 4) { label = label.slice(0, -2) + '…'; width = ctx.measureText(label).width; }
    if (width > room) continue;
    // A container's title at the top left of the part of it in view (it stays in sight while the view moves inside a
    // large container), a leaf's name in its middle.
    const vx = Math.max(x0, 0), vy = Math.max(y0, 0);
    if (container && (vx + width + 8 > x0 + bw || vy + px + 4 > y0 + bh)) continue;
    const lx = container ? vx + 4 : x0 + bw / 2 - width / 2, ly = container ? vy + px * 0.5 + 3 : y0 + bh / 2;
    const r = [lx - 1, ly - px * 0.6, lx + width + 1, ly + px * 0.6];
    if (!free(...r)) continue;
    take(r);
    ctx.textAlign = 'left';
    ctx.lineJoin = 'round';
    ctx.lineWidth = 3;
    ctx.strokeStyle = haloOf(t.colour);
    ctx.strokeText(label, lx, ly);
    ctx.fillStyle = t.colour;
    ctx.fillText(label, lx, ly);
    drawn++;
  }
}

// A dark halo behind light text, a light one behind dark text.
const halos = new Map();
function haloOf(colour) {
  if (!halos.has(colour)) {
    const [r, g, b] = parseColour(colour);
    halos.set(colour, 0.2126 * r + 0.7152 * g + 0.0722 * b > 0.5 ? 'rgba(10, 15, 24, 0.85)' : 'rgba(255, 255, 255, 0.85)');
  }
  return halos.get(colour);
}

// The texts whose size on screen is at least `minTextPixels`, culled to the view, on the 2D canvas `labels`; smaller
// ones as labels of a fixed screen size where they fit (`drawLodLabels`).
function drawTexts(ctx, labels, texts, view, ratio, {minTextPixels, font, text}) {
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  ctx.clearRect(0, 0, labels.width, labels.height);
  if (text === false) return;
  const s = view.scale, w = labels.width / ratio, h = labels.height / ratio, rect = viewRect(view, w, h);
  ctx.setTransform(ratio, 0, 0, ratio, 0, 0);
  drawLodLabels(ctx, texts, view, w, h, rect, {minTextPixels, font});
  if (14 * s < minTextPixels) return;
  ctx.setTransform(s * ratio, 0, 0, s * ratio, (w / 2 - view.centreX * s) * ratio, (h / 2 - view.centreY * s) * ratio);
  for (const t of texts) {
    if (t.size * s < minTextPixels) continue;
    if (t.x < rect.x - 400 || t.x > rect.x + rect.width + 400 || t.y < rect.y - 40 || t.y > rect.y + rect.height + 40) continue;
    ctx.font = `${t.weight ?? 400} ${t.size}px ${font}`;
    ctx.fillStyle = t.colour;
    ctx.textAlign = t.align ?? 'start';
    ctx.textBaseline = t.baseline ?? 'alphabetic';
    if (t.angle) { ctx.save(); ctx.translate(t.x, t.y); ctx.rotate(t.angle); ctx.fillText(t.text, 0, 0); ctx.restore(); }
    else ctx.fillText(t.text, t.x, t.y);
  }
}

/**
 * The renderer around a backend. `backend` = {upload(buffers), draw(view, ratio), destroy()}; `kind` names it.
 */
export function rendererShell(canvas, scene, options, backend, kind) {
  const labels = options.labels ?? null;
  const ctx = labels?.getContext('2d') ?? null;
  const textOptions = {minTextPixels: options.minTextPixels ?? 6, font: options.fontFamily ?? 'system-ui, -apple-system, Segoe UI, sans-serif', text: options.text};
  // What the far layer draws besides the scene: the selection, pins, a trace and the style (`sceneBuffers`), and the
  // hover outline.
  let state = {selected: options.selected ?? null, selectedRoute: options.selectedRoute ?? null, pinned: options.pinned ?? null, trace: options.trace ?? null, style: options.style ?? null};
  let current = scene, built = null, frame = 0, hovered = null, morph = null;
  const ease = t => (t < 0.5 ? 4 * t * t * t : 1 - (-2 * t + 2) ** 3 / 2);
  const morphT = () => (morph ? ease(Math.min(1, (performance.now() - morph.start) / morph.duration)) : 1);
  const build = () => { built = sceneBuffers(current, state); if (hovered != null) built.layers[built.layers.length - 1] = hoverBuffers(built.scene, hovered, state.style); backend.upload(built); };
  build();
  const ratio = () => globalThis.devicePixelRatio || 1;
  const fit = () => {
    const b = built.bounds, w = canvas.clientWidth || canvas.width, h = canvas.clientHeight || canvas.height;
    const scale = Math.min(w / Math.max(b.width, 1), h / Math.max(b.height, 1)) * 0.95;
    return {centreX: b.x + b.width / 2, centreY: b.y + b.height / 2, scale};
  };
  let view = options.view ?? fit();
  function draw() {
    if (frame) { cancelAnimationFrame(frame); frame = 0; }
    const t = morphT();
    backend.draw(view, ratio(), {t});
    // While the drawing moves, the texts wait (they would stand at the end's places), as in the cluster overview.
    if (ctx) drawTexts(ctx, labels, built.texts, view, ratio(), morph ? {...textOptions, text: false} : textOptions);
    options.onView?.(view);
    if (morph) {
      if (t >= 1) { morph = null; backend.dropPrevious?.(); draw(); }
      else frame = requestAnimationFrame(() => { frame = 0; draw(); });
    }
  }
  const schedule = () => { if (!frame) frame = requestAnimationFrame(() => { frame = 0; draw(); }); };
  return {
    kind,
    get view() { return view; },
    get scene() { return built.scene; },
    get index() { return built.index; },
    get state() { return {...state}; },
    setView(next) { view = {...next}; schedule(); },
    fit() { view = fit(); schedule(); return view; },
    /**
     * A new scene; with `animate` (milliseconds, none under reduced motion) a morph from the drawing now: every box moves
     * from where it was (by id), lines and arrowheads fade over, texts wait until it ends.
     */
    setScene(next, {keepView = true, animate = 0} = {}) {
      const reduced = !!globalThis.matchMedia?.('(prefers-reduced-motion: reduce)').matches;
      if (animate > 0 && !reduced && backend.dropPrevious) {
        const before = built;
        current = next;
        built = sceneBuffers(current, state);
        if (hovered != null) built.layers[built.layers.length - 1] = hoverBuffers(built.scene, hovered, state.style);
        backend.upload(built, {starts: morphStarts(before, built), keep: true});
        morph = {start: performance.now(), duration: animate};
      } else { morph = null; current = next; build(); }
      if (!keepView) view = fit();
      schedule();
    },
    /** Whether a morph is running. */
    get animating() { return !!morph; },
    setState(next) { state = {...state, ...next}; build(); schedule(); },
    get hovered() { return hovered; },
    /** Outlines the node (leaf, container or bus bar) `id` as hovered, or none with null; rebuilds only that layer. */
    setHover(id) {
      if ((id ?? null) === hovered || morph) return;
      hovered = id ?? null;
      built.layers[built.layers.length - 1] = hoverBuffers(built.scene, hovered, state.style);
      backend.upload(built);
      schedule();
    },
    resize() {
      const r = ratio(), w = Math.max(1, Math.round((canvas.clientWidth || canvas.width) * r)), h = Math.max(1, Math.round((canvas.clientHeight || canvas.height) * r));
      for (const c of [canvas, labels]) if (c && (c.width !== w || c.height !== h)) { c.width = w; c.height = h; }
      draw();
    },
    draw,
    world(clientX, clientY) {
      const rect = canvas.getBoundingClientRect();
      return [view.centreX + (clientX - rect.left - rect.width / 2) / view.scale, view.centreY + (clientY - rect.top - rect.height / 2) / view.scale];
    },
    pick(clientX, clientY) {
      const [x, y] = this.world(clientX, clientY);
      return built.index.pick(x, y, 5 / view.scale);
    },
    destroy() { if (frame) cancelAnimationFrame(frame); backend.destroy(); },
  };
}

/**
 * Wheel zoom at the pointer and drag panning on `target` for anything with `view` and `setView` (a far renderer or the
 * scene view); a click without movement calls `onClick(clientX, clientY, event)`. Returns a function that removes them.
 */
export function bindSceneNavigation(target, viewer, {onClick, minScale = 0.005, maxScale = 8} = {}) {
  const wheel = e => {
    e.preventDefault();
    const rect = target.getBoundingClientRect(), v = viewer.view;
    const next = Math.max(minScale, Math.min(maxScale, v.scale * Math.exp(-e.deltaY * 0.002)));
    const px = e.clientX - rect.left - rect.width / 2, py = e.clientY - rect.top - rect.height / 2;
    const wx = v.centreX + px / v.scale, wy = v.centreY + py / v.scale;
    viewer.setView({centreX: wx - px / next, centreY: wy - py / next, scale: next});
  };
  let drag = null;
  const down = e => {
    if (e.defaultPrevented || (e.button !== 0 && e.button !== 1)) return;
    drag = {id: e.pointerId, x: e.clientX, y: e.clientY, view: viewer.view, moved: false};
  };
  const move = e => {
    if (!drag || e.pointerId !== drag.id) return;
    const dx = e.clientX - drag.x, dy = e.clientY - drag.y;
    if (!drag.moved && Math.hypot(dx, dy) < 4) return;
    if (!drag.moved) { drag.moved = true; target.setPointerCapture?.(e.pointerId); }
    viewer.setView({...drag.view, centreX: drag.view.centreX - dx / drag.view.scale, centreY: drag.view.centreY - dy / drag.view.scale});
  };
  const up = e => {
    if (drag && e.pointerId === drag.id && !drag.moved && e.type === 'pointerup') onClick?.(e.clientX, e.clientY, e);
    drag = null;
  };
  target.addEventListener('wheel', wheel, {passive: false});
  target.addEventListener('pointerdown', down);
  target.addEventListener('pointermove', move);
  for (const t of ['pointerup', 'pointercancel']) target.addEventListener(t, up);
  return () => {
    target.removeEventListener('wheel', wheel);
    target.removeEventListener('pointerdown', down);
    target.removeEventListener('pointermove', move);
    for (const t of ['pointerup', 'pointercancel']) target.removeEventListener(t, up);
  };
}
