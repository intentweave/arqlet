// Provisional (ADR 0025, step V1): the far layer's first-choice backend, WebGPU. No dependency. The same three
// pipelines as the WebGL 2 fallback (`scene-gl.js`) on the same buffers (`sceneBuffers`): boxes and segments as
// instanced quads, triangles for hulls and arrowheads; 4× multisampling; one depth per route line (LESS for the
// segments, LESS-EQUAL for the arrowheads) so that a translucent line blends once at its corners. The view, texts and
// picking are the shell's (`scene-renderer.js`).
//
// The device is asked for asynchronously (`sceneBackend` in scene-renderer.js); the renderer itself is made
// synchronously from it.
import {BOX_STRIDE, SEGMENT_STRIDE, TRIANGLE_STRIDE} from './scene-buffers.js';
import {rendererShell} from './scene-renderer.js';

const COMMON = `
struct View { centre: vec2f, scale: f32, unused: f32, size: vec2f, unused2: vec2f };
@group(0) @binding(0) var<uniform> view: View;
fn toClip(screen: vec2f, depth: f32) -> vec4f {
  let clip = screen / (view.size * 0.5);
  return vec4f(clip.x, -clip.y, depth, 1.0);
}
fn wrap(a: f32, b: f32) -> f32 { return a - b * floor(a / b); }
`;

const BOX = COMMON + `
struct Out {
  @builtin(position) position: vec4f,
  @location(0) p: vec2f, @location(1) extent: vec2f, @location(2) radius: f32, @location(3) stroke: f32,
  @location(4) kind: f32, @location(5) dash: f32, @location(6) fill: vec4f, @location(7) strokeColour: vec4f,
  @location(8) unit: f32,
};
@vertex fn vs(@location(0) corner: vec2f, @location(1) rect: vec4f, @location(2) shape: vec4f,
              @location(3) fill: vec4f, @location(4) strokeColour: vec4f) -> Out {
  var o: Out;
  let centre = (rect.xy + rect.zw * 0.5 - view.centre) * view.scale;
  // At least half a device pixel, so that a box far out stays a dot.
  let h = max(rect.zw * 0.5 * view.scale, vec2f(0.5));
  var stroke = 0.0;
  if (shape.z > 0.0) { stroke = max(shape.z * view.scale, 1.0); }
  let reach = h + stroke * 0.5 + 1.0;
  o.position = toClip(centre + corner * reach, 0.0);
  o.p = corner * reach; o.extent = h; o.radius = min(shape.x * view.scale, min(h.x, h.y));
  o.stroke = stroke; o.kind = shape.y; o.dash = shape.w; o.fill = fill; o.strokeColour = strokeColour; o.unit = view.scale;
  return o;
}
@fragment fn fs(i: Out) -> @location(0) vec4f {
  var d: f32;
  if (i.kind > 0.5) { d = length(i.p) - i.extent.x; }
  else { let q = abs(i.p) - i.extent + i.radius; d = length(max(q, vec2f(0.0))) + min(max(q.x, q.y), 0.0) - i.radius; }
  let fill = clamp(0.5 - d, 0.0, 1.0);
  var stroke = 0.0;
  if (i.stroke > 0.0) { stroke = clamp(i.stroke * 0.5 + 0.5 - abs(d), 0.0, 1.0); }
  // Dashes 6 on, 4 off (world units) along the outline.
  if (i.dash > 0.5 && stroke > 0.0) {
    var along: f32;
    if (i.kind > 0.5) { along = atan2(i.p.y, i.p.x) * i.extent.x; }
    else if (abs(i.p.x) - i.extent.x > abs(i.p.y) - i.extent.y) { along = i.p.y + i.extent.y; }
    else { along = i.p.x + i.extent.x; }
    if (wrap(along / i.unit, 10.0) > 6.0) { stroke = 0.0; }
  }
  let f = vec4f(i.fill.rgb * i.fill.a, i.fill.a) * fill;
  let s = vec4f(i.strokeColour.rgb * i.strokeColour.a, i.strokeColour.a) * stroke;
  let colour = s + f * (1.0 - s.a);
  if (colour.a <= 0.0) { discard; }
  return colour;
}`;

const SEGMENT = COMMON + `
struct Out {
  @builtin(position) position: vec4f,
  @location(0) across: f32, @location(1) halfWidth: f32, @location(2) along: f32, @location(3) dash: vec2f,
  @location(4) colour: vec4f,
};
@vertex fn vs(@location(0) corner: vec2f, @location(1) ends: vec4f, @location(2) style: vec4f,
              @location(3) startAlong: f32, @location(4) colour: vec4f) -> Out {
  var o: Out;
  let p0 = (ends.xy - view.centre) * view.scale;
  let p1 = (ends.zw - view.centre) * view.scale;
  let d = p1 - p0;
  let len = length(d);
  var dir = vec2f(1.0, 0.0);
  if (len > 0.0) { dir = d / len; }
  let normal = vec2f(-dir.y, dir.x);
  let hw = max(style.x * view.scale, 1.0) * 0.5;
  var end = p1 + dir * hw;
  var along = startAlong + (len + hw) / view.scale;
  if (corner.x < 0.0) { end = p0 - dir * hw; along = startAlong - hw / view.scale; }
  let across = corner.y * (hw + 1.0);
  o.position = toClip(end + normal * across, style.y);
  o.across = across; o.halfWidth = hw; o.along = along; o.dash = style.zw; o.colour = colour;
  return o;
}
@fragment fn fs(i: Out) -> @location(0) vec4f {
  if (i.dash.x > 0.0 && wrap(i.along, i.dash.x + i.dash.y) > i.dash.x) { discard; }
  let a = clamp(i.halfWidth + 0.5 - abs(i.across), 0.0, 1.0) * i.colour.a;
  if (a <= 0.0) { discard; }
  return vec4f(i.colour.rgb * a, a);
}`;

const TRIANGLE = COMMON + `
struct Out { @builtin(position) position: vec4f, @location(0) colour: vec4f };
@vertex fn vs(@location(0) p: vec3f, @location(1) colour: vec4f) -> Out {
  var o: Out;
  o.position = toClip((p.xy - view.centre) * view.scale, p.z);
  o.colour = colour;
  return o;
}
@fragment fn fs(i: Out) -> @location(0) vec4f { return vec4f(i.colour.rgb * i.colour.a, i.colour.a); }`;

const attrs = (list, stride, stepMode) => ({arrayStride: stride * 4, stepMode, attributes: list.map(([loc, size, offset]) => ({shaderLocation: loc, offset: offset * 4, format: size === 1 ? 'float32' : `float32x${size}`}))});
const CORNERS = attrs([[0, 2, 0]], 2, 'vertex');
const BOX_LAYOUT = [CORNERS, attrs([[1, 4, 0], [2, 4, 4], [3, 4, 8], [4, 4, 12]], BOX_STRIDE, 'instance')];
const SEGMENT_LAYOUT = [CORNERS, attrs([[1, 4, 0], [2, 4, 4], [3, 1, 8], [4, 4, 9]], SEGMENT_STRIDE, 'instance')];
const TRIANGLE_LAYOUT = [attrs([[0, 3, 0], [1, 4, 3]], TRIANGLE_STRIDE, 'vertex')];
const SAMPLES = 4;

/** Whether this browser offers WebGPU (the device itself is asked for by `sceneBackend`). */
export const webgpuAvailable = () => !!globalThis.navigator?.gpu;

/**
 * Draws a scene (or its columns) on `canvas` with WebGPU on `options.device` (from `sceneBackend()` or
 * `adapter.requestDevice()`). The same view, options and methods as the WebGL 2 renderer.
 */
export function createSceneGpuRenderer(canvas, scene, options = {}) {
  const device = options.device;
  if (!device) throw Error('createSceneGpuRenderer needs a WebGPU device');
  const context = canvas.getContext('webgpu');
  if (!context) throw Error('WebGPU is not available on this canvas');
  const format = navigator.gpu.getPreferredCanvasFormat();
  context.configure({device, format, alphaMode: 'premultiplied'});
  const background = options.background ?? [0, 0, 0, 0];
  const uniform = device.createBuffer({size: 32, usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST});
  const groupLayout = device.createBindGroupLayout({entries: [{binding: 0, visibility: GPUShaderStage.VERTEX, buffer: {}}]});
  const group = device.createBindGroup({layout: groupLayout, entries: [{binding: 0, resource: {buffer: uniform}}]});
  const layout = device.createPipelineLayout({bindGroupLayouts: [groupLayout]});
  const blend = {color: {srcFactor: 'one', dstFactor: 'one-minus-src-alpha'}, alpha: {srcFactor: 'one', dstFactor: 'one-minus-src-alpha'}};
  const modules = {box: device.createShaderModule({code: BOX}), segment: device.createShaderModule({code: SEGMENT}), triangle: device.createShaderModule({code: TRIANGLE})};
  const pipeline = (module, buffers, topology, depthCompare = 'always', depthWriteEnabled = false) => device.createRenderPipeline({
    layout,
    vertex: {module, entryPoint: 'vs', buffers},
    fragment: {module, entryPoint: 'fs', targets: [{format, blend}]},
    primitive: {topology},
    depthStencil: {format: 'depth24plus', depthCompare, depthWriteEnabled},
    multisample: {count: SAMPLES},
  });
  const pipelines = {
    box: pipeline(modules.box, BOX_LAYOUT, 'triangle-strip'),
    segment: pipeline(modules.segment, SEGMENT_LAYOUT, 'triangle-strip'),
    triangle: pipeline(modules.triangle, TRIANGLE_LAYOUT, 'triangle-list'),
    routeSegment: pipeline(modules.segment, SEGMENT_LAYOUT, 'triangle-strip', 'less', true),
    routeArrow: pipeline(modules.triangle, TRIANGLE_LAYOUT, 'triangle-list', 'less-equal', true),
  };
  const corners = device.createBuffer({size: 32, usage: GPUBufferUsage.VERTEX | GPUBufferUsage.COPY_DST});
  device.queue.writeBuffer(corners, 0, new Float32Array([-1, -1, 1, -1, -1, 1, 1, 1]));

  let layers = [], targets = null;
  const upload = (data, stride) => {
    if (!data.length) return {buffer: null, count: 0};
    const buffer = device.createBuffer({size: Math.ceil(data.byteLength / 4) * 4, usage: GPUBufferUsage.VERTEX | GPUBufferUsage.COPY_DST});
    device.queue.writeBuffer(buffer, 0, data);
    return {buffer, count: data.length / stride};
  };
  const release = () => { for (const l of layers) for (const part of [l.boxes, l.segments, l.triangles]) part.buffer?.destroy(); layers = []; };
  // The multisampled colour and the depth targets, made again when the canvas changes size.
  const targetsFor = (width, height) => {
    if (targets && targets.width === width && targets.height === height) return targets;
    targets?.colour.destroy(); targets?.depth.destroy();
    const colour = device.createTexture({size: [width, height], format, sampleCount: SAMPLES, usage: GPUTextureUsage.RENDER_ATTACHMENT});
    const depth = device.createTexture({size: [width, height], format: 'depth24plus', sampleCount: SAMPLES, usage: GPUTextureUsage.RENDER_ATTACHMENT});
    targets = {width, height, colour, depth, colourView: colour.createView(), depthView: depth.createView()};
    return targets;
  };

  const backend = {
    upload(built) {
      release();
      layers = built.layers.map(l => ({boxes: upload(l.boxes, BOX_STRIDE), segments: upload(l.segments, SEGMENT_STRIDE), triangles: upload(l.triangles, TRIANGLE_STRIDE)}));
    },
    draw(view, ratio) {
      const t = targetsFor(canvas.width, canvas.height);
      device.queue.writeBuffer(uniform, 0, new Float32Array([view.centreX, view.centreY, view.scale * ratio, 0, canvas.width, canvas.height, 0, 0]));
      const encoder = device.createCommandEncoder();
      const pass = encoder.beginRenderPass({
        colorAttachments: [{view: t.colourView, resolveTarget: context.getCurrentTexture().createView(), loadOp: 'clear', storeOp: 'discard',
          clearValue: {r: background[0] * background[3], g: background[1] * background[3], b: background[2] * background[3], a: background[3]}}],
        depthStencilAttachment: {view: t.depthView, depthClearValue: 1, depthLoadOp: 'clear', depthStoreOp: 'discard'},
      });
      pass.setBindGroup(0, group);
      const quads = (p, part) => { if (!part.count) return; pass.setPipeline(p); pass.setVertexBuffer(0, corners); pass.setVertexBuffer(1, part.buffer); pass.draw(4, part.count); };
      const tris = (p, part) => { if (!part.count) return; pass.setPipeline(p); pass.setVertexBuffer(0, part.buffer); pass.draw(part.count); };
      layers.forEach((l, i) => {
        if (i === 2) {
          quads(pipelines.routeSegment, l.segments);
          tris(pipelines.routeArrow, l.triangles);
          quads(pipelines.box, l.boxes);
        } else {
          tris(pipelines.triangle, l.triangles);
          quads(pipelines.box, l.boxes);
          quads(pipelines.segment, l.segments);
        }
      });
      pass.end();
      device.queue.submit([encoder.finish()]);
    },
    destroy() {
      release();
      targets?.colour.destroy(); targets?.depth.destroy();
      uniform.destroy(); corners.destroy();
      context.unconfigure();
    },
  };
  return rendererShell(canvas, scene, options, backend, 'webgpu');
}
