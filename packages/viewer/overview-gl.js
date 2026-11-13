// WebGL renderer of the cluster overview (ADR 0018). No dependency: WebGL 1 with two programs, one for shapes (discs and
// boxes as quads, the outline found in the fragment shader) and one for lines and triangles. Texts go on an optional 2D
// canvas laid over it. Draws what `overviewBuffers` returns; computes no layout.
//
// Changes are animated, not simulated: the engine computes the final drawing, the renderer only interpolates. A new
// layout of the same graph (`morph`) moves every vertex from its old to its new place in the vertex shader (two sets of
// positions and a uniform `t`); a change of what is collapsed or structured (`setState` with `fade`) fades the old
// drawing out under the new one.
import {overviewBuffers, pickNode, pickExpanded, labelsOf, lensInverse, SHAPE_STRIDE, LINE_STRIDE} from './overview.js';

// The fisheye lens of `lensAt` (overview.js) in device pixels from the canvas centre: u_lens = (x, y, radius,
// magnification), radius 0 for none. Returns the moved point and the enlargement there.
const LENS = `
uniform vec4 u_lens;
vec3 lens(vec2 p) {
  if (u_lens.z <= 0.0) return vec3(p, 1.0);
  vec2 d = p - u_lens.xy;
  float r = length(d) / u_lens.z;
  if (r >= 1.0) return vec3(p, 1.0);
  float f = u_lens.w / ((u_lens.w - 1.0) * r + 1.0);
  return vec3(u_lens.xy + d * f, f);
}`;

const SHAPE_VS = `
attribute vec2 a_centre; attribute vec2 a_half; attribute vec2 a_centre2; attribute vec2 a_half2;
attribute vec2 a_corner; attribute vec4 a_colour; attribute float a_kind;
uniform vec2 u_centre; uniform float u_scale; uniform vec2 u_size; uniform float u_ratio; uniform float u_t;
varying vec2 v_local; varying vec4 v_colour; varying float v_kind; varying vec2 v_pixels;
// Breathing (idle): (phase in radians, amplitude, wavelength in device pixels); amplitude 0 for none. A disc grows and
// shrinks by the amplitude around its middle, in a wave that runs outwards from the canvas centre.
uniform vec3 u_breath;
${LENS}
void main() {
  vec2 centre = mix(a_centre, a_centre2, u_t), extent = mix(a_half, a_half2, u_t);
  // At least 1.5 pixels across, so that a leaf far out stays visible as a dot.
  vec2 pixels = max(extent * u_scale * u_ratio, vec2(0.75));
  vec2 middle = (centre - u_centre) * u_scale * u_ratio;
  // Only discs breathe; the boxes and lines of structured clusters stay with their texts.
  if (u_breath.y > 0.0 && a_kind < 1.5) pixels *= 1.0 + u_breath.y * sin(u_breath.x - 6.2831853 * length(middle) / u_breath.z);
  // Every corner goes through the lens (a disc larger than the lens keeps its outline); the edge width uses the
  // enlargement at the middle.
  vec2 screen = lens(middle + a_corner * pixels).xy;
  vec2 clip = screen / (u_size * 0.5);
  gl_Position = vec4(clip.x, -clip.y, 0.0, 1.0);
  v_local = a_corner; v_colour = a_colour; v_kind = a_kind; v_pixels = pixels * lens(middle).z;
}`;
const SHAPE_FS = `
precision mediump float;
varying vec2 v_local; varying vec4 v_colour; varying float v_kind; varying vec2 v_pixels;
uniform float u_fade;
// The distance from the middle, 1 on the outline: a circle, or a square, octagon or diamond inscribed in it. The
// outline is the tenths of the kind (0 to 3).
float outlineDistance(vec2 p, float outline) {
  vec2 a = abs(p);
  if (outline < 0.5) return length(p);
  if (outline < 1.5) return max(a.x, a.y) * 1.4142136;
  if (outline < 2.5) return max(max(a.x, a.y), (a.x + a.y) * 0.7071068) / 0.9238795;
  return a.x + a.y;
}
void main() {
  vec4 colour;
  if (v_kind < 1.5) {
    float base = floor(v_kind + 0.05);
    float d = outlineDistance(v_local, floor((v_kind - base) * 10.0 + 0.5));
    if (d > 1.0) discard;
    float edge = 1.0 / max(v_pixels.x, 1.0);
    if (base < 0.5) {
      colour = vec4(v_colour.rgb, v_colour.a * (1.0 - smoothstep(1.0 - edge, 1.0, d)));
    } else {
      // A ring about 1.2 pixels wide, and a faint fill so that a cluster reads as an area.
      float line = 1.0 - smoothstep(1.2 * edge, 2.4 * edge, 1.0 - d);
      colour = vec4(v_colour.rgb, max(line * 0.8, 0.045));
    }
  } else {
    // A box: the distance to its border in pixels, on the nearer axis.
    vec2 inside = (1.0 - abs(v_local)) * v_pixels;
    float border = min(inside.x, inside.y);
    float line = 1.0 - smoothstep(0.6, 1.4, border);
    if (v_kind < 2.5) {
      colour = vec4(mix(vec3(0.14, 0.21, 0.29), v_colour.rgb, line), 1.0);
    } else {
      colour = vec4(v_colour.rgb, max(line * 0.9, v_colour.a));
    }
  }
  gl_FragColor = vec4(colour.rgb, colour.a * u_fade);
}`;
const LINE_VS = `
attribute vec2 a_position; attribute vec2 a_position2; attribute vec4 a_colour;
uniform vec2 u_centre; uniform float u_scale; uniform vec2 u_size; uniform float u_ratio; uniform float u_t;
varying vec4 v_colour;
${LENS}
void main() {
  vec2 clip = lens((mix(a_position, a_position2, u_t) - u_centre) * u_scale * u_ratio).xy / (u_size * 0.5);
  gl_Position = vec4(clip.x, -clip.y, 0.0, 1.0);
  v_colour = a_colour;
}`;
const LINE_FS = `
precision mediump float;
varying vec4 v_colour; uniform float u_alpha; uniform float u_fade;
void main() { gl_FragColor = vec4(v_colour.rgb, v_colour.a * u_alpha * u_fade); }`;

function program(gl, vs, fs) {
  const compile = (type, source) => {
    const s = gl.createShader(type);
    gl.shaderSource(s, source);
    gl.compileShader(s);
    if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) throw Error(gl.getShaderInfoLog(s));
    return s;
  };
  const p = gl.createProgram();
  gl.attachShader(p, compile(gl.VERTEX_SHADER, vs));
  gl.attachShader(p, compile(gl.FRAGMENT_SHADER, fs));
  gl.linkProgram(p);
  if (!gl.getProgramParameter(p, gl.LINK_STATUS)) throw Error(gl.getProgramInfoLog(p));
  return p;
}

const PARTS = ['edges', 'discs', 'back', 'routes', 'arrows', 'front'];
const SHAPE_PARTS = new Set(['discs', 'back', 'front']);
/** Ease in and out: a move starts and ends gently. */
const ease = t => t < 0.5 ? 4 * t * t * t : 1 - (-2 * t + 2) ** 3 / 2;

/**
 * Draws `model` (from `overviewModel`) into `canvas`. Returns a renderer: `view` is `{centreX, centreY, scale}` with
 * `scale` in CSS pixels per unit; `setState({expanded, collapsed, selected, trace}, {fade})` draws clusters structured in
 * their discs (index → {scene}), collapsed into one disc (indices), marks one and a path (`pathOf`), blending the change in over `fade`
 * milliseconds; `morph(model, {duration, view})` moves the drawing to a new layout of the same graph (and the view to
 * `view`). `options.labels` is a 2D canvas laid over `canvas` for texts; `options.onView(view)` is called after every
 * view change; `options.edgeAlpha` (default 1) scales the opacity of the overview's edges; `options.background` is the
 * clear colour (RGBA in 0..1).
 */
export function createOverviewRenderer(canvas, initialModel, options = {}) {
  const gl = canvas.getContext('webgl', {antialias: true, premultipliedAlpha: false});
  if (!gl) throw Error('WebGL is not available');
  const shapes = program(gl, SHAPE_VS, SHAPE_FS), lines = program(gl, LINE_VS, LINE_FS);
  let model = initialModel, expanded = new Map(), collapsed = new Set(), selected = -1, trace = null;
  // The fisheye lens (`setLens`), in CSS pixels of the canvas, or null.
  let lens = null;
  // Breathing (`breathe`): {amplitude, period, wavelength, start, stop?}; it fades in over `fadeIn` and out over `fadeOut`
  // milliseconds after `stop`, and is dropped once faded out.
  let breath = null;
  const breathState = () => {
    if (!breath) return {phase: 0, amplitude: 0};
    const now = performance.now();
    const into = Math.min(1, (now - breath.start) / breath.fadeIn);
    const out = breath.stop ? Math.max(0, 1 - (now - breath.stop) / breath.fadeOut) : 1;
    return {phase: 2 * Math.PI * (now - breath.start) / breath.period, amplitude: breath.amplitude * ease(into) * ease(out)};
  };
  // A layer: per part a buffer of the start and one of the end of a morph (the same buffer when nothing moves).
  const layer = (from, to = from) => {
    const out = {buffers: {}, counts: from.count};
    for (const p of PARTS) {
      const a = gl.createBuffer();
      gl.bindBuffer(gl.ARRAY_BUFFER, a);
      gl.bufferData(gl.ARRAY_BUFFER, from[p], gl.STATIC_DRAW);
      let b = a;
      if (to !== from) {
        b = gl.createBuffer();
        gl.bindBuffer(gl.ARRAY_BUFFER, b);
        gl.bufferData(gl.ARRAY_BUFFER, to[p], gl.STATIC_DRAW);
      }
      out.buffers[p] = {a, b};
    }
    return out;
  };
  const drop = l => {
    if (!l) return;
    for (const p of PARTS) { gl.deleteBuffer(l.buffers[p].a); if (l.buffers[p].b !== l.buffers[p].a) gl.deleteBuffer(l.buffers[p].b); }
  };
  let outlines = options.shapes ?? {};
  const data = () => overviewBuffers(model, expanded, collapsed, selected, {shapes: outlines, trace});
  let current = layer(data()), previous = null;
  // The running animation: a morph (`t` of the current layer, and the view) and a fade (of the previous layer).
  let morphing = null, fading = null, frame = 0;

  const attribute = (p, name, buffer, stride, size, offset) => {
    const at = gl.getAttribLocation(p, name);
    if (at < 0) return;
    gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
    gl.enableVertexAttribArray(at);
    gl.vertexAttribPointer(at, size, gl.FLOAT, false, stride * 4, offset * 4);
  };
  const ratio = () => globalThis.devicePixelRatio || 1;
  const uniforms = (p, view, t, fade) => {
    gl.uniform2f(gl.getUniformLocation(p, 'u_centre'), view.centreX, view.centreY);
    gl.uniform1f(gl.getUniformLocation(p, 'u_scale'), view.scale);
    gl.uniform2f(gl.getUniformLocation(p, 'u_size'), canvas.width, canvas.height);
    gl.uniform1f(gl.getUniformLocation(p, 'u_ratio'), ratio());
    gl.uniform1f(gl.getUniformLocation(p, 'u_t'), t);
    gl.uniform1f(gl.getUniformLocation(p, 'u_fade'), fade);
    // The lens in device pixels from the canvas centre (y down, as the shaders compute before flipping).
    const r = ratio();
    gl.uniform4f(gl.getUniformLocation(p, 'u_lens'),
      lens ? (lens.x - canvas.clientWidth / 2) * r : 0, lens ? (lens.y - canvas.clientHeight / 2) * r : 0,
      lens ? lens.radius * r : 0, lens ? lens.magnification : 1);
    const b = breathState();
    gl.uniform3f(gl.getUniformLocation(p, 'u_breath'), b.phase, b.amplitude, (breath?.wavelength ?? 600) * r);
  };
  const drawPart = (l, part, mode, view, t, fade) => {
    if (!l.counts[part]) return;
    const {a, b} = l.buffers[part];
    // Only the arrays of the program in use are on: one left on by the other program would point at a buffer of the
    // wrong size.
    for (let i = 0; i < 8; i++) gl.disableVertexAttribArray(i);
    if (SHAPE_PARTS.has(part)) {
      gl.useProgram(shapes);
      uniforms(shapes, view, t, fade);
      attribute(shapes, 'a_centre', a, SHAPE_STRIDE, 2, 0);
      attribute(shapes, 'a_half', a, SHAPE_STRIDE, 2, 2);
      attribute(shapes, 'a_corner', a, SHAPE_STRIDE, 2, 4);
      attribute(shapes, 'a_colour', a, SHAPE_STRIDE, 4, 6);
      attribute(shapes, 'a_kind', a, SHAPE_STRIDE, 1, 10);
      attribute(shapes, 'a_centre2', b, SHAPE_STRIDE, 2, 0);
      attribute(shapes, 'a_half2', b, SHAPE_STRIDE, 2, 2);
    } else {
      gl.useProgram(lines);
      uniforms(lines, view, t, fade);
      gl.uniform1f(gl.getUniformLocation(lines, 'u_alpha'), part === 'edges' ? options.edgeAlpha ?? 1 : 1);
      attribute(lines, 'a_position', a, LINE_STRIDE, 2, 0);
      attribute(lines, 'a_colour', a, LINE_STRIDE, 4, 2);
      attribute(lines, 'a_position2', b, LINE_STRIDE, 2, 0);
    }
    gl.drawArrays(mode, 0, l.counts[part]);
  };
  const drawLayer = (l, view, t, fade) => {
    drawPart(l, 'edges', gl.LINES, view, t, fade);
    drawPart(l, 'discs', gl.TRIANGLES, view, t, fade);
    drawPart(l, 'back', gl.TRIANGLES, view, t, fade);
    drawPart(l, 'routes', gl.LINES, view, t, fade);
    drawPart(l, 'arrows', gl.TRIANGLES, view, t, fade);
    drawPart(l, 'front', gl.TRIANGLES, view, t, fade);
  };
  const writeLabels = (view, show) => {
    const c = options.labels;
    if (!c) return;
    const w = Math.round(c.clientWidth * ratio()), h = Math.round(c.clientHeight * ratio());
    if (c.width !== w || c.height !== h) { c.width = w; c.height = h; }
    const ctx = c.getContext('2d');
    ctx.setTransform(ratio(), 0, 0, ratio(), 0, 0);
    ctx.clearRect(0, 0, c.clientWidth, c.clientHeight);
    // The lens's rim, faint, so that one sees where the enlargement ends.
    if (lens) {
      ctx.beginPath();
      ctx.arc(lens.x, lens.y, lens.radius, 0, 2 * Math.PI);
      ctx.strokeStyle = 'rgba(223,231,239,0.32)';
      ctx.lineWidth = 1;
      ctx.stroke();
    }
    // While the drawing moves, the texts wait: they would sit at the places of the end.
    if (!show) return;
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    for (const label of labelsOf(model, expanded, view, c.clientWidth, c.clientHeight, {collapsed, lens})) {
      ctx.font = `${label.kind === 'cluster' ? 600 : 400} ${label.size.toFixed(1)}px system-ui, sans-serif`;
      ctx.fillStyle = label.kind === 'cluster' ? 'rgba(223,231,239,0.85)' : 'rgba(223,231,239,0.95)';
      ctx.fillText(label.text, label.x, label.y);
    }
  };
  const progress = a => a ? Math.min(1, (performance.now() - a.start) / a.duration) : 1;
  const tick = () => {
    frame = 0;
    if (morphing) {
      const p = ease(progress(morphing));
      if (morphing.view) {
        const [v0, v1] = morphing.view;
        // The scale moves in proportion, so that a zoom feels even.
        const scale = v0.scale * (v1.scale / v0.scale) ** p;
        renderer.view = {scale, centreX: v0.centreX + (v1.centreX - v0.centreX) * p, centreY: v0.centreY + (v1.centreY - v0.centreY) * p};
      }
      if (progress(morphing) >= 1) finishMorph();
    }
    if (fading && progress(fading) >= 1) { drop(previous); previous = null; fading = null; }
    if (breath?.stop && performance.now() - breath.stop >= breath.fadeOut) breath = null;
    // Breathing alone leaves the texts as they are: they do not move, and placing them every frame is work.
    renderer.draw({labels: !!(morphing || fading) || !breath});
    if (morphing || fading || breath) frame = requestAnimationFrame(tick);
  };
  const animate = () => { if (!frame) frame = requestAnimationFrame(tick); };
  const finishMorph = () => {
    if (!morphing) return;
    const done = morphing;
    morphing = null;
    drop(current);
    current = layer(done.to);
    if (done.view) renderer.view = {...done.view[1]};
    options.onView?.(renderer.view);
  };

  const renderer = {
    view: {centreX: 0, centreY: 0, scale: 1},
    get model() { return model; },
    get expanded() { return expanded; },
    get collapsed() { return collapsed; },
    get animating() { return !!(morphing || fading); },
    setView(view) { renderer.view = {...view}; renderer.draw(); options.onView?.(renderer.view); },
    /** The fisheye lens (see `lensAt`), `{x, y, radius, magnification}` in CSS pixels of the canvas, or null. */
    get lens() { return lens; },
    setLens(next) {
      const valid = next && next.radius > 0 && next.magnification > 1;
      if (!valid && !lens) return;
      lens = valid ? {x: next.x, y: next.y, radius: next.radius, magnification: next.magnification} : null;
      // One drawing per frame, however often the pointer moves.
      if (!frame) frame = requestAnimationFrame(() => { frame = 0; renderer.draw(); if (morphing || fading || breath) animate(); });
    },
    setExpanded(map) { renderer.setState({expanded: map}); },
    /** The outline per node kind (`circle`, `square`, `octagon`, `diamond`); kinds not named are circles. */
    setShapes(next) { outlines = {...next}; renderer.setState({}); },
    /**
     * Draws clusters structured (`expanded`, index → {scene}), collapsed into one disc (`collapsed`, indices) and marks
     * `selected` (-1: none); omitted parts are kept. With `fade` (milliseconds) the old drawing blends into the new one.
     */
    setState({expanded: e = expanded, collapsed: c = collapsed, selected: s = selected, trace: p = trace} = {}, {fade = 0} = {}) {
      finishMorph();
      expanded = new Map(e); collapsed = new Set(c); selected = s; trace = p;
      if (fade > 0) {
        // A blend still running keeps fading what it had: the older drawing is dropped only when the new one is whole.
        drop(previous);
        previous = current;
        fading = {start: performance.now(), duration: fade};
      } else drop(current);
      current = layer(data());
      if (fade > 0) animate(); else renderer.draw();
    },
    /**
     * Moves the drawing to `next`, a new overview model of the same graph (for instance after a continuation), over
     * `duration` milliseconds, and the view to `view` if given. The state (structured, collapsed, selected) stays; when
     * the two drawings do not have the same vertices, the new one is shown at once.
     */
    morph(next, {duration = 450, view = null} = {}) {
      finishMorph();
      const from = data();
      const old = model;
      model = next;
      const to = data();
      const same = PARTS.every(p => from[p].length === to[p].length);
      if (!same || duration <= 0) {
        drop(current);
        current = layer(to);
        if (view) renderer.view = {...view};
        renderer.draw();
        options.onView?.(renderer.view);
        return false;
      }
      drop(current);
      current = layer(from, to);
      morphing = {start: performance.now(), duration, to, view: view ? [{...renderer.view}, {...view}] : null, old};
      animate();
      return true;
    },
    resize() {
      const w = Math.round(canvas.clientWidth * ratio()), h = Math.round(canvas.clientHeight * ratio());
      if (canvas.width !== w || canvas.height !== h) { canvas.width = w; canvas.height = h; }
      renderer.draw();
    },
    /** Whether the drawing breathes (see `breathe`), fading out included. */
    get breathing() { return !!breath; },
    /**
     * Lets the discs breathe: each grows and shrinks by `amplitude` (a fraction of its radius) over `period`
     * milliseconds, in a wave of `wavelength` CSS pixels that runs outwards from the centre of the canvas. Fades in over
     * `fadeIn` milliseconds. `breathe(null)` fades it out (over `fadeOut`, default 400) and stops. Only the drawing changes.
     */
    breathe(settings, {fadeOut = 400} = {}) {
      if (!settings) {
        if (!breath) return;
        // At once (fadeOut 0): gone with the next drawing.
        if (fadeOut <= 0) { breath = null; return; }
        if (!breath.stop) { breath.stop = performance.now(); breath.fadeOut = fadeOut; animate(); }
        return;
      }
      const {amplitude = 0.06, period = 4000, wavelength = 600, fadeIn = 1500} = settings;
      if (breath && !breath.stop) return;
      breath = {amplitude, period: Math.max(period, 100), wavelength: Math.max(wavelength, 1), fadeIn: Math.max(fadeIn, 1), fadeOut, start: performance.now()};
      animate();
    },
    draw({labels = true} = {}) {
      const view = renderer.view;
      gl.viewport(0, 0, canvas.width, canvas.height);
      gl.clearColor(...(options.background ?? [0.047, 0.063, 0.094, 1]));
      gl.clear(gl.COLOR_BUFFER_BIT);
      gl.enable(gl.BLEND);
      gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);
      const t = morphing ? ease(progress(morphing)) : 0;
      const f = fading ? ease(progress(fading)) : 1;
      // The old drawing fades out under the new one, which is drawn whole: what both have does not dim (a blend of two
      // half-transparent drawings would let the background through and flicker), what goes fades, what comes is there.
      if (previous) drawLayer(previous, view, 0, 1 - f);
      drawLayer(current, view, t, 1);
      if (labels) writeLabels(view, !morphing);
    },
    /** The world point under a CSS pixel of the canvas. */
    world(clientX, clientY) {
      const rect = canvas.getBoundingClientRect(), v = renderer.view;
      // Under the lens, the point shown there.
      const [x, y] = lensInverse(lens, clientX - rect.left, clientY - rect.top);
      return [v.centreX + (x - rect.width / 2) / v.scale, v.centreY + (y - rect.height / 2) / v.scale];
    },
    /** The deepest overview node under a CSS pixel, or -1 (inside a structured cluster: that cluster). */
    pick(clientX, clientY) {
      const [x, y] = renderer.world(clientX, clientY);
      return pickExpanded(model, expanded, x, y)?.cluster ?? pickNode(model, x, y, collapsed);
    },
    /** The scene node under a CSS pixel inside a structured cluster (see `pickExpanded`), or null. */
    pickScene(clientX, clientY) { return pickExpanded(model, expanded, ...renderer.world(clientX, clientY)); },
    destroy() {
      if (frame) cancelAnimationFrame(frame);
      drop(current); drop(previous);
      gl.deleteProgram(shapes); gl.deleteProgram(lines);
      options.labels?.getContext('2d').clearRect(0, 0, options.labels.width, options.labels.height);
    },
  };
  return renderer;
}

/**
 * Wheel zoom at the pointer and drag panning for a renderer. `onClick(index, event)` gets the node under a click that was
 * not a drag; `onHover(index, event)` the node under the pointer; `onDoubleClick(index, event)` a double-click. With
 * `lens` (a function that returns `{radius, magnification}` or null, asked on every move), a fisheye lens follows the
 * pointer and is gone when the pointer leaves the canvas. With `idle` (`{after, breathe}`), the discs start to breathe
 * (`renderer.breathe(breathe)`) after `after` milliseconds (default 8000) without input, and stop at the next input; not
 * when the system asks for reduced motion. Returns a function that removes the listeners.
 */
export function bindOverviewNavigation(canvas, renderer, {onClick, onHover, onDoubleClick, lens, idle} = {}) {
  let drag = null;
  // Breathing after `idle.after` milliseconds without input; any input stops it and starts the wait again. Not when
  // the user asked the system for less motion.
  const still = globalThis.matchMedia?.('(prefers-reduced-motion: reduce)')?.matches;
  let idleTimer = 0;
  const awake = () => {
    if (!idle || still) return;
    renderer.breathe(null);
    clearTimeout(idleTimer);
    idleTimer = setTimeout(() => { if (!drag) renderer.breathe(idle.breathe ?? {}); }, idle.after ?? 8000);
  };
  awake();
  const input = () => awake();
  const follow = e => {
    if (!lens) return;
    const settings = lens(), rect = canvas.getBoundingClientRect();
    renderer.setLens(settings ? {...settings, x: e.clientX - rect.left, y: e.clientY - rect.top} : null);
  };
  const leave = () => { if (lens) renderer.setLens(null); };
  const wheel = e => {
    e.preventDefault();
    awake();
    const [wx, wy] = renderer.world(e.clientX, e.clientY);
    const v = renderer.view, factor = Math.exp(-e.deltaY * 0.0015);
    const scale = Math.min(Math.max(v.scale * factor, 1e-4), 1e4);
    // Keep the world point under the pointer where it is.
    renderer.setView({scale, centreX: wx - (wx - v.centreX) * v.scale / scale, centreY: wy - (wy - v.centreY) * v.scale / scale});
  };
  const down = e => { awake(); drag = {x: e.clientX, y: e.clientY, view: renderer.view, moved: false}; canvas.setPointerCapture?.(e.pointerId); };
  const move = e => {
    awake();
    follow(e);
    if (!drag) { onHover?.(renderer.pick(e.clientX, e.clientY), e); return; }
    const dx = e.clientX - drag.x, dy = e.clientY - drag.y;
    if (!drag.moved && Math.hypot(dx, dy) < 4) return;
    drag.moved = true;
    renderer.setView({...drag.view, centreX: drag.view.centreX - dx / drag.view.scale, centreY: drag.view.centreY - dy / drag.view.scale});
  };
  const up = e => {
    if (drag && !drag.moved) onClick?.(renderer.pick(e.clientX, e.clientY), e);
    drag = null;
  };
  const cancel = () => { drag = null; };
  const double = e => onDoubleClick?.(renderer.pick(e.clientX, e.clientY), e);
  canvas.addEventListener('dblclick', double);
  canvas.addEventListener('wheel', wheel, {passive: false});
  canvas.addEventListener('pointerdown', down);
  canvas.addEventListener('pointermove', move);
  canvas.addEventListener('pointerup', up);
  canvas.addEventListener('pointercancel', cancel);
  canvas.addEventListener('pointerleave', leave);
  globalThis.addEventListener?.('keydown', input);
  return () => {
    clearTimeout(idleTimer);
    renderer.breathe(null, {fadeOut: 0});
    globalThis.removeEventListener?.('keydown', input);
    canvas.removeEventListener('pointerleave', leave);
    canvas.removeEventListener('wheel', wheel);
    canvas.removeEventListener('pointerdown', down);
    canvas.removeEventListener('pointermove', move);
    canvas.removeEventListener('pointerup', up);
    canvas.removeEventListener('pointercancel', cancel);
    canvas.removeEventListener('dblclick', double);
  };
}
