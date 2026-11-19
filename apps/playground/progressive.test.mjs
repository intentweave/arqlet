import {test} from 'node:test';
import assert from 'node:assert/strict';
import {coarseGraph, bloomStart} from './progressive.js';

// Two directories with three files each, every file holding four symbols; edges between symbols of different files.
function tree() {
  const nodes = [], edges = [];
  for (const d of ['d0', 'd1']) {
    nodes.push({id: d, kind: 'directory', label: d, parentId: null});
    for (let f = 0; f < 3; f++) {
      const file = `${d}/f${f}`;
      nodes.push({id: file, kind: 'file', label: file, parentId: d});
      for (let s = 0; s < 4; s++) nodes.push({id: `${file}/s${s}`, kind: 'symbol', label: 's', parentId: file});
    }
  }
  edges.push({id: 'a', kind: 'call', source: 'd0/f0/s0', target: 'd1/f2/s3'});
  edges.push({id: 'b', kind: 'call', source: 'd0/f0/s1', target: 'd1/f2/s0'});
  edges.push({id: 'c', kind: 'call', source: 'd0/f1/s0', target: 'd0/f1/s1'});
  return {nodes, edges};
}

test('the rough graph keeps the top levels, a cut container stands for its leaves, edges are lifted once', () => {
  const g = tree();
  assert.equal(coarseGraph(g, {limit: 100}), null, 'fits already');
  const rough = coarseGraph(g, {limit: 10, radius: 4});
  assert.equal(rough.cut, 1);
  assert.deepEqual(rough.graph.nodes.map(n => n.id), ['d0', 'd0/f0', 'd0/f1', 'd0/f2', 'd1', 'd1/f0', 'd1/f1', 'd1/f2']);
  // A file holds four symbols.
  assert.equal(rough.radii['d0/f0'], 4 * 2 * 1.3);
  // a and b both run d0/f0 → d1/f2: one edge; c lies inside d0/f1: none.
  assert.deepEqual(rough.graph.edges.map(e => [e.source, e.target, e.weight]), [['d0/f0', 'd1/f2', 2]]);
  // A flat graph has nothing to cut.
  const flat = {nodes: Array.from({length: 20}, (_, i) => ({id: `n${i}`, kind: 'k', label: 'n'})), edges: []};
  assert.equal(coarseGraph(flat, {limit: 10}), null);
});

test('blooming starts every node at its rough ancestor', () => {
  const full = {ids: ['d0', 'd0/f0', 'd0/f0/s0', 'x'], parent: [-1, 0, 1, -1], x: [0, 1, 2, 9], y: [0, 1, 2, 9], r: [10, 5, 1, 1]};
  const rough = {ids: ['d0', 'd0/f0'], x: [100, 110], y: [200, 210], r: [30, 8]};
  const start = bloomStart(full, rough);
  assert.deepEqual(start.x, [100, 110, 110, 9]);
  assert.deepEqual(start.y, [200, 210, 210, 9]);
  // A rough node keeps its rough disc; what it holds starts small inside; a node without a rough ancestor stays.
  assert.deepEqual(start.r, [30, 8, 0.25, 1]);
});

test('the rough graph keeps only the strongest lifted edges', () => {
  // 40 files under one directory each with one symbol, every symbol calling every other: 1560 pairs, at most 2 per node.
  const nodes = [{id: 'd', kind: 'directory', label: 'd', parentId: null}], edges = [];
  for (let f = 0; f < 40; f++) {
    nodes.push({id: `f${f}`, kind: 'file', label: 'f', parentId: 'd'});
    nodes.push({id: `f${f}/s`, kind: 'symbol', label: 's', parentId: `f${f}`});
  }
  for (let a = 0; a < 40; a++) for (let b = 0; b < 40; b++) if (a !== b) edges.push({id: `${a}-${b}`, kind: 'call', source: `f${a}/s`, target: `f${b}/s`});
  // One pair is stronger.
  edges.push({id: 'extra', kind: 'call', source: 'f3/s', target: 'f7/s'});
  const rough = coarseGraph({nodes, edges}, {limit: 50});
  assert.equal(rough.graph.edges.length, 2 * rough.graph.nodes.length);
  assert.ok(rough.graph.edges.some(e => e.source === 'f3' && e.target === 'f7' && e.weight === 2));
});
