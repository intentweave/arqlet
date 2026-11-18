#!/usr/bin/env node
// The option `routeSearchWindow` against the default: the same import with and without it, scored by the shared metrics
// (line–node hits, crossings, bends, area) and timed. Each dataset is imported with `limit` (default 1000).
//
// Usage: node benchmarks/eval/search-window.mjs [--out FILE.jsonl] DATASET[@LIMIT] ...
//   DATASET is a name in benchmarks/datasets (without .json) or a path to a graph file.
import {readFileSync, writeFileSync, existsSync} from 'node:fs';
import {ROOT, arqlet, normalizeArqlet, metrics} from './lib.mjs';

const args = process.argv.slice(2);
const at = args.indexOf('--out');
const out = at >= 0 ? args.splice(at, 2)[1] : null;
const KEYS = ['lineNodeHits', 'siblingOverlaps', 'crossings', 'bends', 'area', 'length'];
const rows = [];
for (const spec of args) {
  const [name, limit] = spec.split('@');
  const file = existsSync(name) ? name : `${ROOT}benchmarks/datasets/${name}.json`;
  const parsed = JSON.parse(readFileSync(file, 'utf8'));
  const graph = parsed.graph ?? parsed;
  const row = {dataset: name.split('/').pop().replace(/\.json$/, ''), limit: Number(limit ?? 1000)};
  for (const [variant, options] of [['default', {}], ['window', {routeSearchWindow: true}]]) {
    const {snapshot, ms} = arqlet({op: 'import', graph, limit: row.limit, ...(variant === 'default' ? {} : {options})});
    const m = metrics(normalizeArqlet(snapshot));
    row[variant] = {ms: Math.round(ms), ...Object.fromEntries(KEYS.filter(k => k in m).map(k => [k, m[k]]))};
  }
  row.identical = KEYS.every(k => row.default[k] === row.window[k]);
  rows.push(row);
  const cell = v => KEYS.filter(k => k in v).map(k => `${k} ${typeof v[k] === 'number' ? Math.round(v[k]) : v[k]}`).join(', ');
  console.log(`${row.dataset}@${row.limit}: default [${cell(row.default)}] ${row.default.ms} ms | window [${cell(row.window)}] ${row.window.ms} ms`);
}
if (out) writeFileSync(out, rows.map(r => JSON.stringify(r)).join('\n') + '\n');
