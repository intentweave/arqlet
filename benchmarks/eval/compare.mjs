#!/usr/bin/env node
// Parity and quality comparison: Arqlet against elkjs, elk-rs and dagre on the same view.
//
// Every engine receives the same graph: Arqlet's bounded view of each dataset (its connected
// part, up to LIMIT nodes). Every output is converted into one geometry format, and one metric
// implementation scores all of them. Node sizes are 180x54 for leaves, as in Arqlet; containers
// are sized by each engine. Direction is mapped from the view's default direction; per-container
// directions are not mapped, because only Arqlet supports them.
//
// Usage: node compare.mjs [--limit 150] [--flat-force] [--out FILE.jsonl] [--svg DIR] [DATASET ...]
// `--svg DIR` writes DIR/<dataset>.html with every engine's drawing side by side (dagre left out).
// `--flat-force` removes parent nesting and adds Cytoscape CoSE/breadthfirst and d3-force.
// `--inprocess-core` compares Arqlet's core layout and ELK's layout() on the same preselected graph.
// Requires: cargo build --release -p arqlet-runtime --bins (provides layout-command).
// `--inprocess-core` additionally needs `--bin bench --features benchmark-timings`.
import {readFileSync, writeFileSync, mkdirSync, readdirSync, existsSync, rmSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {arch, cpus, loadavg, platform, tmpdir} from 'node:os';
import {join} from 'node:path';
import {createHash, randomUUID} from 'node:crypto';
import ELK from 'elkjs';
import ElkRs from '@openedges/elk-layout';
import dagre from '@dagrejs/dagre';
import cytoscape from 'cytoscape';
import {forceCenter, forceCollide, forceLink, forceManyBody, forceSimulation} from 'd3-force';
import {LAYOUT_COMMAND, LEAF, ROOT, arqlet, normalizeArqlet, metrics, round} from './lib.mjs';
import {writeComparisonPage} from './svg.mjs';
import {elkLayout as adaptedElkLayout} from './elk-adapter.mjs';


const args = process.argv.slice(2);
const option = (name, fallback) => {
  const i = args.indexOf(name);
  return i >= 0 ? args.splice(i, 2)[1] : fallback;
};
const LIMIT = Number(option('--limit', 150));
// Each engine runs REPEAT times per dataset. The metrics come from the first run; the time is the median of the runs.
const REPEAT = Number(option('--repeat', 1));
// `--uniform-direction`: Arqlet lays out every container in the view's default direction and layered, as the other
// engines do (they get no per-container directions or strategies), so that all engines solve the same task.
const UNIFORM = args.includes('--uniform-direction') && (args.splice(args.indexOf('--uniform-direction'), 1), true);
// Force-directed engines do not implement nested containment or layered ranking. This mode flattens each
// bounded input graph and adds Cytoscape and d3-force as a separate, explicitly non-hierarchical comparison.
const FLAT_FORCE = args.includes('--flat-force') && (args.splice(args.indexOf('--flat-force'), 1), true);
// Measures Arqlet's core layout and ELK's layout() calls on the exact same preselected graph.
const INPROCESS_CORE = args.includes('--inprocess-core') && (args.splice(args.indexOf('--inprocess-core'), 1), true);
const BENCH_COMMAND = process.env.BENCH_COMMAND ?? `${ROOT}target/release/bench`;
const SVG = option('--svg', null);
const OUT = option('--out', `${ROOT}benchmarks/results/eval-${new Date().toISOString().replace(/[:.]/g, '-')}.jsonl`);
const selected = args.length ? args : readdirSync(`${ROOT}benchmarks/datasets`).filter(f => f.endsWith('.json') && !/^synthetic-20k|intentweave/.test(f)).map(f => f.slice(0, -5));

if (!existsSync(LAYOUT_COMMAND)) {
  console.error(`Missing ${LAYOUT_COMMAND}. Run: cargo build --release -p arqlet-runtime --bins`);
  process.exit(1);
}
if (INPROCESS_CORE && !existsSync(BENCH_COMMAND)) {
  console.error(`Missing ${BENCH_COMMAND}. Run: cargo build --release -p arqlet-runtime --bin bench --features benchmark-timings --locked`);
  process.exit(1);
}

// ---------- Arqlet outputs for one graph ----------
// The default engine, and the experimental layered construction (option `layeredConstruction`).
const ARQLET_VARIANTS = [
  ['arqlet', {}],
  ['arqlet layered', {layeredConstruction: true}],
  ['arqlet hierarchical', {layeredConstruction: true, hierarchicalOrdering: true}],
];
function arqletOutputs(graph) {
  let view = null;
  const outputs = [];
  for (const [label, options] of ARQLET_VARIANTS) {
    const imported = arqlet({op: 'import', graph, limit: LIMIT, options});
    const snapshot = imported.snapshot;
    const refined = arqlet({
      op: 'refine', snapshot, expectedRevision: snapshot.revision,
      expectedLayoutVersion: snapshot.layoutVersion ?? 0, threads: 1,
    });
    view ??= snapshot.graph;
    outputs.push(
      {engine: `${label} (initial)`, ms: imported.ms, importMs: imported.ms, out: normalizeArqlet(snapshot)},
      {
        engine: `${label} (refined)`,
        ms: imported.ms + refined.ms,
        importMs: imported.ms,
        refineMs: refined.ms,
        out: normalizeArqlet(refined.snapshot),
      },
    );
  }
  return {view, outputs};
}

// ---------- elkjs (layered, orthogonal routing, hierarchy included) ----------
const elk = new ELK();
// elk-rs (a Rust port of ELK, the same JSON interface; native addon on this platform), with the same options as elkjs.
const elkRs = new ElkRs();
const DAGRE_DIRECTION = {lr: 'LR', tb: 'TB', hybrid: 'LR'};
const D3_TICKS = 300;
const COSE_ITERATIONS = 1000;
const FORCE_SEED = 42;

async function elkLayout(view, engine = elk) {
  return adaptedElkLayout(view, engine);
}

function preselectView(graph) {
  const imported = arqlet({
    op: 'import',
    graph,
    limit: LIMIT,
    options: {layeredConstruction: true, hierarchicalOrdering: true},
  });
  return {view: imported.snapshot.graph, referenceImportMs: imported.ms};
}

function arqletCoreLayout(view, name) {
  const graphPath = join(tmpdir(), `arqlet-eval-${process.pid}-${randomUUID()}.json`);
  try {
    writeFileSync(graphPath, JSON.stringify(view));
    const output = execFileSync(BENCH_COMMAND, [
      '--graph', graphPath,
      '--name', name,
      '--reps', '1',
      '--layout-only', '1',
      '--options', JSON.stringify({layeredConstruction: true, hierarchicalOrdering: true}),
    ], {cwd: ROOT, encoding: 'utf8', maxBuffer: 256 * 1024 * 1024});
    const result = JSON.parse(output);
    return {
      engine: 'arqlet (in-process core)',
      ms: result.layoutMs.medianMs,
      layoutMs: result.layoutMs.medianMs,
      out: normalizeArqlet(result.snapshot),
    };
  } finally {
    rmSync(graphPath, {force: true});
  }
}

// ---------- dagre (compound graph) ----------
// Compound mode is dagre's hierarchy support. Some inputs make it throw inside dagre; `compound: false`
// lays out the same nodes and edges flat, so containers are only sized, not nested.
function dagreLayout(view, {compound = true} = {}) {
  const containers = new Set(view.nodes.map(n => n.parentId).filter(Boolean));
  const g = new dagre.graphlib.Graph({compound, multigraph: true});
  g.setGraph({rankdir: DAGRE_DIRECTION[view.defaultDirection]});
  g.setDefaultEdgeLabel(() => ({}));
  for (const n of view.nodes) {
    g.setNode(n.id, containers.has(n.id) ? {} : {width: LEAF.width, height: LEAF.height});
  }
  if (compound) for (const n of view.nodes) if (n.parentId) g.setParent(n.id, n.parentId);
  // dagre's compound mode fails on edges that end at a container, so those edges are left out of the compound layout and
  // counted in the row. The flat mode leaves them out too, because a container has no geometry there.
  const inputEdges = view.edges.filter(e => e.target !== null && (compound || (!containers.has(e.source) && !containers.has(e.target))));
  const dropped = view.edges.filter(e => e.target !== null).length - inputEdges.length;
  for (const e of inputEdges) g.setEdge(e.source, e.target, {}, e.id);
  const started = performance.now();
  dagre.layout(g);
  const ms = performance.now() - started;
  // Flat mode gives containers no geometry, so only leaves and edges between leaves are drawn.
  const drawn = compound ? view.nodes : view.nodes.filter(n => !containers.has(n.id));
  const nodes = drawn.map(n => {
    const d = g.node(n.id);
    // The flat mode draws no containers, so no drawn node has a parent.
    return {id: n.id, parentId: compound ? (n.parentId ?? null) : null, container: containers.has(n.id), x: d.x - d.width / 2, y: d.y - d.height / 2, w: d.width, h: d.height};
  });
  const edges = inputEdges.map(e => {
    const d = g.edge({v: e.source, w: e.target, name: e.id});
    return {id: e.id, source: e.source, target: e.target, points: d?.points ?? []};
  });
  return {nodes, edges, ms, dropped};
}

function seededRandom(seed) {
  let state = seed >>> 0;
  return () => {
    state = (1664525 * state + 1013904223) >>> 0;
    return state / 0x100000000;
  };
}

function initialPosition(index, count) {
  const radius = Math.max(LEAF.width, LEAF.height) * Math.sqrt(count) * 1.5;
  const angle = (2 * Math.PI * index) / Math.max(count, 1);
  return {x: radius * Math.cos(angle), y: radius * Math.sin(angle)};
}

function flatForceEdges(view) {
  return view.edges.filter(e => e.target !== null);
}

function forceRoutes(view, positions) {
  const loopOrdinals = new Map();
  return flatForceEdges(view).map(edge => {
    const source = positions.get(edge.source);
    const target = positions.get(edge.target);
    if (edge.source !== edge.target) {
      return {id: edge.id, source: edge.source, target: edge.target, points: [source, target]};
    }
    const ordinal = loopOrdinals.get(edge.source) ?? 0;
    loopOrdinals.set(edge.source, ordinal + 1);
    const right = source.x + LEAF.width / 2;
    const offset = ordinal * 8;
    return {
      id: edge.id, source: edge.source, target: edge.target,
      points: [
        {x: right, y: source.y - 18},
        {x: right + 30 + offset, y: source.y - 18},
        {x: right + 30 + offset, y: source.y + 18},
        {x: right, y: source.y + 18},
      ],
    };
  });
}

function cytoscapeLayout(view, name) {
  const validEdges = flatForceEdges(view);
  const cy = cytoscape({
    headless: true,
    styleEnabled: true,
    elements: [
      ...view.nodes.map((node, index) => ({
        data: {id: node.id},
        position: initialPosition(index, view.nodes.length),
      })),
      ...validEdges.map(edge => ({
        data: {id: edge.id, source: edge.source, target: edge.target},
      })),
    ],
    style: [{selector: 'node', style: {width: LEAF.width, height: LEAF.height}}],
  });
  const started = performance.now();
  cy.layout(name === 'cose'
    ? {
      name, animate: false, fit: false, randomize: false, numIter: COSE_ITERATIONS,
      nodeRepulsion: 4500, idealEdgeLength: 200, edgeElasticity: 100, gravity: 0.25,
    }
    : {
      name, animate: false, fit: false, directed: true, circle: false, avoidOverlap: true, spacingFactor: 1.75,
    }).run();
  const ms = performance.now() - started;
  const positions = new Map(cy.nodes().map(node => [node.id(), node.position()]));
  const nodes = view.nodes.map(node => {
    const p = positions.get(node.id);
    return {id: node.id, parentId: null, container: false, x: p.x - LEAF.width / 2, y: p.y - LEAF.height / 2, w: LEAF.width, h: LEAF.height};
  });
  const edges = forceRoutes(view, positions);
  cy.destroy();
  return {nodes, edges, ms};
}

function d3ForceLayout(view) {
  const nodes = view.nodes.map((node, index) => ({id: node.id, ...initialPosition(index, view.nodes.length)}));
  const edges = flatForceEdges(view).map(edge => ({id: edge.id, source: edge.source, target: edge.target}));
  const started = performance.now();
  const simulation = forceSimulation(nodes)
    .randomSource(seededRandom(FORCE_SEED))
    .force('link', forceLink(edges).id(node => node.id).distance(200).strength(0.15))
    .force('charge', forceManyBody().strength(-900))
    .force('collide', forceCollide(Math.hypot(LEAF.width, LEAF.height) / 2 + 12).iterations(2))
    .force('center', forceCenter(0, 0))
    .stop();
  simulation.tick(D3_TICKS);
  const ms = performance.now() - started;
  const positions = new Map(nodes.map(node => [node.id, {x: node.x, y: node.y}]));
  const outputNodes = nodes.map(node => ({
    id: node.id, parentId: null, container: false,
    x: node.x - LEAF.width / 2, y: node.y - LEAF.height / 2, w: LEAF.width, h: LEAF.height,
  }));
  const outputEdges = forceRoutes(view, positions);
  return {nodes: outputNodes, edges: outputEdges, ms};
}

// ---------- Run ----------
mkdirSync(OUT.slice(0, OUT.lastIndexOf('/')), {recursive: true});
const environment = {
  cpuModel: cpus()[0]?.model, logicalCpus: cpus().length, arch, os: platform(), loadAverage: loadavg(),
  node: process.version, layoutCommand: 'release',
  gitRevision: execFileSync('git', ['rev-parse', '--short', 'HEAD'], {cwd: ROOT, encoding: 'utf8'}).trim(),
  gitDirty: execFileSync('git', ['status', '--porcelain'], {cwd: ROOT, encoding: 'utf8'}).trim().length > 0,
};
const runConfig = {
  limit: LIMIT, repeat: REPEAT, uniformDirection: UNIFORM, flatForce: FLAT_FORCE,
  inprocessCore: INPROCESS_CORE,
  forceSeed: FLAT_FORCE ? FORCE_SEED : null, d3Ticks: FLAT_FORCE ? D3_TICKS : null,
  cytoscapeCoseIterations: FLAT_FORCE ? COSE_ITERATIONS : null,
  engineVersions: {elkjs: '0.12.0', elkRs: '0.12.1', cytoscape: FLAT_FORCE ? '3.34.4' : null, d3Force: FLAT_FORCE ? '3.0.1' : null},
};
const rows = [];
// One pass over the engines of a view. A failed engine is kept as a finding: it cannot lay out this input in this mode.
async function runEngines(view, outputs, name) {
  const engines = INPROCESS_CORE ? [arqletCoreLayout(view, name)] : [...outputs];
  engines.push(await elkLayout(view).then(r => ({engine: 'elkjs layered', ...r})));
  try {
    engines.push(await elkLayout(view, elkRs).then(r => ({engine: 'elk-rs layered', ...r})));
  } catch (error) {
    engines.push({engine: 'elk-rs layered', failed: error.message ?? String(error)});
  }
  if (FLAT_FORCE) {
    for (const [engine, layout] of [
      ['cytoscape cose (flat)', 'cose'],
      ['cytoscape breadthfirst (flat)', 'breadthfirst'],
    ]) {
      try {
        engines.push({engine, ...cytoscapeLayout(view, layout)});
      } catch (error) {
        engines.push({engine, failed: error.message ?? String(error)});
      }
    }
    try {
      engines.push({engine: 'd3-force (flat)', ...d3ForceLayout(view)});
    } catch (error) {
      engines.push({engine: 'd3-force (flat)', failed: error.message ?? String(error)});
    }
  }
  for (const [engine, options] of [['dagre', {}], ['dagre (flat)', {compound: false}]]) {
    try {
      engines.push({engine, ...dagreLayout(view, options)});
    } catch (error) {
      engines.push({engine, failed: error.message});
    }
  }
  return engines;
}
const median = values => {
  const sorted = [...values].sort((a, b) => a - b);
  const mid = Math.floor(sorted.length / 2);
  return sorted.length % 2 ? sorted[mid] : (sorted[mid - 1] + sorted[mid]) / 2;
};

for (const name of selected) {
  let graph = JSON.parse(readFileSync(`${ROOT}benchmarks/datasets/${name}.json`, 'utf8'));
  if (FLAT_FORCE) {
    graph = {
      ...graph,
      nodes: graph.nodes.map(node => ({...node, parentId: null})),
      containerDirections: {},
      containerStrategies: {},
    };
  }
  if (UNIFORM) {
    graph.containerDirections = {};
    graph.containerStrategies = {};
    graph.defaultStrategy = 'none';
  }
  const {view, outputs, referenceImportMs} = INPROCESS_CORE
    ? {...preselectView(graph), outputs: []}
    : arqletOutputs(graph);
  const engines = await runEngines(view, outputs, name);
  if (REPEAT > 1) {
    const times = new Map(engines.map(e => [e.engine, e.failed ? null : [e.ms]]));
    const adapterTimes = new Map(engines.map(e => [
      e.engine,
      e.failed || e.prepareMs === undefined ? null : {
        prepareMs: [e.prepareMs],
        normalizeMs: [e.normalizeMs],
        totalMs: [e.totalMs],
      },
    ]));
    const componentTimes = new Map(engines.map(e => [
      e.engine,
      e.failed || e.importMs === undefined ? null : {
        importMs: [e.importMs],
        ...(e.refineMs === undefined ? {} : {refineMs: [e.refineMs]}),
      },
    ]));
    for (let i = 1; i < REPEAT; i++) {
      const againOutputs = INPROCESS_CORE ? [] : arqletOutputs(graph).outputs;
      const again = await runEngines(view, againOutputs, name);
      for (const e of again) {
        if (e.failed || !times.get(e.engine)) continue;
        times.get(e.engine).push(e.ms);
        const adapter = adapterTimes.get(e.engine);
        if (adapter) {
          adapter.prepareMs.push(e.prepareMs);
          adapter.normalizeMs.push(e.normalizeMs);
          adapter.totalMs.push(e.totalMs);
        }
        const components = componentTimes.get(e.engine);
        if (components) {
          components.importMs.push(e.importMs);
          if (components.refineMs) components.refineMs.push(e.refineMs);
        }
      }
    }
    for (const e of engines) {
      if (e.failed || !times.get(e.engine)) continue;
      e.ms = median(times.get(e.engine));
      const adapter = adapterTimes.get(e.engine);
      if (adapter) {
        e.prepareMs = median(adapter.prepareMs);
        e.normalizeMs = median(adapter.normalizeMs);
        e.totalMs = median(adapter.totalMs);
      }
      const components = componentTimes.get(e.engine);
      if (components) {
        e.importMs = median(components.importMs);
        if (components.refineMs) e.refineMs = median(components.refineMs);
      }
    }
  }
  const panels = [];
  for (const e of engines) {
    if (e.failed) {
      const row = {dataset: name, engine: e.engine, viewNodes: view.nodes.length, viewEdges: view.edges.filter(x => x.target !== null).length, failed: e.failed, environment, runConfig};
      rows.push(row);
      console.log(`${name.padEnd(22)} ${e.engine.padEnd(24)} FAILED: ${e.failed}`);
      continue;
    }
    const out = e.out ?? {nodes: e.nodes, edges: e.edges};
    // A metric error is this engine's failure, recorded as such; the other engines of the dataset still run.
    let scored;
    try {
      scored = metrics(out);
    } catch (error) {
      const row = {dataset: name, engine: e.engine, viewNodes: view.nodes.length, viewEdges: view.edges.filter(x => x.target !== null).length, failed: `metrics: ${error.message}`, environment, runConfig};
      rows.push(row);
      console.log(`${name.padEnd(22)} ${e.engine.padEnd(24)} FAILED: metrics: ${error.message}`);
      continue;
    }
    const row = {
      dataset: name, engine: e.engine, viewNodes: view.nodes.length, viewEdges: view.edges.filter(x => x.target !== null).length,
      layoutMs: round(e.ms), droppedEdges: e.dropped ?? 0, ...scored, environment,
      runConfig,
      inputSha256: createHash('sha256').update(JSON.stringify(view)).digest('hex'),
      ...(referenceImportMs === undefined ? {} : {referenceImportMs: round(referenceImportMs)}),
      ...(e.prepareMs === undefined ? {} : {adapterPrepareMs: round(e.prepareMs)}),
      ...(e.normalizeMs === undefined ? {} : {adapterNormalizeMs: round(e.normalizeMs)}),
      ...(e.totalMs === undefined ? {} : {adapterTotalMs: round(e.totalMs)}),
      ...(e.importMs === undefined ? {} : {importMs: round(e.importMs)}),
      ...(e.refineMs === undefined ? {} : {refineMs: round(e.refineMs)}),
    };
    rows.push(row);
    if (!e.engine.startsWith('dagre') && !e.engine.endsWith('(initial)')) panels.push({engine: e.engine, out, row});
    console.log(`${name.padEnd(22)} ${e.engine.padEnd(24)} crossings ${String(row.crossings).padStart(4)}  line-node ${String(row.lineNodeHits).padStart(4)}  overlaps ${row.siblingOverlaps + row.crossBranchOverlaps}  shared ${row.sharedLinePairs}  hugging ${row.hugPairs}  bends ${row.bends}  area ${row.area}  length ${row.length}  ${row.layoutMs} ms`);
  }
  if (SVG) writeComparisonPage(SVG, name, view, panels);
}
writeFileSync(OUT, rows.map(r => JSON.stringify(r)).join('\n') + '\n');
console.log(`Results: ${OUT}`);
