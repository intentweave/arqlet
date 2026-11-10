#!/usr/bin/env node
// Phase ablation for dagre and elkjs, on the same views as compare.mjs.
//
// Neither engine reports its phases, so each phase is measured by switching it to a cheaper method and timing the
// whole layout again. The difference to the default is the phase's cost on that input (an approximation: a cheaper
// phase can also change the work of the later phases). Only the engine's own layout call is timed; graph construction
// is outside the timer, as in compare.mjs.
//
// Usage: node stages.mjs [--limit 150] [--repeat 3] [--out FILE.jsonl] [DATASET ...]
import {readFileSync, mkdirSync, writeFileSync} from 'node:fs';
import ELK from 'elkjs';
import dagre from '@dagrejs/dagre';
import {ROOT, LEAF, arqlet} from './lib.mjs';

const args = process.argv.slice(2);
const option = (name, fallback) => {
  const i = args.indexOf(name);
  return i >= 0 ? args.splice(i, 2)[1] : fallback;
};
const LIMIT = Number(option('--limit', 150));
const REPEAT = Number(option('--repeat', 3));
const OUT = option('--out', `${ROOT}benchmarks/results/stages-${new Date().toISOString().replace(/[:.]/g, '-')}.jsonl`);
const selected = args.length
  ? args
  : ['petri-net', 'er-diagram', 'state-order', 'activity-order', 'uml-class', 'bpmn-pools', 'network-vlan',
     'arqlet-pipeline', 'cloud-architecture', 'mixed-layouts', 'nested-tree-radial', 'tree-100',
     'organic-25', 'organic-50', 'organic-100', 'organic-200', 'layerweave-code'];

const ELK_DIRECTION = {lr: 'RIGHT', tb: 'DOWN', hybrid: 'RIGHT'};
const DAGRE_DIRECTION = {lr: 'LR', tb: 'TB', hybrid: 'LR'};
const elk = new ELK();

// The view's containers and drawn edges, as compare.mjs builds them.
function shape(view) {
  const containers = new Set(view.nodes.map(n => n.parentId).filter(Boolean));
  const drawn = view.edges.filter(e => e.target !== null);
  const toContainer = drawn.filter(e => containers.has(e.source) || containers.has(e.target)).length;
  return {
    containers,
    drawn,
    stats: {
      viewNodes: view.nodes.length,
      containers: containers.size,
      leaves: view.nodes.length - containers.size,
      drawnEdges: drawn.length,
      edgesToContainers: toContainer,
    },
  };
}

// A dagre graph for one variant. `compound` keeps the hierarchy; edges that end at a container are dropped in both modes.
function dagreGraph(view, {compound, ranker, disableOptimalOrderHeuristic}) {
  const {containers, drawn} = shape(view);
  const g = new dagre.graphlib.Graph({compound, multigraph: true});
  g.setGraph({
    rankdir: DAGRE_DIRECTION[view.defaultDirection],
    ...(ranker ? {ranker} : {}),
    ...(disableOptimalOrderHeuristic ? {disableOptimalOrderHeuristic: true} : {}),
  });
  g.setDefaultEdgeLabel(() => ({}));
  for (const n of view.nodes) g.setNode(n.id, containers.has(n.id) ? {} : {width: LEAF.width, height: LEAF.height});
  if (compound) for (const n of view.nodes) if (n.parentId) g.setParent(n.id, n.parentId);
  for (const e of drawn) {
    // Edges that end at a container are left out in both modes, as in compare.mjs.
    if (containers.has(e.source) || containers.has(e.target)) continue;
    g.setEdge(e.source, e.target, {}, e.id);
  }
  return g;
}

// An ELK graph for one variant. `overrides` replace the layout options of compare.mjs.
function elkRoot(view, overrides) {
  const {containers, drawn} = shape(view);
  const elkNodes = new Map();
  for (const n of view.nodes) {
    elkNodes.set(n.id, containers.has(n.id) ? {id: n.id, children: [], layoutOptions: {}} : {id: n.id, ...LEAF});
  }
  const root = {id: 'root', children: [], edges: []};
  for (const n of view.nodes) (n.parentId ? elkNodes.get(n.parentId).children : root.children).push(elkNodes.get(n.id));
  root.edges = drawn.map(e => ({id: e.id, sources: [e.source], targets: [e.target]}));
  root.layoutOptions = {
    'elk.algorithm': 'layered',
    'elk.direction': ELK_DIRECTION[view.defaultDirection],
    'elk.edgeRouting': 'ORTHOGONAL',
    'elk.hierarchyHandling': 'INCLUDE_CHILDREN',
    ...overrides,
  };
  return root;
}

const DAGRE_VARIANTS = [
  ['dagre flat: default', {compound: false}],
  ['dagre flat: ranker tight-tree', {compound: false, ranker: 'tight-tree'}],
  ['dagre flat: ranker longest-path', {compound: false, ranker: 'longest-path'}],
  ['dagre flat: no order heuristic', {compound: false, disableOptimalOrderHeuristic: true}],
  ['dagre compound: default', {compound: true}],
  ['dagre compound: ranker longest-path', {compound: true, ranker: 'longest-path'}],
  ['dagre compound: no order heuristic', {compound: true, disableOptimalOrderHeuristic: true}],
];
const ELK_VARIANTS = [
  ['elk: default', {}],
  ['elk: polyline routing (no orthogonal routing)', {'elk.edgeRouting': 'POLYLINE'}],
  ['elk: no crossing minimization', {'elk.layered.crossingMinimization.strategy': 'NONE'}],
  ['elk: simple node placement', {'elk.layered.nodePlacement.strategy': 'SIMPLE'}],
  ['elk: longest-path layering', {'elk.layered.layering.strategy': 'LONGEST_PATH'}],
];

const median = values => {
  const sorted = [...values].sort((a, b) => a - b);
  const mid = Math.floor(sorted.length / 2);
  return sorted.length % 2 ? sorted[mid] : (sorted[mid - 1] + sorted[mid]) / 2;
};

// Median time of the engine's own layout call over REPEAT runs, each on a fresh input. A failed run is a finding.
async function timeDagre(view, options) {
  const times = [];
  for (let i = 0; i < REPEAT; i++) {
    const g = dagreGraph(view, options);
    const started = performance.now();
    try {
      dagre.layout(g);
    } catch (error) {
      return {failed: error.message};
    }
    times.push(performance.now() - started);
  }
  return {ms: round(median(times))};
}

async function timeElk(view, overrides) {
  const times = [];
  for (let i = 0; i < REPEAT; i++) {
    const root = elkRoot(view, overrides);
    const started = performance.now();
    try {
      await elk.layout(root);
    } catch (error) {
      return {failed: error.message};
    }
    times.push(performance.now() - started);
  }
  return {ms: round(median(times))};
}

const round = v => Math.round(v * 100) / 100;

mkdirSync(OUT.slice(0, OUT.lastIndexOf('/')), {recursive: true});
const rows = [];
for (const name of selected) {
  const graph = JSON.parse(readFileSync(`${ROOT}benchmarks/datasets/${name}.json`, 'utf8'));
  const view = arqlet({op: 'import', graph, limit: LIMIT}).snapshot.graph;
  const {stats} = shape(view);
  for (const [variant, options] of DAGRE_VARIANTS) {
    rows.push({dataset: name, engine: 'dagre', variant, ...stats, ...(await timeDagre(view, options))});
  }
  for (const [variant, overrides] of ELK_VARIANTS) {
    rows.push({dataset: name, engine: 'elkjs', variant, ...stats, ...(await timeElk(view, overrides))});
  }
  console.log(`${name.padEnd(22)} done (${stats.viewNodes} nodes, ${stats.drawnEdges} edges)`);
}
writeFileSync(OUT, rows.map(r => JSON.stringify(r)).join('\n') + '\n');
console.log(`Results: ${OUT}`);
