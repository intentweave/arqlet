// Provisional (ADR 0025, step V1): the far layer's fallback backend, WebGL 2, for browsers without WebGPU
// (`scene-gpu.js` is the first choice). No dependency. Three programs: boxes (instanced quads, a rounded rectangle or a
// circle found in the fragment shader, fill and stroke), segments (instanced quads with a width in world units and at
// least one device pixel, square caps, optional dashes) and triangles (hulls and arrowheads). Draws what `sceneBuffers`
// returns; computes no layout. The view, texts and picking are the shell's (`scene-renderer.js`).
//
// The lines of one route overlap at their corners; with a depth per line (later lines nearer) and the depth test LESS
// every pixel of a translucent line is blended once, as SVG strokes a polyline, and a later line still covers an
// earlier one. Arrowheads use LEQUAL, so they cover their own line and lie under later ones.
import {BOX_STRIDE, SEGMENT_STRIDE, TRIANGLE_STRIDE} from './scene-buffers.js';
import {rendererShell} from './scene-renderer.js';
export {bindSceneNavigation} from './scene-renderer.js';

const BOX_VS = `#version 300 es
in vec2 a_corner;
in vec4 a_rect; in vec4 a_shape; in vec4 a_fill; in vec4 a_stroke;
uniform vec2 u_centre; uniform float u_scale; uniform vec2 u_size;
out vec2 v_p; out vec2 v_half; out float v_radius; out float v_stroke; out float v_kind; out float v_dash;
out vec4 v_fill; out vec4 v_strokeColour; out float v_unit; out float v_thin;
void main() {
  vec2 centre = (a_rect.xy + a_rect.zw * 0.5 - u_centre) * u_scale;
  // At least half a device pixel, so that a box far out stays a dot.
  vec2 half_ = max(a_rect.zw * 0.5 * u_scale, vec2(0.5));
  float stroke = a_shape.z > 0.0 ? max(a_shape.z * u_scale, 1.0) : 0.0;
  vec2 extent = half_ + stroke * 0.5 + 1.0;
  vec2 screen = centre + a_corner * extent;
  vec2 clip = screen / (u_size * 0.5);
  gl_Position = vec4(clip.x, -clip.y, 0.0, 1.0);
  v_p = a_corner * extent; v_half = half_; v_radius = min(a_shape.x * u_scale, min(half_.x, half_.y));
  v_kind = a_shape.y; v_stroke = stroke; v_dash = a_shape.w; v_unit = u_scale;
  v_fill = a_fill; v_strokeColour = a_stroke;
  // A stroke thinner than a pixel is drawn one pixel wide with its coverage as opacity, as SVG rasterises it.
  v_thin = a_shape.z > 0.0 ? min(a_shape.z * u_scale, 1.0) : 0.0;
}`;
const BOX_FS = `#version 300 es
precision highp float;
in vec2 v_p; in vec2 v_half; in float v_radius; in float v_stroke; in float v_kind; in float v_dash;
in vec4 v_fill; in vec4 v_strokeColour; in float v_unit; in float v_thin;
out vec4 colour;
void main() {
  float d;
  if (v_kind > 0.5) d = length(v_p) - v_half.x;
  else { vec2 q = abs(v_p) - v_half + v_radius; d = length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - v_radius; }
  float fill = clamp(0.5 - d, 0.0, 1.0);
  float stroke = v_stroke > 0.0 ? clamp(v_stroke * 0.5 + 0.5 - abs(d), 0.0, 1.0) : 0.0;
  // Dashes 6 on, 4 off (world units) along the outline: the angle on a circle, the side's own axis on a rectangle.
  if (v_dash > 0.5 && stroke > 0.0) {
    float along = v_kind > 0.5 ? atan(v_p.y, v_p.x) * v_half.x
      : (abs(v_p.x) - v_half.x > abs(v_p.y) - v_half.y ? v_p.y + v_half.y : v_p.x + v_half.x);
    if (mod(along / v_unit, 10.0) > 6.0) stroke = 0.0;
  }
  vec4 f = vec4(v_fill.rgb * v_fill.a, v_fill.a) * fill;
  vec4 s = vec4(v_strokeColour.rgb * v_strokeColour.a, v_strokeColour.a) * stroke * v_thin;
  colour = s + f * (1.0 - s.a);
  if (colour.a <= 0.0) discard;
}`;
const SEGMENT_VS = `#version 300 es
in vec2 a_corner;
in vec4 a_ends; in vec4 a_style; in float a_from; in vec4 a_colour;
uniform vec2 u_centre; uniform float u_scale; uniform vec2 u_size;
out float v_across; out float v_half; out float v_along; out vec2 v_dash; out vec4 v_colour; out float v_thin;
void main() {
  vec2 p0 = (a_ends.xy - u_centre) * u_scale, p1 = (a_ends.zw - u_centre) * u_scale;
  vec2 d = p1 - p0; float len = length(d);
  vec2 dir = len > 0.0 ? d / len : vec2(1.0, 0.0), normal = vec2(-dir.y, dir.x);
  float half_ = max(a_style.x * u_scale, 1.0) * 0.5;
  vec2 end = a_corner.x < 0.0 ? p0 - dir * half_ : p1 + dir * half_;
  float across = a_corner.y * (half_ + 1.0);
  vec2 screen = end + normal * across;
  vec2 clip = screen / (u_size * 0.5);
  gl_Position = vec4(clip.x, -clip.y, a_style.y * 2.0 - 1.0, 1.0);
  v_across = across; v_half = half_; v_colour = a_colour; v_dash = a_style.zw; v_thin = min(a_style.x * u_scale, 1.0);
  v_along = a_from + (a_corner.x < 0.0 ? -half_ : len + half_) / u_scale;
}`;
const SEGMENT_FS = `#version 300 es
precision highp float;
in float v_across; in float v_half; in float v_along; in vec2 v_dash; in vec4 v_colour; in float v_thin;
out vec4 colour;
void main() {
  if (v_dash.x > 0.0 && mod(v_along, v_dash.x + v_dash.y) > v_dash.x) discard;
  float a = clamp(v_half + 0.5 - abs(v_across), 0.0, 1.0) * v_colour.a * v_thin;
  if (a <= 0.0) discard;
  colour = vec4(v_colour.rgb * a, a);
}`;
const TRIANGLE_VS = `#version 300 es
in vec3 a_position; in vec4 a_colour;
uniform vec2 u_centre; uniform float u_scale; uniform vec2 u_size;
out vec4 v_colour;
void main() {
  vec2 clip = (a_position.xy - u_centre) * u_scale / (u_size * 0.5);
  gl_Position = vec4(clip.x, -clip.y, a_position.z * 2.0 - 1.0, 1.0);
  v_colour = a_colour;
}`;
const TRIANGLE_FS = `#version 300 es
precision highp float;
in vec4 v_colour; out vec4 colour;
void main() { colour = vec4(v_colour.rgb * v_colour.a, v_colour.a); }`;

function program(gl, vs, fs) {
  const shader = (type, source) => {
    const s = gl.createShader(type);
    gl.shaderSource(s, source); gl.compileShader(s);
    if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) throw Error(gl.getShaderInfoLog(s));
    return s;
  };
  const p = gl.createProgram();
  gl.attachShader(p, shader(gl.VERTEX_SHADER, vs)); gl.attachShader(p, shader(gl.FRAGMENT_SHADER, fs));
  gl.linkProgram(p);
  if (!gl.getProgramParameter(p, gl.LINK_STATUS)) throw Error(gl.getProgramInfoLog(p));
  const uniforms = {};
  for (const name of ['u_centre', 'u_scale', 'u_size']) uniforms[name] = gl.getUniformLocation(p, name);
  return {p, uniforms, attribute: name => gl.getAttribLocation(p, name)};
}

/** Whether this browser offers WebGL 2 (feature detection for the fallback to SVG). */
export function webgl2Available(doc = globalThis.document) {
  try { return !!doc?.createElement('canvas').getContext('webgl2'); } catch { return false; }
}

/**
 * Draws a scene (or its columns) on `canvas` with WebGL 2. The view is the world point at the canvas centre and CSS
 * pixels per world unit. Throws if WebGL 2 is not available.
 */
export function createSceneRenderer(canvas, scene, options = {}) {
  const gl = canvas.getContext('webgl2', {antialias: true, premultipliedAlpha: true, alpha: true, depth: true, preserveDrawingBuffer: !!options.preserveDrawingBuffer});
  if (!gl) throw Error('WebGL 2 is not available');
  const background = options.background ?? [0, 0, 0, 0];
  const boxes = program(gl, BOX_VS, BOX_FS), segments = program(gl, SEGMENT_VS, SEGMENT_FS), triangles = program(gl, TRIANGLE_VS, TRIANGLE_FS);
  const corners = gl.createBuffer();
  gl.bindBuffer(gl.ARRAY_BUFFER, corners);
  gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 1, -1, -1, 1, 1, 1]), gl.STATIC_DRAW);

  // One VAO per layer and program, over one buffer each.
  const instanced = (prog, data, stride, attributes) => {
    const vao = gl.createVertexArray();
    gl.bindVertexArray(vao);
    gl.bindBuffer(gl.ARRAY_BUFFER, corners);
    const c = prog.attribute('a_corner');
    gl.enableVertexAttribArray(c); gl.vertexAttribPointer(c, 2, gl.FLOAT, false, 0, 0);
    const buffer = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
    gl.bufferData(gl.ARRAY_BUFFER, data, gl.STATIC_DRAW);
    for (const [name, size, offset] of attributes) {
      const at = prog.attribute(name);
      if (at < 0) continue;
      gl.enableVertexAttribArray(at);
      gl.vertexAttribPointer(at, size, gl.FLOAT, false, stride * 4, offset * 4);
      gl.vertexAttribDivisor(at, 1);
    }
    gl.bindVertexArray(null);
    return {vao, buffer, count: data.length / stride};
  };
  const plain = (prog, data) => {
    const vao = gl.createVertexArray();
    gl.bindVertexArray(vao);
    const buffer = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
    gl.bufferData(gl.ARRAY_BUFFER, data, gl.STATIC_DRAW);
    const p = prog.attribute('a_position'), c = prog.attribute('a_colour');
    gl.enableVertexAttribArray(p); gl.vertexAttribPointer(p, 3, gl.FLOAT, false, TRIANGLE_STRIDE * 4, 0);
    gl.enableVertexAttribArray(c); gl.vertexAttribPointer(c, 4, gl.FLOAT, false, TRIANGLE_STRIDE * 4, 12);
    gl.bindVertexArray(null);
    return {vao, buffer, count: data.length / TRIANGLE_STRIDE};
  };

  let layers = [];
  const release = () => { for (const l of layers) for (const part of [l.boxes, l.segments, l.triangles]) { gl.deleteVertexArray(part.vao); gl.deleteBuffer(part.buffer); } layers = []; };
  const uniforms = (prog, view, ratio) => {
    gl.useProgram(prog.p);
    gl.uniform2f(prog.uniforms.u_centre, view.centreX, view.centreY);
    gl.uniform1f(prog.uniforms.u_scale, view.scale * ratio);
    gl.uniform2f(prog.uniforms.u_size, canvas.width, canvas.height);
  };
  const backend = {
    upload(built) {
      release();
      layers = built.layers.map(l => ({
        boxes: instanced(boxes, l.boxes, BOX_STRIDE, [['a_rect', 4, 0], ['a_shape', 4, 4], ['a_fill', 4, 8], ['a_stroke', 4, 12]]),
        segments: instanced(segments, l.segments, SEGMENT_STRIDE, [['a_ends', 4, 0], ['a_style', 4, 4], ['a_from', 1, 8], ['a_colour', 4, 9]]),
        triangles: plain(triangles, l.triangles),
      }));
    },
    draw(view, ratio) {
      gl.viewport(0, 0, canvas.width, canvas.height);
      gl.clearColor(background[0] * background[3], background[1] * background[3], background[2] * background[3], background[3]);
      gl.clearDepth(1);
      gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);
      gl.enable(gl.BLEND);
      gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_ALPHA);
      layers.forEach((l, i) => {
        const routes = i === 2;
        if (routes) { gl.enable(gl.DEPTH_TEST); gl.depthFunc(gl.LESS); gl.depthMask(true); } else gl.disable(gl.DEPTH_TEST);
        if (!routes && l.triangles.count) { uniforms(triangles, view, ratio); gl.bindVertexArray(l.triangles.vao); gl.drawArrays(gl.TRIANGLES, 0, l.triangles.count); }
        if (!routes && l.boxes.count) { uniforms(boxes, view, ratio); gl.bindVertexArray(l.boxes.vao); gl.drawArraysInstanced(gl.TRIANGLE_STRIP, 0, 4, l.boxes.count); }
        if (l.segments.count) { uniforms(segments, view, ratio); gl.bindVertexArray(l.segments.vao); gl.drawArraysInstanced(gl.TRIANGLE_STRIP, 0, 4, l.segments.count); }
        if (routes) {
          if (l.triangles.count) { gl.depthFunc(gl.LEQUAL); uniforms(triangles, view, ratio); gl.bindVertexArray(l.triangles.vao); gl.drawArrays(gl.TRIANGLES, 0, l.triangles.count); }
          gl.disable(gl.DEPTH_TEST);
          if (l.boxes.count) { uniforms(boxes, view, ratio); gl.bindVertexArray(l.boxes.vao); gl.drawArraysInstanced(gl.TRIANGLE_STRIP, 0, 4, l.boxes.count); }
        }
      });
      gl.bindVertexArray(null);
    },
    destroy() {
      release();
      for (const p of [boxes, segments, triangles]) gl.deleteProgram(p.p);
      gl.deleteBuffer(corners);
    },
  };
  return rendererShell(canvas, scene, options, backend, 'webgl2');
}
