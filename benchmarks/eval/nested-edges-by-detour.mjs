// Usage: node benchmarks/eval/nested-edges-by-detour.mjs [--json]; needs target/release/layout-command.
// Experimental `nestedEdgesByDetour` against the default of S1-4 (regionPorts, straightDetours, singlePassReversed, set
// explicitly) on every dataset in benchmarks/datasets except the very large ones: bounded views of 150 nodes, one import
// each; whether the drawing is identical (boxes and routes hashed), and otherwise its metrics, geometry issues and time.
import {readdirSync, readFileSync} from 'node:fs';
import {createHash} from 'node:crypto';
import {arqlet, normalizeArqlet, metrics, ROOT} from './lib.mjs';

const SKIP = /^(synthetic-20k|protocol-|overview-|intentweave)/;
const B = {regionPorts: true, straightDetours: true, singlePassReversed: true};
const hash = s => createHash('sha256').update(JSON.stringify({boxes: s.boxes, routes: s.routes})).digest('hex').slice(0, 12);
const rows = [];
for (const f of readdirSync(`${ROOT}benchmarks/datasets`).filter(f => f.endsWith('.json') && !SKIP.test(f)).sort()) {
  const raw = JSON.parse(readFileSync(`${ROOT}benchmarks/datasets/${f}`, 'utf8'));
  const graph = raw.graph ?? raw;
  const out = {};
  for (const [variant, options] of [['default', B], ['nestedEdgesByDetour', {...B, nestedEdgesByDetour: true}]]) {
    const {snapshot, ms} = arqlet({op: 'import', graph, limit: 150, options});
    const m = metrics(normalizeArqlet(snapshot));
    out[variant] = {hash: hash(snapshot), lineNodeHits: m.lineNodeHits, crossings: m.crossings, bends: m.bends,
      areaK: Math.round(m.area / 1e3), geometryIssues: snapshot.geometryIssues.length, ms: Math.round(ms)};
  }
  rows.push({dataset: f.replace('.json', ''), identical: out.default.hash === out.nestedEdgesByDetour.hash, ...out});
}
if (process.argv.includes('--json')) for (const r of rows) console.log(JSON.stringify(r));
else {
  console.log('| dataset | identical | default: hits / crossings / bends / area (k) / issues / ms | nestedEdgesByDetour: same |');
  console.log('|---|---|---|---|');
  const cell = v => `${v.lineNodeHits} / ${v.crossings} / ${v.bends} / ${v.areaK} / ${v.geometryIssues} / ${v.ms}`;
  for (const r of rows) console.log(`| ${r.dataset} | ${r.identical ? 'yes' : 'no'} | ${cell(r.default)} | ${cell(r.nestedEdgesByDetour)} |`);
}
