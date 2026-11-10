#!/usr/bin/env node
// Option grid: measures the benefit and the cost of each configuration variant on the same datasets,
// so the default strategy can be chosen from numbers rather than from intuition.
//
// Each variant is applied through the same operations the browser uses: import, then `configure`
// (options patch, full recompute), then `refine`. Every result is scored with the metric
// implementation shared with compare.mjs. Bundled members are not drawn and do not count.
//
// Usage: node grid.mjs [--limit 150] [--out FILE.jsonl] [VARIANT ...]
import {readFileSync, writeFileSync, mkdirSync, readdirSync} from 'node:fs';
import {ROOT, arqlet, normalizeArqlet, metrics, round} from './lib.mjs';

const ORDER = ['siblingOverlaps', 'edgeNodeHits', 'sharedRoutePairs', 'nearNodeSegments', 'routedCrossings', 'projectedCrossings', 'bends'];

// Variant: options patch for `configure`, plus an optional default-direction override for the whole view.
const VARIANTS = {
  'default': {options: {}},
  'fast (one ordering)': {options: {preset: 'fast'}},
  'no routing candidate': {options: {routingCandidate: false}},
  'bundling complete': {options: {bundling: 'complete'}},
  'end-to-end as second-order': {options: {objectives: [...ORDER, 'endToEnd', 'cost']}},
  'direction lr': {options: {}, direction: 'lr'},
  'direction tb': {options: {}, direction: 'tb'},
  'direction hybrid': {options: {}, direction: 'hybrid'},
  'direction shortest': {options: {}, direction: 'shortest'},
  'direction hybrid, end-to-end criterion': {options: {hybridBy: 'endToEnd'}, direction: 'hybrid'},
  'free placement': {options: {freePlacement: true}},
  'placement search 40': {options: {placementSearch: 40}},
  'placement search 200': {options: {placementSearch: 200}},
  'placement search 600': {options: {placementSearch: 600}},
  'routing clearance + corridors': {options: {routingClearance: 10, trackSpacing: 10, routingCorridors: true}},
};

const args = process.argv.slice(2);
const option = (name, fallback) => {
  const i = args.indexOf(name);
  return i >= 0 ? args.splice(i, 2)[1] : fallback;
};
const LIMIT = Number(option('--limit', 150));
const OUT = option('--out', `${ROOT}benchmarks/results/grid-${new Date().toISOString().replace(/[:.]/g, '-')}.jsonl`);
const chosen = args.length ? args : Object.keys(VARIANTS);
const datasets = readdirSync(`${ROOT}benchmarks/datasets`)
  .filter(f => f.endsWith('.json') && !/^synthetic-20k|intentweave/.test(f))
  .map(f => f.slice(0, -5));

function run(name, variant) {
  const graph = JSON.parse(readFileSync(`${ROOT}benchmarks/datasets/${name}.json`, 'utf8'));
  if (variant.direction) graph.defaultDirection = variant.direction;
  const imported = arqlet({op: 'import', graph, limit: LIMIT});
  let snapshot = imported.snapshot;
  let configureMs = 0;
  if (Object.keys(variant.options).length) {
    const configured = arqlet({
      op: 'configure', snapshot, expectedRevision: snapshot.revision,
      expectedLayoutVersion: snapshot.layoutVersion ?? 0, options: variant.options,
    });
    snapshot = configured.snapshot;
    configureMs = configured.ms;
  }
  const refined = arqlet({
    op: 'refine', snapshot, expectedRevision: snapshot.revision,
    expectedLayoutVersion: snapshot.layoutVersion ?? 0, threads: 1,
  });
  const out = normalizeArqlet(refined.snapshot);
  return {
    ...metrics(out),
    drawnEdges: out.edges.length,
    refineMs: round(refined.ms),
    setupMs: round(imported.ms + configureMs),
    candidatesEvaluated: refined.snapshot.optimization?.candidatesEvaluated ?? 1,
    strategy: refined.snapshot.optimization?.strategy ?? 'keep-existing',
  };
}

mkdirSync(OUT.slice(0, OUT.lastIndexOf('/')), {recursive: true});
const rows = [];
for (const variantName of chosen) {
  const variant = VARIANTS[variantName];
  if (!variant) throw new Error(`Unknown variant: ${variantName}`);
  for (const name of datasets) {
    const row = {variant: variantName, dataset: name, ...run(name, variant)};
    rows.push(row);
    console.log(`${variantName.padEnd(40)} ${name.padEnd(22)} crossings ${String(row.crossings).padStart(4)}  line-node ${String(row.lineNodeHits).padStart(3)}  length ${String(Math.round(row.length)).padStart(7)}  refine ${row.refineMs} ms  ${row.strategy}`);
  }
}
writeFileSync(OUT, rows.map(r => JSON.stringify(r)).join('\n') + '\n');
console.log(`Results: ${OUT}`);
