import test from 'node:test';
import assert from 'node:assert/strict';
import {withAggregatedWeights} from './aggregation.js';

const source = {nodes: [{id: 'C'}, {id: 'c1', parentId: 'C'}, {id: 'c2', parentId: 'C'}, {id: 'x'}, {id: 'y'}],
 edges: [{id: 'e1', source: 'c1', target: 'x'}, {id: 'e2', source: 'c2', target: 'x'}, {id: 'e3', source: 'x', target: 'c1'},
  {id: 'e4', source: 'x', target: 'y'}, {id: 'e5', source: 'x', target: 'y'}]};
const scene = {routes: ['e1', 'e3', 'e4', 'e5'].map(id => ({id, points: [], weight: 1}))};

test('a drawn edge of a collapsed container carries the count of the edges it stands for', () => {
 const out = withAggregatedWeights(scene, {sourceGraph: source, collapsedIds: ['C'], options: {visibleNodeBudget: 3}});
 const by = Object.fromEntries(out.routes.map(r => [r.id, r]));
 assert.equal(by.e1.weight, 2);
 assert.deepEqual(by.e1.members, ['e2']);
 // One edge in the other direction, and parallel edges between visible nodes, are drawn as they are.
 assert.equal(by.e3.weight, 1);
 assert.equal(by.e4.weight, 1);
 assert.equal(by.e5.weight, 1);
});

test('without collapsed containers, or without a node budget (each edge has its own route), the scene is unchanged', () => {
 assert.equal(withAggregatedWeights(scene, {sourceGraph: source, collapsedIds: [], options: {visibleNodeBudget: 3}}), scene);
 assert.equal(withAggregatedWeights(scene, {sourceGraph: source, collapsedIds: ['C'], options: {}}), scene);
});
