#!/usr/bin/env node
// Port placement experiment: arqlet with each automatic port placement, scored with the metric code of compare.mjs.
// Purpose: find how many crossings the port order removes (crossings at a shared endpoint node), before any change to the
// routing. The default is `symmetric`; `angular` and `optimized` are the existing alternatives.
//
// Usage: node port-experiment.mjs [--limit 100] [--out FILE.jsonl] [DATASET ...]
import {readFileSync, mkdirSync, writeFileSync} from 'node:fs';
import {ROOT, arqlet, normalizeArqlet, metrics} from './lib.mjs';

const args = process.argv.slice(2);
const option = (name, fallback) => {
  const i = args.indexOf(name);
  return i >= 0 ? args.splice(i, 2)[1] : fallback;
};
const LIMIT = Number(option('--limit', 100));
const OUT = option('--out', `${ROOT}benchmarks/results/ports-${new Date().toISOString().replace(/[:.]/g, '-')}.jsonl`);
const selected = args.length ? args : ['network-vlan', 'activity-order', 'petri-net', 'arqlet-pipeline', 'cloud-architecture', 'bpmn-pools', 'uml-class', 'layerweave-code'];
const PLACEMENTS = ['symmetric', 'angular', 'optimized'];

mkdirSync(OUT.slice(0, OUT.lastIndexOf('/')), {recursive: true});
const rows = [];
for (const name of selected) {
  const graph = JSON.parse(readFileSync(`${ROOT}benchmarks/datasets/${name}.json`, 'utf8'));
  for (const portPlacement of PLACEMENTS) {
    const imported = arqlet({op: 'import', graph, limit: LIMIT, options: {portPlacement}});
    const refined = arqlet({
      op: 'refine', snapshot: imported.snapshot, expectedRevision: imported.snapshot.revision,
      expectedLayoutVersion: imported.snapshot.layoutVersion ?? 0, threads: 1,
    });
    for (const [stage, snapshot] of [['initial', imported.snapshot], ['refined', refined.snapshot]]) {
      const m = metrics(normalizeArqlet(snapshot));
      rows.push({dataset: name, portPlacement, stage, crossings: m.crossings, sharedEndpointCrossings: m.sharedEndpointCrossings,
        sharedLinePairs: m.sharedLinePairs, lineNodeHits: m.lineNodeHits, bends: m.bends, length: m.length, hugPairs: m.hugPairs});
    }
    console.log(`${name.padEnd(22)} ${portPlacement.padEnd(10)} refined: crossings ${refined.snapshot ? rows.at(-1).crossings : '-'} at shared endpoints ${rows.at(-1).sharedEndpointCrossings}`);
  }
}
writeFileSync(OUT, rows.map(r => JSON.stringify(r)).join('\n') + '\n');
console.log(`Results: ${OUT}`);
