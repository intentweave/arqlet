// GPU renderer of the cluster overview (ADR 0018). No dependency. Two programs, one for shapes (discs and boxes as
// quads, the outline found in the fragment shader) and one for lines and triangles, on one of three backends (ADR 0025,
// O2): WebGPU when `options.device` is given (WGSL below), else WebGL 2, else WebGL 1 (the same GLSL ES 1.00 programs,
// which WebGL 2 runs as they are). Texts go on an optional 2D
// canvas laid over it. Draws what `overviewBuffers` returns; computes no layout.
//
// Changes are animated, not simulated: the engine computes the final drawing, the renderer only interpolates. A new
// layout of the same graph (`morph`) moves every vertex from its old to its new place in the vertex shader (two sets of
// positions and a uniform `t`); a change of what is collapsed or structured (`setState` with `fade`) fades the old
// drawing out under the new one.
import {overviewBuffers, pickNode, pickExpanded, labelsOf, lensInverse, ownersOf, orbitPoint, SHAPE_STRIDE, LINE_STRIDE} from './overview.js';

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

// Cosmic motion (galaxy style, idle): u_cosmic = (seconds, strength 0..1, extent of the drawing in world units). Every
// world point turns a little around the drawing's centre, by an angle that oscillates slowly and differs with the
// distance from the centre, so the galaxies seem to rotate and shear against each other; bounded, it never drifts away.
// The same field moves stars and line ends, so lines stay on their stars.
const COSMIC = `
uniform vec3 u_cosmic;
vec2 cosmic(vec2 p) {
  if (u_cosmic.y <= 0.0) return p;
  float d = length(p) / max(u_cosmic.z, 1.0);
  float a = u_cosmic.y * (0.045 * sin(u_cosmic.x * 0.21 + d * 5.0) + 0.025 * sin(u_cosmic.x * 0.13 - d * 9.0 + 1.3));
  float c = cos(a), s = sin(a);
  return vec2(c * p.x - s * p.y, s * p.x + c * p.y);
}`;

// Orbit (pseudo-3D): u_orbit = (yaw, pitch, depth in world units, on), u_extent the drawing's radius. A world point gets a
// depth from a smooth field at the scale of the clusters plus a little of its own (as `orbitDepth` in overview.js), is
// turned around the view's centre and seen in perspective from three extents away: (x, y) relative to the centre in
// world units, and the enlargement there. Off: the plain offset, enlargement 1.
const ORBIT = `
uniform vec4 u_orbit; uniform float u_extent;
float orbitDepth(vec2 w) {
  vec2 q = w / max(u_extent, 1.0) * 3.0;
  float field = sin(q.x * 1.7 + 0.3) * cos(q.y * 1.3 - 0.7) * 0.7 + sin((q.x + q.y) * 2.3) * 0.3;
  return field + (fract(sin(w.x * 12.9898 + w.y * 78.233) * 43758.5453) - 0.5) * 0.3;
}
vec3 orbit(vec2 w) {
  vec2 rel = w - u_centre;
  if (u_orbit.w < 0.5) return vec3(rel, 1.0);
  vec3 p = vec3(rel, orbitDepth(w) * u_orbit.z);
  float cy = cos(u_orbit.x), sy = sin(u_orbit.x);
  p = vec3(cy * p.x + sy * p.z, p.y, -sy * p.x + cy * p.z);
  float cp = cos(u_orbit.y), sp = sin(u_orbit.y);
  p = vec3(p.x, cp * p.y - sp * p.z, sp * p.y + cp * p.z);
  float dist = max(u_extent, 1.0) * 3.0;
  float f = dist / max(dist + p.z, dist * 0.2);
  return vec3(p.xy * f, f);
}`;

const SHAPE_VS = `
attribute vec2 a_centre; attribute vec2 a_half; attribute vec2 a_centre2; attribute vec2 a_half2;
attribute vec2 a_corner; attribute vec4 a_colour; attribute float a_kind;
uniform vec2 u_centre; uniform float u_scale; uniform vec2 u_size; uniform float u_ratio; uniform float u_t;
varying vec2 v_local; varying vec4 v_colour; varying float v_kind; varying vec2 v_pixels; varying float v_seed;
// Breathing (idle): (phase in radians, amplitude, wavelength in device pixels); amplitude 0 for none. A disc grows and
// shrinks by the amplitude around its middle, in a wave that runs outwards from the canvas centre.
uniform vec3 u_breath;
${LENS}
${COSMIC}
${ORBIT}
void main() {
  vec2 centre = cosmic(mix(a_centre, a_centre2, u_t)), extent = mix(a_half, a_half2, u_t);
  vec3 seen = orbit(centre);
  // A phase of its own per star, for the twinkle.
  v_seed = fract(sin(dot(mix(a_centre, a_centre2, u_t), vec2(12.9898, 78.233))) * 43758.5453);
  // At least 1.5 pixels across, so that a leaf far out stays visible as a dot.
  vec2 pixels = max(extent * u_scale * u_ratio * seen.z, vec2(0.75));
  vec2 middle = seen.xy * u_scale * u_ratio;
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
varying vec2 v_local; varying vec4 v_colour; varying float v_kind; varying vec2 v_pixels; varying float v_seed;
uniform float u_fade;
// The twinkle of the cosmic motion: (seconds, strength 0..1); its own uniform, as the precisions of the shaders differ.
uniform vec2 u_twinkle;
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
  if (v_kind > 3.5) {
    // Galaxy mode: a glow (4), a soft Gaussian spot; a star (5), a bright core in its own glow.
    float d = length(v_local);
    if (d > 1.0) discard;
    float glow = exp(-d * d * 4.5) * (1.0 - d);
    float a = v_kind < 4.5 ? glow : max(1.0 - smoothstep(0.22, 0.34, d), glow * 0.55);
    // Stars twinkle while the drawing is in cosmic motion, each on its own phase.
    if (v_kind > 4.5 && u_twinkle.y > 0.0) a *= 1.0 - 0.4 * u_twinkle.y * (0.5 + 0.5 * sin(u_twinkle.x * 2.3 + v_seed * 6.2831853));
    colour = vec4(v_colour.rgb, v_colour.a * a);
  } else if (v_kind < 1.5) {
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
${COSMIC}
${ORBIT}
void main() {
  vec2 clip = lens(orbit(cosmic(mix(a_position, a_position2, u_t))).xy * u_scale * u_ratio).xy / (u_size * 0.5);
  gl_Position = vec4(clip.x, -clip.y, 0.0, 1.0);
  v_colour = a_colour;
}`;
const LINE_FS = `
precision mediump float;
varying vec4 v_colour; uniform float u_alpha; uniform float u_fade;
void main() { gl_FragColor = vec4(v_colour.rgb, v_colour.a * u_alpha * u_fade); }`;

// The same programs in WGSL for WebGPU. One uniform block per draw (a dynamic offset): the view, the morph's t, the fade,
// the lens, breathing, the cosmic motion, the twinkle, the orbit and the line opacity.
const WGSL_COMMON = `
struct U {
  centre: vec2f, size: vec2f,
  scale: f32, ratio: f32, t: f32, fade: f32,
  lens: vec4f, breath: vec4f, cosmic: vec4f, twinkle: vec4f, orbit: vec4f,
  extent: f32, alpha: f32, unused0: f32, unused1: f32,
};
@group(0) @binding(0) var<uniform> u: U;
fn lens(p: vec2f) -> vec3f {
  if (u.lens.z <= 0.0) { return vec3f(p, 1.0); }
  let d = p - u.lens.xy;
  let r = length(d) / u.lens.z;
  if (r >= 1.0) { return vec3f(p, 1.0); }
  let f = u.lens.w / ((u.lens.w - 1.0) * r + 1.0);
  return vec3f(u.lens.xy + d * f, f);
}
fn cosmic(p: vec2f) -> vec2f {
  if (u.cosmic.y <= 0.0) { return p; }
  let d = length(p) / max(u.cosmic.z, 1.0);
  let a = u.cosmic.y * (0.045 * sin(u.cosmic.x * 0.21 + d * 5.0) + 0.025 * sin(u.cosmic.x * 0.13 - d * 9.0 + 1.3));
  let c = cos(a);
  let s = sin(a);
  return vec2f(c * p.x - s * p.y, s * p.x + c * p.y);
}
fn orbitDepth(w: vec2f) -> f32 {
  let q = w / max(u.extent, 1.0) * 3.0;
  let field = sin(q.x * 1.7 + 0.3) * cos(q.y * 1.3 - 0.7) * 0.7 + sin((q.x + q.y) * 2.3) * 0.3;
  return field + (fract(sin(w.x * 12.9898 + w.y * 78.233) * 43758.5453) - 0.5) * 0.3;
}
fn orbit(w: vec2f) -> vec3f {
  let rel = w - u.centre;
  if (u.orbit.w < 0.5) { return vec3f(rel, 1.0); }
  var p = vec3f(rel, orbitDepth(w) * u.orbit.z);
  let cy = cos(u.orbit.x);
  let sy = sin(u.orbit.x);
  p = vec3f(cy * p.x + sy * p.z, p.y, -sy * p.x + cy * p.z);
  let cp = cos(u.orbit.y);
  let sp = sin(u.orbit.y);
  p = vec3f(p.x, cp * p.y - sp * p.z, sp * p.y + cp * p.z);
  let dist = max(u.extent, 1.0) * 3.0;
  let f = dist / max(dist + p.z, dist * 0.2);
  return vec3f(p.xy * f, f);
}
`;
const SHAPE_WGSL = WGSL_COMMON + `
struct S {
  @builtin(position) position: vec4f,
  @location(0) local: vec2f, @location(1) colour: vec4f, @location(2) kind: f32, @location(3) pixels: vec2f, @location(4) seed: f32,
};
@vertex fn vs(@location(0) centreA: vec2f, @location(1) halfA: vec2f, @location(2) corner: vec2f, @location(3) colour: vec4f,
              @location(4) kind: f32, @location(5) centreB: vec2f, @location(6) halfB: vec2f) -> S {
  var o: S;
  let centre = cosmic(mix(centreA, centreB, u.t));
  let extent = mix(halfA, halfB, u.t);
  let seen = orbit(centre);
  o.seed = fract(sin(dot(mix(centreA, centreB, u.t), vec2f(12.9898, 78.233))) * 43758.5453);
  var pixels = max(extent * u.scale * u.ratio * seen.z, vec2f(0.75));
  let middle = seen.xy * u.scale * u.ratio;
  if (u.breath.y > 0.0 && kind < 1.5) { pixels = pixels * (1.0 + u.breath.y * sin(u.breath.x - 6.2831853 * length(middle) / u.breath.z)); }
  let screen = lens(middle + corner * pixels).xy;
  let clip = screen / (u.size * 0.5);
  o.position = vec4f(clip.x, -clip.y, 0.0, 1.0);
  o.local = corner; o.colour = colour; o.kind = kind; o.pixels = pixels * lens(middle).z;
  return o;
}
fn outlineDistance(p: vec2f, outline: f32) -> f32 {
  let a = abs(p);
  if (outline < 0.5) { return length(p); }
  if (outline < 1.5) { return max(a.x, a.y) * 1.4142136; }
  if (outline < 2.5) { return max(max(a.x, a.y), (a.x + a.y) * 0.7071068) / 0.9238795; }
  return a.x + a.y;
}
@fragment fn fs(i: S) -> @location(0) vec4f {
  var colour: vec4f;
  if (i.kind > 3.5) {
    let d = length(i.local);
    if (d > 1.0) { discard; }
    let glow = exp(-d * d * 4.5) * (1.0 - d);
    var a = max(1.0 - smoothstep(0.22, 0.34, d), glow * 0.55);
    if (i.kind < 4.5) { a = glow; }
    if (i.kind > 4.5 && u.twinkle.y > 0.0) { a = a * (1.0 - 0.4 * u.twinkle.y * (0.5 + 0.5 * sin(u.twinkle.x * 2.3 + i.seed * 6.2831853))); }
    colour = vec4f(i.colour.rgb, i.colour.a * a);
  } else if (i.kind < 1.5) {
    let base = floor(i.kind + 0.05);
    let d = outlineDistance(i.local, floor((i.kind - base) * 10.0 + 0.5));
    if (d > 1.0) { discard; }
    let edge = 1.0 / max(i.pixels.x, 1.0);
    if (base < 0.5) {
      colour = vec4f(i.colour.rgb, i.colour.a * (1.0 - smoothstep(1.0 - edge, 1.0, d)));
    } else {
      let line = 1.0 - smoothstep(1.2 * edge, 2.4 * edge, 1.0 - d);
      colour = vec4f(i.colour.rgb, max(line * 0.8, 0.045));
    }
  } else {
    let inside = (1.0 - abs(i.local)) * i.pixels;
    let border = min(inside.x, inside.y);
    let line = 1.0 - smoothstep(0.6, 1.4, border);
    if (i.kind < 2.5) { colour = vec4f(mix(vec3f(0.14, 0.21, 0.29), i.colour.rgb, line), 1.0); }
    else { colour = vec4f(i.colour.rgb, max(line * 0.9, i.colour.a)); }
  }
  return vec4f(colour.rgb, colour.a * u.fade);
}`;
const LINE_WGSL = WGSL_COMMON + `
struct L { @builtin(position) position: vec4f, @location(0) colour: vec4f };
@vertex fn vs(@location(0) a: vec2f, @location(1) colour: vec4f, @location(2) b: vec2f) -> L {
  var o: L;
  let clip = lens(orbit(cosmic(mix(a, b, u.t))).xy * u.scale * u.ratio).xy / (u.size * 0.5);
  o.position = vec4f(clip.x, -clip.y, 0.0, 1.0);
  o.colour = colour;
  return o;
}
@fragment fn fs(i: L) -> @location(0) vec4f { return vec4f(i.colour.rgb, i.colour.a * u.alpha * u.fade); }`;

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

const PARTS = ['edges', 'buses', 'discs', 'back', 'routes', 'arrows', 'front'];
const SHAPE_PARTS = new Set(['discs', 'back', 'front']);
// The parts in drawing order and how each is drawn.
const DRAW_ORDER = [['edges', 'lines'], ['buses', 'triangles'], ['discs', 'triangles'], ['back', 'triangles'], ['routes', 'lines'], ['arrows', 'triangles'], ['front', 'triangles']];

// A backend draws layers (per part a buffer of a morph's start and one of its end) with the uniform values the renderer
// computes: `layer(from, to)`, `drop(layer)`, `frame(draws, background, blend)` with draws = [{layer, uniforms}] and blend
// 'over' (alpha), 'add' (the galaxy style: light adds up) or 'ink' (the galaxy style on a light background: the colours,
// absorptions, are subtracted; the alpha stays opaque), `destroy()`. WebGL 2 runs the GLSL ES 1.00 programs as they are; `webgl1` forces WebGL 1.
function webglBackend(canvas, webgl1 = false) {
  const settings = {antialias: true, premultipliedAlpha: false};
  const gl = (webgl1 ? null : canvas.getContext('webgl2', settings)) ?? canvas.getContext('webgl', settings);
  if (!gl) throw Error('WebGL is not available');
  const kind = typeof WebGL2RenderingContext !== 'undefined' && gl instanceof WebGL2RenderingContext ? 'webgl2' : 'webgl';
  const shapes = program(gl, SHAPE_VS, SHAPE_FS), lines = program(gl, LINE_VS, LINE_FS);
  const attribute = (p, name, buffer, stride, size, offset) => {
    const at = gl.getAttribLocation(p, name);
    if (at < 0) return;
    gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
    gl.enableVertexAttribArray(at);
    gl.vertexAttribPointer(at, size, gl.FLOAT, false, stride * 4, offset * 4);
  };
  const set = (p, u) => {
    const at = name => gl.getUniformLocation(p, name);
    gl.uniform2f(at('u_centre'), u.centre[0], u.centre[1]);
    gl.uniform1f(at('u_scale'), u.scale);
    gl.uniform2f(at('u_size'), u.size[0], u.size[1]);
    gl.uniform1f(at('u_ratio'), u.ratio);
    gl.uniform1f(at('u_t'), u.t);
    gl.uniform1f(at('u_fade'), u.fade);
    gl.uniform4f(at('u_lens'), ...u.lens);
    gl.uniform3f(at('u_breath'), ...u.breath);
    gl.uniform3f(at('u_cosmic'), ...u.cosmic);
    gl.uniform2f(at('u_twinkle'), ...u.twinkle);
    gl.uniform4f(at('u_orbit'), ...u.orbit);
    gl.uniform1f(at('u_extent'), u.extent);
  };
  const drawPart = (l, part, mode, u) => {
    if (!l.counts[part]) return;
    const {a, b} = l.buffers[part];
    // Only the arrays of the program in use are on: one left on by the other program would point at a buffer of the
    // wrong size.
    for (let i = 0; i < 8; i++) gl.disableVertexAttribArray(i);
    if (SHAPE_PARTS.has(part)) {
      gl.useProgram(shapes);
      set(shapes, u);
      attribute(shapes, 'a_centre', a, SHAPE_STRIDE, 2, 0);
      attribute(shapes, 'a_half', a, SHAPE_STRIDE, 2, 2);
      attribute(shapes, 'a_corner', a, SHAPE_STRIDE, 2, 4);
      attribute(shapes, 'a_colour', a, SHAPE_STRIDE, 4, 6);
      attribute(shapes, 'a_kind', a, SHAPE_STRIDE, 1, 10);
      attribute(shapes, 'a_centre2', b, SHAPE_STRIDE, 2, 0);
      attribute(shapes, 'a_half2', b, SHAPE_STRIDE, 2, 2);
    } else {
      gl.useProgram(lines);
      set(lines, u);
      gl.uniform1f(gl.getUniformLocation(lines, 'u_alpha'), part === 'edges' ? u.edgeAlpha : 1);
      attribute(lines, 'a_position', a, LINE_STRIDE, 2, 0);
      attribute(lines, 'a_colour', a, LINE_STRIDE, 4, 2);
      attribute(lines, 'a_position2', b, LINE_STRIDE, 2, 0);
    }
    gl.drawArrays(mode, 0, l.counts[part]);
  };
  return {
    kind,
    layer(from, to = from) {
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
    },
    drop(l) {
      if (!l) return;
      for (const p of PARTS) { gl.deleteBuffer(l.buffers[p].a); if (l.buffers[p].b !== l.buffers[p].a) gl.deleteBuffer(l.buffers[p].b); }
    },
    frame(draws, background, blend) {
      gl.viewport(0, 0, canvas.width, canvas.height);
      gl.clearColor(...background);
      gl.clear(gl.COLOR_BUFFER_BIT);
      gl.enable(gl.BLEND);
      if (blend === 'ink') {
        gl.blendEquationSeparate(gl.FUNC_REVERSE_SUBTRACT, gl.FUNC_ADD);
        gl.blendFuncSeparate(gl.SRC_ALPHA, gl.ONE, gl.ZERO, gl.ONE);
      } else {
        gl.blendEquation(gl.FUNC_ADD);
        gl.blendFunc(gl.SRC_ALPHA, blend === 'add' ? gl.ONE : gl.ONE_MINUS_SRC_ALPHA);
      }
      for (const {layer, uniforms} of draws) for (const [part, how] of DRAW_ORDER) drawPart(layer, part, how === 'lines' ? gl.LINES : gl.TRIANGLES, uniforms);
    },
    destroy() { gl.deleteProgram(shapes); gl.deleteProgram(lines); },
  };
}

// WebGPU: the WGSL programs, 4× multisampling, a pipeline per program, topology and blend (additive or ink in the galaxy
// style), one uniform block per draw at a dynamic offset. The canvas is opaque: the WebGL canvas composites its alpha
// over the page, which matters only where the page differs from the clear colour.
function webgpuBackend(canvas, device) {
  const context = canvas.getContext('webgpu');
  if (!context) throw Error('WebGPU is not available on this canvas');
  const format = navigator.gpu.getPreferredCanvasFormat();
  context.configure({device, format, alphaMode: 'opaque'});
  const SLOT = 256, MAX_DRAWS = 2 * DRAW_ORDER.length;
  const uniform = device.createBuffer({size: SLOT * MAX_DRAWS, usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST});
  const groupLayout = device.createBindGroupLayout({entries: [{binding: 0, visibility: GPUShaderStage.VERTEX | GPUShaderStage.FRAGMENT, buffer: {hasDynamicOffset: true}}]});
  const group = device.createBindGroup({layout: groupLayout, entries: [{binding: 0, resource: {buffer: uniform, size: 128}}]});
  const layout = device.createPipelineLayout({bindGroupLayouts: [groupLayout]});
  const attrs = (list, stride) => ({arrayStride: stride * 4, stepMode: 'vertex', attributes: list.map(([loc, size, offset]) => ({shaderLocation: loc, offset: offset * 4, format: size === 1 ? 'float32' : `float32x${size}`}))});
  const shapeBuffers = [attrs([[0, 2, 0], [1, 2, 2], [2, 2, 4], [3, 4, 6], [4, 1, 10]], SHAPE_STRIDE), attrs([[5, 2, 0], [6, 2, 2]], SHAPE_STRIDE)];
  const lineBuffers = [attrs([[0, 2, 0], [1, 4, 2]], LINE_STRIDE), attrs([[2, 2, 0]], LINE_STRIDE)];
  const modules = {shapes: device.createShaderModule({code: SHAPE_WGSL}), lines: device.createShaderModule({code: LINE_WGSL})};
  const blendOf = blend => {
    if (blend === 'ink') return {color: {operation: 'reverse-subtract', srcFactor: 'src-alpha', dstFactor: 'one'}, alpha: {operation: 'add', srcFactor: 'zero', dstFactor: 'one'}};
    const c = {srcFactor: 'src-alpha', dstFactor: blend === 'add' ? 'one' : 'one-minus-src-alpha'};
    return {color: c, alpha: c};
  };
  const pipelines = new Map();
  const pipeline = (program, topology, blend) => {
    const key = `${program}:${topology}:${blend}`;
    if (!pipelines.has(key)) pipelines.set(key, device.createRenderPipeline({
      layout,
      vertex: {module: modules[program], entryPoint: 'vs', buffers: program === 'shapes' ? shapeBuffers : lineBuffers},
      fragment: {module: modules[program], entryPoint: 'fs', targets: [{format, blend: blendOf(blend)}]},
      primitive: {topology},
      multisample: {count: 4},
    }));
    return pipelines.get(key);
  };
  let target = null;
  const targetFor = (w, h) => {
    if (target && target.w === w && target.h === h) return target;
    target?.texture.destroy();
    const texture = device.createTexture({size: [w, h], format, sampleCount: 4, usage: GPUTextureUsage.RENDER_ATTACHMENT});
    target = {w, h, texture, view: texture.createView()};
    return target;
  };
  const upload = data => {
    const buffer = device.createBuffer({size: Math.max(4, Math.ceil(data.byteLength / 4) * 4), usage: GPUBufferUsage.VERTEX | GPUBufferUsage.COPY_DST});
    if (data.byteLength) device.queue.writeBuffer(buffer, 0, data);
    return buffer;
  };
  const block = new Float32Array(SLOT / 4 * MAX_DRAWS);
  return {
    kind: 'webgpu',
    layer(from, to = from) {
      const out = {buffers: {}, counts: from.count};
      for (const p of PARTS) { const a = upload(from[p]); out.buffers[p] = {a, b: to !== from ? upload(to[p]) : a}; }
      return out;
    },
    drop(l) {
      if (!l) return;
      for (const p of PARTS) { l.buffers[p].a.destroy(); if (l.buffers[p].b !== l.buffers[p].a) l.buffers[p].b.destroy(); }
    },
    frame(draws, background, blend) {
      const t = targetFor(canvas.width, canvas.height);
      // All uniform blocks of the frame first, in one write; then the draws read them at their offsets.
      const list = [];
      for (const {layer, uniforms: u} of draws) for (const [part, how] of DRAW_ORDER) {
        if (!layer.counts[part] || list.length >= MAX_DRAWS) continue;
        const at = list.length * SLOT / 4;
        block.set([u.centre[0], u.centre[1], u.size[0], u.size[1], u.scale, u.ratio, u.t, u.fade, ...u.lens, ...u.breath, 0, ...u.cosmic, 0, ...u.twinkle, 0, 0, ...u.orbit, u.extent, part === 'edges' ? u.edgeAlpha : 1, 0, 0], at);
        list.push({layer, part, how});
      }
      if (list.length) device.queue.writeBuffer(uniform, 0, block, 0, list.length * SLOT / 4);
      const encoder = device.createCommandEncoder();
      const pass = encoder.beginRenderPass({colorAttachments: [{view: t.view, resolveTarget: context.getCurrentTexture().createView(), loadOp: 'clear', storeOp: 'discard',
        clearValue: {r: background[0], g: background[1], b: background[2], a: background[3]}}]});
      list.forEach(({layer, part, how}, k) => {
        pass.setPipeline(pipeline(SHAPE_PARTS.has(part) ? 'shapes' : 'lines', how === 'lines' ? 'line-list' : 'triangle-list', blend));
        pass.setBindGroup(0, group, [k * SLOT]);
        pass.setVertexBuffer(0, layer.buffers[part].a);
        pass.setVertexBuffer(1, layer.buffers[part].b);
        pass.draw(layer.counts[part]);
      });
      pass.end();
      device.queue.submit([encoder.finish()]);
    },
    destroy() { target?.texture.destroy(); uniform.destroy(); context.unconfigure(); },
  };
}

/** Ease in and out: a move starts and ends gently. */
const ease = t => t < 0.5 ? 4 * t * t * t : 1 - (-2 * t + 2) ** 3 / 2;

// The default clear colour, and the tone of a clear colour: light above one half relative luminance (sRGB weights).
const DARK_BACKGROUND = [0.047, 0.063, 0.094, 1];
const toneOf = ([r, g, b]) => (0.2126 * r + 0.7152 * g + 0.0722 * b > 0.5 ? 'light' : 'dark');

/**
 * Provisional: a CSS mask that fades an element out over `width` CSS pixels at each of its four edges (eased, so the
 * transition has no visible band), for `mask-image` with `mask-composite: intersect`; '' for a width of 0 or less.
 */
export function edgeFadeMask(width) {
  if (!(width > 0)) return '';
  const steps = [0, 0.2, 0.4, 0.6, 0.8, 1].map(t => [t * t * (3 - 2 * t), t * width]);
  const one = direction => `linear-gradient(${direction}, ${[
    ...steps.map(([a, at]) => `rgba(0,0,0,${a.toFixed(3)}) ${at.toFixed(1)}px`),
    ...[...steps].reverse().map(([a, at]) => `rgba(0,0,0,${a.toFixed(3)}) calc(100% - ${at.toFixed(1)}px)`),
  ].join(', ')})`;
  return `${one('to right')}, ${one('to bottom')}`;
}

// Per renderer the navigation bindings that follow its pause and resume (called with true at pause, false at resume).
const lifecycle = new WeakMap();

/**
 * Draws `model` (from `overviewModel`) into `canvas`. Returns a renderer: `view` is `{centreX, centreY, scale}` with
 * `scale` in CSS pixels per unit; `setState({expanded, collapsed, selected, trace}, {fade})` draws clusters structured in
 * their discs (index → {scene}), collapsed into one disc (indices), marks one and a path (`pathOf`), blending the change in over `fade`
 * milliseconds; `morph(model, {duration, view})` moves the drawing to a new layout of the same graph (and the view to
 * `view`). `options.labels` is a 2D canvas laid over `canvas` for texts; `options.onView(view)` is called after every
 * view change; `options.edgeAlpha` (default 1) scales the opacity of the overview's edges; `options.background` is the
 * clear colour (RGBA in 0..1, `setBackground`). A light background (relative luminance above one half) sets the tone
 * to light: texts are dark, and the galaxy style draws in ink instead of light. `options.edgeFade` (CSS pixels,
 * `setEdgeFade`) fades the drawing out at the canvas edges into the page behind it. `pause()` stops all drawing while the
 * canvas is hidden and `resume()` draws again (provisional).
 */
export function createOverviewRenderer(canvas, initialModel, options = {}) {
  // WebGPU with a device (`options.device`, from `sceneBackend`), else WebGL 2, else WebGL 1; `options.backend: 'webgl'`
  // keeps WebGL even with a device, `'webgl1'` forces WebGL 1 (the reference the others are checked against).
  const backend = options.device && options.backend !== 'webgl' && options.backend !== 'webgl1' ? webgpuBackend(canvas, options.device) : webglBackend(canvas, options.backend === 'webgl1');
  let model = initialModel, expanded = new Map(), collapsed = new Set(), selected = -1, trace = null;
  // Paused (`pause`, `resume`): no frame is drawn and no animation frame requested. The animations' clock stands still
  // meanwhile (`now` leaves out the time spent paused), so a morph, a blend or breathing goes on at resume where it was,
  // and one started while paused starts at resume. A resize asked for while paused waits for resume too (a hidden canvas
  // may measure 0).
  let paused = false, pausedAt = 0, pausedFor = 0, resizeWanted = false, destroyed = false;
  const now = () => (paused ? pausedAt : performance.now()) - pausedFor;
  // The fisheye lens (`setLens`), in CSS pixels of the canvas, or null.
  let lens = null;
  // Breathing (`breathe`): {amplitude, period, wavelength, start, stop?}; it fades in over `fadeIn` and out over `fadeOut`
  // milliseconds after `stop`, and is dropped once faded out.
  let breath = null;
  const breathState = () => {
    if (!breath) return {phase: 0, amplitude: 0};
    const t = now();
    const into = Math.min(1, (t - breath.start) / breath.fadeIn);
    const out = breath.stop ? Math.max(0, 1 - (t - breath.stop) / breath.fadeOut) : 1;
    return {phase: 2 * Math.PI * (t - breath.start) / breath.period, amplitude: breath.amplitude * ease(into) * ease(out)};
  };
  // A layer: per part a buffer of the start and one of the end of a morph (the same buffer when nothing moves).
  const layer = (from, to = from) => backend.layer(from, to);
  const drop = l => backend.drop(l);
  let outlines = options.shapes ?? {};
  // Provisional: the galaxy style (`setGalaxy`): glows and stars instead of discs and rings, faint lines, light added.
  let galaxy = !!options.galaxy;
  // The clear colour and the tone it gives (`setBackground`): on a light background the galaxy style is ink.
  let background = options.background ?? DARK_BACKGROUND, tone = toneOf(background);
  // Provisional: the fade at the edges (`setEdgeFade`), in CSS pixels; a CSS mask on the canvas and the texts' canvas, so
  // the page shows through at the edges (a dark galaxy in a light host, for example). 0: none.
  let edgeFade = 0;
  const applyEdgeFade = () => {
    const mask = edgeFadeMask(edgeFade);
    for (const element of [canvas, options.labels]) {
      if (!element?.style) continue;
      for (const [name, value] of [['mask-image', mask], ['-webkit-mask-image', mask], ['mask-composite', mask && 'intersect'], ['-webkit-mask-composite', mask && 'source-in']]) {
        if (value) element.style.setProperty(name, value); else element.style.removeProperty(name);
      }
    }
  };
  // The appearance (style.js): the theme's trace, route, line and bus colours and a palette for the clusters.
  let style = options.style ?? null;
  // How structured clusters are drawn (ADR 0025 O3): 'cluster' (boxes in the cluster's colour) or 'scene' (as the scene view).
  let structured = options.structured ?? 'cluster';
  // Provisional: the orbit (pseudo-3D, `setOrbit`): {yaw, pitch, depth} in radians and world units, or null (flat).
  let orbit = null;
  // Provisional: names of nodes when they are large enough on the screen, and their details (`setNames`).
  let names = {nodes: !!options.names, details: !!options.details};
  // The edges' total length in world units, measured whenever the buffers are built (for the adaptive dimming).
  let edgeLength = 0;
  const data = () => {
    const d = overviewBuffers(model, expanded, collapsed, selected, {shapes: outlines, trace, galaxy, style, structured, tone});
    let sum = 0;
    for (let k = 0; k + 1 < d.count.edges; k += 2) {
      const a = k * LINE_STRIDE, b = (k + 1) * LINE_STRIDE;
      sum += Math.hypot(d.edges[b] - d.edges[a], d.edges[b + 1] - d.edges[a + 1]);
    }
    edgeLength = sum;
    return d;
  };
  // Adaptive dimming (`options.edgeDimming`, on): how many edge lines lie over a pixel on average in what is visible (the
  // edges' length on the screen, of the part the canvas shows, over its area); the more, the fainter each line, so that
  // where thousands overlap they do not add up to a white tangle. Zoomed out the drawing is dimmed, zoomed in the lines
  // come back.
  const edgeDim = view => {
    if (options.edgeDimming === false) return 1;
    const w = canvas.clientWidth || 1, h = canvas.clientHeight || 1, extent = 2 * (model.overview.rootRadius || 1) * view.scale;
    const coverage = edgeLength * view.scale / Math.max(w * h, extent * extent);
    return 1 / Math.sqrt(1 + coverage / 1.5);
  };
  let current = layer(data()), previous = null;
  // The running animation: a morph (`t` of the current layer, and the view) and a fade (of the previous layer).
  let morphing = null, fading = null, frame = 0;

  const ratio = () => globalThis.devicePixelRatio || 1;
  // The uniform values of a draw, for every backend.
  const uniformsOf = (view, t, fade) => {
    const r = ratio(), b = breathState();
    // In the galaxy style the idle breath is cosmic motion instead (its strength fades in and out as the breath does).
    const strength = galaxy && breath ? b.amplitude / Math.max(breath.amplitude, 1e-9) : 0;
    const seconds = breath ? (now() - breath.start) / 1000 : 0;
    return {
      centre: [view.centreX, view.centreY], scale: view.scale, size: [canvas.width, canvas.height], ratio: r, t, fade,
      // The lens in device pixels from the canvas centre (y down, as the shaders compute before flipping).
      lens: [lens ? (lens.x - canvas.clientWidth / 2) * r : 0, lens ? (lens.y - canvas.clientHeight / 2) * r : 0, lens ? lens.radius * r : 0, lens ? lens.magnification : 1],
      breath: [b.phase, galaxy ? 0 : b.amplitude, (breath?.wavelength ?? 600) * r],
      cosmic: [seconds, strength, model.overview.rootRadius || 1],
      // Wrapped, so that the fragment shader's lower precision stays fine over a long idle time.
      twinkle: [seconds % 2732, strength],
      orbit: [orbit?.yaw ?? 0, orbit?.pitch ?? 0, orbit?.depth ?? 0, orbit ? 1 : 0],
      extent: model.overview.rootRadius || 1,
      edgeAlpha: (options.edgeAlpha ?? 1) * edgeDim(view),
    };
  };
  const writeLabels = (view, show) => {
    const c = options.labels;
    if (!c) return;
    const w = Math.round(c.clientWidth * ratio()), h = Math.round(c.clientHeight * ratio());
    if (c.width !== w || c.height !== h) { c.width = w; c.height = h; }
    const ctx = c.getContext('2d'), ink = tone === 'light';
    ctx.setTransform(ratio(), 0, 0, ratio(), 0, 0);
    ctx.clearRect(0, 0, c.clientWidth, c.clientHeight);
    // The lens's rim, faint, so that one sees where the enlargement ends.
    if (lens) {
      ctx.beginPath();
      ctx.arc(lens.x, lens.y, lens.radius, 0, 2 * Math.PI);
      ctx.strokeStyle = ink ? 'rgba(27,42,58,0.32)' : 'rgba(223,231,239,0.32)';
      ctx.lineWidth = 1;
      ctx.stroke();
    }
    // While the drawing moves, the texts wait: they would sit at the places of the end.
    if (!show) return;
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    for (const label of labelsOf(model, expanded, view, c.clientWidth, c.clientHeight, {collapsed, lens, galaxy, orbit, names: names.nodes, details: names.details})) {
      ctx.font = `${label.kind === 'cluster' ? 600 : 400} ${label.size.toFixed(1)}px system-ui, sans-serif`;
      if (label.colour) {
        // A galaxy's tag: a rounded box (dark, light on a light background), a dot in its colour, its name.
        const width = ctx.measureText(label.text).width, dot = label.size * 0.36, pad = label.size * 0.55;
        const w = width + dot * 2 + pad * 3, h = label.size * 1.8, left = label.x - w / 2, top = label.y - h / 2;
        ctx.beginPath();
        ctx.roundRect?.(left, top, w, h, h * 0.3) ?? ctx.rect(left, top, w, h);
        ctx.fillStyle = ink ? 'rgba(255,255,255,0.86)' : 'rgba(16,20,28,0.82)';
        ctx.fill();
        if (ink) { ctx.strokeStyle = 'rgba(27,42,58,0.18)'; ctx.lineWidth = 1; ctx.stroke(); }
        ctx.beginPath();
        ctx.arc(left + pad + dot, label.y, dot, 0, 2 * Math.PI);
        ctx.fillStyle = `rgb(${label.colour.map(v => Math.round(v * 255)).join(',')})`;
        ctx.fill();
        ctx.textAlign = 'left';
        ctx.fillStyle = ink ? 'rgba(27,42,58,0.95)' : 'rgba(236,241,247,0.95)';
        ctx.fillText(label.text, left + pad * 2 + dot * 2, label.y);
        ctx.textAlign = 'center';
        continue;
      }
      ctx.fillStyle = ink ? (label.kind === 'cluster' ? 'rgba(27,42,58,0.85)' : 'rgba(27,42,58,0.95)') : label.kind === 'cluster' ? 'rgba(223,231,239,0.85)' : 'rgba(223,231,239,0.95)';
      ctx.fillText(label.text, label.x, label.y);
    }
  };
  const progress = a => a ? Math.min(1, (now() - a.start) / a.duration) : 1;
  const tick = () => {
    frame = 0;
    if (paused || destroyed) return;
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
    if (breath?.stop && now() - breath.stop >= breath.fadeOut) breath = null;
    // Breathing alone leaves the texts as they are: they do not move, and placing them every frame is work.
    renderer.draw({labels: !!(morphing || fading) || !breath});
    if (morphing || fading || breath) frame = requestAnimationFrame(tick);
  };
  const animate = () => { if (!frame && !paused && !destroyed) frame = requestAnimationFrame(tick); };
  // One drawing on the next frame (however often the lens or the orbit changes), and the animation going on after it;
  // while paused nothing: resume draws.
  const soon = () => {
    if (paused || destroyed || frame) return;
    frame = requestAnimationFrame(() => { frame = 0; renderer.draw(); if (morphing || fading || breath) animate(); });
  };
  const matchSize = () => {
    const w = Math.round(canvas.clientWidth * ratio()), h = Math.round(canvas.clientHeight * ratio());
    if (canvas.width !== w || canvas.height !== h) { canvas.width = w; canvas.height = h; }
  };
  const finishMorph = () => {
    if (!morphing) return;
    const done = morphing;
    morphing = null;
    drop(current);
    current = layer(done.to);
    if (done.view) renderer.view = {...done.view[1]};
    options.onView?.(renderer.view);
  };

  // Picking under the orbit: the node whose projected disc holds the point, the smallest such (a star before the cloud
  // around it); not inside structured clusters.
  const pickOrbit = (clientX, clientY) => {
    const rect = canvas.getBoundingClientRect(), v = renderer.view, o = model.overview, extent = o.rootRadius || 1;
    const mx = clientX - rect.left - rect.width / 2, my = clientY - rect.top - rect.height / 2;
    const owner = ownersOf(model, expanded, collapsed);
    let best = -1, size = Infinity;
    for (let i = 0; i < model.n; i++) {
      if (owner[i] >= 0) continue;
      const [px, py, f] = orbitPoint(orbit, v, extent, o.x[i], o.y[i]);
      const r = o.r[i] * f * v.scale;
      if ((px * v.scale - mx) ** 2 + (py * v.scale - my) ** 2 <= Math.max(r, 3) ** 2 && r < size) { best = i; size = r; }
    }
    return best >= 0 && model.chainTop ? model.chainTop[best] : best;
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
      soon();
    },
    setExpanded(map) { renderer.setState({expanded: map}); },
    /** The outline per node kind (`circle`, `square`, `octagon`, `diamond`); kinds not named are circles. */
    setShapes(next) { outlines = {...next}; renderer.setState({}); },
    /** Provisional: draws in the galaxy style (glows, stars, faint lines, added light) or back in discs and rings. */
    get galaxy() { return galaxy; },
    setGalaxy(on) { if (galaxy !== !!on) { galaxy = !!on; renderer.setState({}); } },
    /** Provisional: the width in CSS pixels over which the drawing fades out at the canvas edges (0: none). */
    get edgeFade() { return edgeFade; },
    setEdgeFade(width) { edgeFade = Math.max(0, Number(width) || 0); applyEdgeFade(); },
    /** Provisional: 'light' when the background is light (texts dark, the galaxy style in ink), else 'dark'. */
    get tone() { return tone; },
    /** Provisional: the clear colour (RGBA in 0..1), for a host that switches between a dark and a light theme; its
     * luminance sets the tone. Without an argument the default dark background. */
    setBackground(rgba = DARK_BACKGROUND) {
      background = [...rgba];
      const next = toneOf(background);
      if (next === tone) { renderer.draw(); return; }
      tone = next;
      // The galaxy's colours depend on the tone (light or ink); the other drawings only need their texts again.
      if (galaxy) renderer.setState({}, {fade: 250}); else renderer.draw();
    },
    /** Provisional: the appearance (style.js, ADR 0026); null for the overview's own colours. */
    setStyle(next) { style = next ?? null; renderer.setState({}); },
    /** Provisional: how structured clusters are drawn: 'cluster' or 'scene' (as the scene view, with the style). */
    setStructured(mode) { structured = mode === 'scene' ? 'scene' : 'cluster'; renderer.setState({}); },
    /**
     * Provisional: the orbit (pseudo-3D): `{yaw, pitch}` in radians turn the drawing around the view's centre, every point
     * at a depth of up to `depth` (default half the drawing's radius) from a smooth field, so clusters lie at
     * different depths and pass each other when it turns; null draws it flat again.
     */
    get orbit() { return orbit && {...orbit}; },
    /** Provisional: name every node whose disc is 6 pixels across or more (`nodes`), with its kind and links (`details`). */
    get names() { return {...names}; },
    setNames(next) { names = {nodes: !!next?.nodes, details: !!next?.details}; renderer.draw(); },
    setOrbit(next) {
      orbit = next ? {yaw: next.yaw ?? 0, pitch: Math.max(-1.3, Math.min(1.3, next.pitch ?? 0)), depth: next.depth ?? (model.overview.rootRadius || 1) / 2} : null;
      soon();
    },
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
        fading = {start: now(), duration: fade};
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
      morphing = {start: now(), duration, to, view: view ? [{...renderer.view}, {...view}] : null, old};
      animate();
      return true;
    },
    resize() {
      if (paused) { resizeWanted = true; return; }
      matchSize();
      renderer.draw();
    },
    /** Provisional: whether the renderer is paused (see `pause`). */
    get paused() { return paused; },
    /**
     * Provisional: stops drawing while the canvas is hidden (for example under a drill-down): no frame, no animation frame
     * requested, the animations' clock held. The GPU resources, the model, the view and the state are kept, and every call
     * that changes them is taken (the view's `onView` included) but drawn only at `resume`. Idempotent.
     */
    pause() {
      if (paused || destroyed) return;
      paused = true;
      pausedAt = performance.now();
      if (frame) { cancelAnimationFrame(frame); frame = 0; }
      for (const f of lifecycle.get(renderer) ?? []) f(true);
    },
    /**
     * Provisional: ends a pause: matches the canvas size if a resize was asked for meanwhile, draws one frame at once and
     * goes on with what was running (a morph, a blend or breathing, from where it stood). Idempotent.
     */
    resume() {
      if (!paused || destroyed) return;
      pausedFor += performance.now() - pausedAt;
      paused = false;
      if (resizeWanted) { resizeWanted = false; matchSize(); }
      renderer.draw();
      if (morphing || fading || breath) animate();
      for (const f of lifecycle.get(renderer) ?? []) f(false);
    },
    /** The backend drawing it: 'webgpu', 'webgl2' or 'webgl'. */
    get backend() { return backend.kind; },
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
        if (!breath.stop) { breath.stop = now(); breath.fadeOut = fadeOut; animate(); }
        return;
      }
      const {amplitude = 0.06, period = 4000, wavelength = 600, fadeIn = 1500} = settings;
      if (breath && !breath.stop) return;
      breath = {amplitude, period: Math.max(period, 100), wavelength: Math.max(wavelength, 1), fadeIn: Math.max(fadeIn, 1), fadeOut, start: now()};
      animate();
    },
    draw({labels = true} = {}) {
      // Paused: drawn at resume.
      if (paused || destroyed) return;
      const view = renderer.view;
      const t = morphing ? ease(progress(morphing)) : 0;
      const f = fading ? ease(progress(fading)) : 1;
      // The old drawing fades out under the new one, which is drawn whole: what both have does not dim (a blend of two
      // half-transparent drawings would let the background through and flicker), what goes fades, what comes is there.
      const draws = [];
      if (previous) draws.push({layer: previous, uniforms: uniformsOf(view, 0, 1 - f)});
      draws.push({layer: current, uniforms: uniformsOf(view, t, 1)});
      // Galaxy mode adds light: where many glows and lines meet it grows brighter; on a light background it adds ink.
      backend.frame(draws, background, galaxy ? (tone === 'light' ? 'ink' : 'add') : 'over');
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
      if (orbit) return pickOrbit(clientX, clientY);
      const [x, y] = renderer.world(clientX, clientY);
      return pickExpanded(model, expanded, x, y)?.cluster ?? pickNode(model, x, y, collapsed);
    },
    /** The scene node under a CSS pixel inside a structured cluster (see `pickExpanded`), or null. */
    pickScene(clientX, clientY) { return pickExpanded(model, expanded, ...renderer.world(clientX, clientY)); },
    destroy() {
      if (destroyed) return;
      destroyed = true;
      if (frame) { cancelAnimationFrame(frame); frame = 0; }
      lifecycle.delete(renderer);
      if (edgeFade) { edgeFade = 0; applyEdgeFade(); }
      drop(current); drop(previous);
      backend.destroy();
      options.labels?.getContext('2d').clearRect(0, 0, options.labels.width, options.labels.height);
    },
  };
  if (options.edgeFade) renderer.setEdgeFade(options.edgeFade);
  lifecycle.set(renderer, new Set());
  return renderer;
}

/**
 * Wheel zoom at the pointer and drag panning for a renderer. `onClick(index, event)` gets the node under a click that was
 * not a drag; `onHover(index, event)` the node under the pointer; `onDoubleClick(index, event)` a double-click. With
 * `lens` (a function that returns `{radius, magnification}` or null, asked on every move), a fisheye lens follows the
 * pointer and is gone when the pointer leaves the canvas. With `idle` (`{after, breathe}`), the discs start to breathe
 * (`renderer.breathe(breathe)`) after `after` milliseconds (default 8000) without input, and stop at the next input; not
 * when the system asks for reduced motion. While the renderer is paused (`renderer.pause()`) the binding takes no input
 * (no zoom, pan, drag, lens, hover or click; a node drag in progress ends with `onNodeDrag(…, 'end')` at its last point)
 * and its idle wait is held; at `resume` the wait starts again unless the drawing still breathes. Returns a function that
 * removes the listeners.
 */
export function bindOverviewNavigation(canvas, renderer, {onClick, onHover, onDoubleClick, onNodeDrag, draggable, lens, idle} = {}) {
  let drag = null;
  // Breathing after `idle.after` milliseconds without input; any input stops it and starts the wait again. Not when
  // the user asked the system for less motion.
  const still = globalThis.matchMedia?.('(prefers-reduced-motion: reduce)')?.matches;
  let idleTimer = 0;
  const wait = () => {
    clearTimeout(idleTimer);
    idleTimer = setTimeout(() => { if (!drag && !renderer.paused) renderer.breathe(idle.breathe ?? {}); }, idle.after ?? 8000);
  };
  const awake = () => {
    if (!idle || still) return;
    renderer.breathe(null);
    wait();
  };
  awake();
  // The renderer's pause holds the idle wait and ends a drag; its resume starts the wait again (breathing that ran on
  // before the pause goes on).
  const onPause = paused => {
    if (paused) {
      clearTimeout(idleTimer);
      // A node drag ends where the node was last moved to, so the host is not left in the middle of it.
      if (drag?.moved && drag.node >= 0 && drag.last) onNodeDrag(drag.node, ...drag.last, 'end');
      drag = null;
      return;
    }
    if (idle && !still && !renderer.breathing) wait();
  };
  lifecycle.get(renderer)?.add(onPause);
  // Input on a paused renderer is ignored: it would change what nobody sees.
  const off = () => !!renderer.paused;
  const input = () => { if (!off()) awake(); };
  const follow = e => {
    if (!lens) return;
    const settings = lens(), rect = canvas.getBoundingClientRect();
    renderer.setLens(settings ? {...settings, x: e.clientX - rect.left, y: e.clientY - rect.top} : null);
  };
  const leave = () => { if (lens) renderer.setLens(null); };
  const wheel = e => {
    e.preventDefault();
    if (off()) return;
    awake();
    const [wx, wy] = renderer.world(e.clientX, e.clientY);
    const v = renderer.view, factor = Math.exp(-e.deltaY * 0.0015);
    const scale = Math.min(Math.max(v.scale * factor, 1e-4), 1e4);
    // Keep the world point under the pointer where it is; under the orbit the flat point is not the one shown there, so the
    // zoom keeps the view's centre (what the orbit turns around).
    if (renderer.orbit) { renderer.setView({...v, scale}); return; }
    renderer.setView({scale, centreX: wx - (wx - v.centreX) * v.scale / scale, centreY: wy - (wy - v.centreY) * v.scale / scale});
  };
  // With `onNodeDrag`, a drag that starts on a leaf (a node without children) moves that node: `onNodeDrag(i, x, y,
  // phase)` with the world point, `move` while dragging and `end` at the release; any other drag pans. `draggable(i)`
  // (of the node under the pointer, -1 for none) chooses another node to move, or -1 to pan.
  const leafAt = e => {
    // Under the orbit a drag turns the drawing; nodes are not moved.
    if (!onNodeDrag || renderer.orbit) return -1;
    const i = renderer.pick(e.clientX, e.clientY), m = renderer.model;
    if (draggable) return draggable(i);
    return i >= 0 && m && !m.children[i].length ? i : -1;
  };
  const down = e => { if (off()) return; awake(); drag = {x: e.clientX, y: e.clientY, view: renderer.view, orbit: renderer.orbit, moved: false, node: leafAt(e)}; canvas.setPointerCapture?.(e.pointerId); };
  const move = e => {
    if (off()) return;
    awake();
    follow(e);
    if (!drag) { onHover?.(renderer.pick(e.clientX, e.clientY), e); return; }
    const dx = e.clientX - drag.x, dy = e.clientY - drag.y;
    if (!drag.moved && Math.hypot(dx, dy) < 4) return;
    drag.moved = true;
    if (drag.node >= 0) { drag.last = renderer.world(e.clientX, e.clientY); onNodeDrag(drag.node, ...drag.last, 'move'); return; }
    // Under the orbit a drag turns the drawing (about half a turn across the canvas); with Shift it pans.
    if (drag.orbit && !e.shiftKey) {
      renderer.setOrbit({...drag.orbit, yaw: drag.orbit.yaw + dx * 0.006, pitch: drag.orbit.pitch + dy * 0.006});
      return;
    }
    renderer.setView({...drag.view, centreX: drag.view.centreX - dx / drag.view.scale, centreY: drag.view.centreY - dy / drag.view.scale});
  };
  const up = e => {
    if (drag && !drag.moved) onClick?.(renderer.pick(e.clientX, e.clientY), e);
    else if (drag && drag.node >= 0) onNodeDrag(drag.node, ...renderer.world(e.clientX, e.clientY), 'end');
    drag = null;
  };
  const cancel = () => { drag = null; };
  const double = e => off() || onDoubleClick?.(renderer.pick(e.clientX, e.clientY), e);
  canvas.addEventListener('dblclick', double);
  canvas.addEventListener('wheel', wheel, {passive: false});
  canvas.addEventListener('pointerdown', down);
  canvas.addEventListener('pointermove', move);
  canvas.addEventListener('pointerup', up);
  canvas.addEventListener('pointercancel', cancel);
  canvas.addEventListener('pointerleave', leave);
  globalThis.addEventListener?.('keydown', input);
  return () => {
    lifecycle.get(renderer)?.delete(onPause);
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
