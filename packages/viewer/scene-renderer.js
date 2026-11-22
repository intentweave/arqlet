// Provisional (ADR 0025): what both GPU backends of the far layer share. A backend (`scene-gpu.js` with WebGPU, the
// first choice; `scene-gl.js` with WebGL 2, the fallback) only uploads the buffers of `sceneBuffers` and draws them for a
// view; the shell here holds the view, the state, the texts on the 2D canvas, picking and the frame scheduling.
import {sceneBuffers, hoverBuffers, viewRect} from './scene-buffers.js';

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

// The texts whose size on screen is at least `minTextPixels`, culled to the view, on the 2D canvas `labels`.
function drawTexts(ctx, labels, texts, view, ratio, {minTextPixels, font, text}) {
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  ctx.clearRect(0, 0, labels.width, labels.height);
  if (text === false) return;
  const s = view.scale, w = labels.width / ratio, h = labels.height / ratio, rect = viewRect(view, w, h);
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
  // What the far layer draws besides the scene: the selection, pins and a trace (`sceneBuffers`), and the hover outline.
  let state = {selected: options.selected ?? null, selectedRoute: options.selectedRoute ?? null, pinned: options.pinned ?? null, trace: options.trace ?? null};
  let current = scene, built = null, frame = 0, hovered = null;
  const build = () => { built = sceneBuffers(current, state); if (hovered != null) built.layers[built.layers.length - 1] = hoverBuffers(built.scene, hovered); backend.upload(built); };
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
    backend.draw(view, ratio());
    if (ctx) drawTexts(ctx, labels, built.texts, view, ratio(), textOptions);
    options.onView?.(view);
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
    setScene(next, {keepView = true} = {}) { current = next; build(); if (!keepView) view = fit(); schedule(); },
    setState(next) { state = {...state, ...next}; build(); schedule(); },
    get hovered() { return hovered; },
    /** Outlines the node (leaf, container or bus bar) `id` as hovered, or none with null; rebuilds only that layer. */
    setHover(id) {
      if ((id ?? null) === hovered) return;
      hovered = id ?? null;
      built.layers[built.layers.length - 1] = hoverBuffers(built.scene, hovered);
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
