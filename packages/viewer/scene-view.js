// Provisional (ADR 0025, step V2): a scene drawn by two layers over one geometry. From afar the WebGL renderer
// (`scene-gl.js`) draws everything; when few elements are in view and they are large enough on screen (`nearPolicy`),
// an SVG layer (`renderSvg`) above it draws exactly the elements in view plus a margin, with labels, ports, controls,
// selection and the host's handlers. Both layers draw the scene's coordinates, so the switch moves nothing.
//
// The near layer holds the excerpt of a rectangle around the view (`margin` of the view's size on every side): panning
// only moves it until the view leaves that rectangle, and a change of the scale or of the scene draws it again. The
// switch fades over `fade` milliseconds (none under reduced motion); once the near layer is opaque the canvas is hidden.
// The far layer's backend is WebGPU when `options.backend` carries a device (`openSceneView` asks for one), else WebGL 2;
// without either the SVG layer draws alone, on every view.
import {renderSvg} from './svg.js';
import {createSceneRenderer, webgl2Available} from './scene-gl.js';
import {createSceneGpuRenderer} from './scene-gpu.js';
import {sceneBackend} from './scene-renderer.js';
import {sceneOf, sceneIndex, sceneExcerpt, nearPolicy, medianBoxPixels, viewRect} from './scene-buffers.js';

const SVG_NS = 'http://www.w3.org/2000/svg';

export function createSceneView(host, input, options = {}) {
  const doc = host.ownerDocument;
  const {budget = 400, minPixels = 24, hysteresis = 1.25, margin = 0.5, fade = 150} = options;
  const reduced = () => !!globalThis.matchMedia?.('(prefers-reduced-motion: reduce)').matches;
  if (getComputedStyle(host).position === 'static') host.style.position = 'relative';
  host.style.overflow = 'hidden';
  const layerStyle = 'position:absolute;left:0;top:0;width:100%;height:100%;';
  const backend = options.backend ?? {kind: options.webgl !== false && webgl2Available(doc) ? 'webgl2' : 'svg'};
  const gl = backend.kind !== 'svg';
  let canvas = null, labels = null, far = null;
  let scene = sceneOf(input);
  if (gl) {
    canvas = doc.createElement('canvas'); canvas.style.cssText = layerStyle; canvas.className = 'lw-far';
    labels = doc.createElement('canvas'); labels.style.cssText = layerStyle + 'pointer-events:none;'; labels.className = 'lw-far-text';
    host.append(canvas, labels);
  }
  const near = doc.createElementNS(SVG_NS, 'svg');
  near.setAttribute('class', 'lw-near');
  near.style.cssText = 'position:absolute;left:0;top:0;overflow:visible;opacity:0;visibility:hidden;';
  host.append(near);

  let index = gl ? null : sceneIndex(scene);
  let leafMedian = 0;
  const measureLeaves = () => {
    const sizes = scene.nodes.filter(n => !n.container).map(n => Math.min(n.bounds.width, n.bounds.height)).sort((a, b) => a - b);
    leafMedian = sizes.length ? sizes[Math.floor(sizes.length / 2)] : 0;
  };
  measureLeaves();
  const width = () => host.clientWidth || 1, height = () => host.clientHeight || 1;
  const fitView = () => {
    const b = scene.bounds, scale = Math.min(width() / Math.max(b.width, 1), height() / Math.max(b.height, 1)) * 0.95;
    return {centreX: b.x + b.width / 2, centreY: b.y + b.height / 2, scale};
  };
  let view = options.view ?? fitView();
  let state = {selected: options.svg?.selected ?? null, selectedRoute: options.svg?.selectedRoute ?? null};
  if (gl) {
    const farOptions = {labels, view, background: options.background, preserveDrawingBuffer: options.preserveDrawingBuffer, ...state};
    far = backend.kind === 'webgpu' ? createSceneGpuRenderer(canvas, scene, {...farOptions, device: backend.device}) : createSceneRenderer(canvas, scene, farOptions);
    index = far.index;
  }
  let layer = gl ? 'far' : 'near', held = null, heldScale = 0, timer = 0;
  const counts = {elements: 0, boxPixels: 0, held: 0};

  // The near layer: the excerpt of `held`, drawn by renderSvg at the view's scale and placed where `held` is on screen.
  function drawNear() {
    const rect = viewRect(view, width(), height());
    const grow = gl ? margin : 0;
    held = gl ? {x: rect.x - rect.width * grow, y: rect.y - rect.height * grow, width: rect.width * (1 + 2 * grow), height: rect.height * (1 + 2 * grow)} : {...scene.bounds};
    heldScale = view.scale;
    const excerpt = gl ? sceneExcerpt(scene, index, held) : {...scene};
    counts.held = excerpt.nodes.length + excerpt.routes.length;
    renderSvg(near, excerpt, {...options.svg, ...state, scale: view.scale, document: doc});
    place();
  }
  function place() {
    const rect = viewRect(view, width(), height());
    near.style.transform = `translate(${(held.x - rect.x) * view.scale}px, ${(held.y - rect.y) * view.scale}px)`;
  }
  const covers = rect => held && rect.x >= held.x && rect.y >= held.y && rect.x + rect.width <= held.x + held.width && rect.y + rect.height <= held.y + held.height;

  function setLayer(next) {
    if (next === layer) return;
    layer = next;
    clearTimeout(timer);
    const ms = reduced() ? 0 : fade;
    near.style.transition = ms ? `opacity ${ms}ms linear` : '';
    if (next === 'near') {
      drawNear();
      near.style.visibility = 'visible';
      near.style.opacity = '1';
      const hide = () => { if (layer === 'near') { canvas.style.visibility = 'hidden'; labels.style.visibility = 'hidden'; } };
      if (ms) timer = setTimeout(hide, ms); else hide();
    } else {
      canvas.style.visibility = 'visible'; labels.style.visibility = 'visible';
      far.setView(view); far.draw();
      near.style.opacity = '0';
      const clear = () => { if (layer === 'far') { near.style.visibility = 'hidden'; near.replaceChildren(); held = null; } };
      if (ms) timer = setTimeout(clear, ms); else clear();
    }
    options.onLayer?.(layer);
  }

  function decide() {
    if (!gl) return 'near';
    // From afar no query is needed: the typical leaf is too small on screen anyway.
    if (leafMedian * view.scale < minPixels / hysteresis) { counts.elements = -1; counts.boxPixels = leafMedian * view.scale; return 'far'; }
    const inView = index.query(viewRect(view, width(), height()));
    counts.elements = inView.nodes.size + inView.routes.size;
    counts.boxPixels = medianBoxPixels(index, inView.nodes, view.scale);
    return nearPolicy({elements: counts.elements, boxPixels: counts.boxPixels, current: layer, budget, minPixels, hysteresis});
  }

  function update() {
    const next = decide();
    if (gl && (next === 'far' || layer === 'far')) far.setView(view);
    if (next !== layer) setLayer(next);
    else if (layer === 'near') {
      if (view.scale !== heldScale || !covers(viewRect(view, width(), height()))) drawNear();
      else place();
    }
    options.onView?.(view, layer);
  }

  const api = {
    get view() { return {...view}; },
    get layer() { return layer; },
    /** The far layer's backend: 'webgpu', 'webgl2' or 'svg' (none). */
    get backend() { return backend.kind; },
    get counts() { return {...counts}; },
    get scene() { return scene; },
    get index() { return index; },
    get far() { return far; },
    get near() { return near; },
    setView(next) { view = {...next}; update(); },
    fit() { view = fitView(); update(); return api.view; },
    setScene(next, {keepView = true} = {}) {
      scene = sceneOf(next);
      measureLeaves();
      if (far) { far.setScene(scene); index = far.index; } else index = sceneIndex(scene);
      if (!keepView) view = fitView();
      held = null;
      if (layer === 'near') drawNear();
      update();
    },
    setState(next) {
      state = {...state, ...next};
      far?.setState(state);
      if (layer === 'near') drawNear();
    },
    resize() { far?.resize(); if (layer === 'near') drawNear(); update(); },
    /** The element under a CSS pixel from the far layer's index (the near layer answers through its own DOM events). */
    pick(clientX, clientY) {
      const rect = host.getBoundingClientRect();
      const x = view.centreX + (clientX - rect.left - rect.width / 2) / view.scale, y = view.centreY + (clientY - rect.top - rect.height / 2) / view.scale;
      return index.pick(x, y, 5 / view.scale);
    },
    destroy() { clearTimeout(timer); far?.destroy(); canvas?.remove(); labels?.remove(); near.remove(); },
  };
  far?.resize();
  if (!gl) drawNear();
  update();
  if (!gl) { near.style.visibility = 'visible'; near.style.opacity = '1'; }
  return api;
}

/**
 * The scene view with the best backend this browser offers: WebGPU, else WebGL 2, else SVG alone. `options.prefer`
 * ('webgl2' or 'svg') skips the better ones.
 */
export async function openSceneView(host, scene, options = {}) {
  const backend = options.backend ?? await sceneBackend({prefer: options.prefer, doc: host.ownerDocument});
  return createSceneView(host, scene, {...options, backend});
}
