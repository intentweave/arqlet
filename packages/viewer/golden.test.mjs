// Representation regression: the SVG viewer and the React Flow adapter must produce the same output
// for the same engine snapshot. Golden files are reviewed diffs; UPDATE_GOLDEN=1 writes or rewrites them.
// This is markup-level, not pixel-level; browser screenshots are a separate step.
import {test} from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync, writeFileSync, existsSync, mkdirSync} from 'node:fs';
import {renderSvg} from './svg.js';
import {toReactFlow} from './react-flow.js';

const here = new URL('.', import.meta.url);
const UPDATE = process.env.UPDATE_GOLDEN === '1';

class Element {
  constructor(tag) { this.tag = tag; this.children = []; this.attrs = {}; this.textContent = ''; }
  append(...items) { this.children.push(...items); }
  replaceChildren() { this.children = []; }
  setAttribute(k, v) { this.attrs[k] = v; }
}
const document = {createElementNS: (_, tag) => new Element(tag)};
const escape = s => String(s).replace(/&/g, '&amp;').replace(/"/g, '&quot;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
// Marker ids come from a process-wide counter; normalize them so output does not depend on test order.
const normalize = s => s.replace(/lw-arrow-\d+/g, 'lw-arrow-N');
function markup(node, indent = '') {
  if (!(node instanceof Element)) return '';
  const attrs = Object.entries(node.attrs).sort(([a], [b]) => a.localeCompare(b))
    .map(([k, v]) => ` ${k}="${escape(v)}"`).join('');
  const text = node.textContent ? escape(node.textContent) : '';
  const inner = node.children.map(c => markup(c, indent + '  ')).join('');
  if (!inner && !text) return `${indent}<${node.tag}${attrs}/>\n`;
  return `${indent}<${node.tag}${attrs}>${text}${inner ? '\n' + inner + indent : ''}</${node.tag}>\n`;
}
function canonical(value) {
  const sort = v => Array.isArray(v) ? v.map(sort) : v && typeof v === 'object'
    ? Object.fromEntries(Object.keys(v).sort().map(k => [k, sort(v[k])])) : v;
  return JSON.stringify(sort(value), null, 1);
}
function check(name, actual) {
  const path = new URL(`golden/${name}`, here);
  // A missing reference fails in check mode; it is written only after review, with UPDATE_GOLDEN=1.
  if (UPDATE) {
    mkdirSync(new URL('golden/', here), {recursive: true});
    writeFileSync(path, actual);
    return;
  }
  assert.ok(existsSync(path), `${name} has no golden file; review the output and run with UPDATE_GOLDEN=1`);
  assert.equal(actual, readFileSync(path, 'utf8'), `${name} differs from golden file; run with UPDATE_GOLDEN=1 after review`);
}

for (const name of ['state-tcp', 'bpmn-pools']) {
  test(`golden: ${name} renders identically from the snapshot`, () => {
    // Scene fixtures are engine output; a Rust test keeps them equal to the engine's build_scene.
    const scene = JSON.parse(readFileSync(new URL(`fixtures/${name}.scene.json`, here), 'utf8'));
    const svg = new Element('svg');
    renderSvg(svg, scene, {document});
    check(`${name}.svg`, normalize(markup(svg)));
    const mapped = toReactFlow(scene);
    // Sorted keys: the golden must not depend on serializer key order.
    check(`${name}.reactflow.json`, canonical({nodes: mapped.nodes, edges: mapped.edges, labels: mapped.labels}) + '\n');
    // The two consumers agree on how many drawn routes and nodes exist.
    assert.equal(svg.children.filter(e => e.tag === 'polyline').length, mapped.edges.length);
    assert.equal(mapped.nodes.length, scene.nodes.length);
  });
}
