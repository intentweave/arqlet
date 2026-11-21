#!/usr/bin/env node
// ADR 0021, item 12: the query crates are usable without the layout engine. This checks it, for the host and for
// wasm32, from `cargo tree` (normal dependencies):
//   1. no query crate has a layout, routing, scene, view, topology, ELK, reference, runtime, WASM or facade crate in its tree;
//   2. `arqlet-query` has no third-party crate of its own: what its tree holds besides workspace crates must also be in the
//      tree of `arqlet-model` (the one internal crate it may use, for `PortableMath` and the graph part).
//
//   node tools/check-query-isolation.mjs        exits 1 and says why when a rule is broken
//
// `arqlet-query-model`, the adapter, joins QUERY_CRATES when it is created (stage M5a of the ADR).
import {execFileSync} from 'node:child_process';
import {resolve} from 'node:path';
import {fileURLToPath} from 'node:url';

const root = resolve(import.meta.dirname, '..');

export const QUERY_CRATES = ['arqlet-query', 'arqlet-query-cypher'];
export const FORBIDDEN = [
  'arqlet', 'arqlet-layout', 'arqlet-routing', 'arqlet-scene', 'arqlet-view', 'arqlet-topology', 'arqlet-elk',
  'arqlet-reference', 'arqlet-runtime', 'arqlet-wasm', 'arqlet-contracts', 'arqlet-node',
];
const TARGETS = [undefined, 'wasm32-unknown-unknown'];

/** The crate names in the output of `cargo tree --prefix none`. */
export function crateNames(treeOutput) {
  const names = new Set();
  for (const line of treeOutput.split('\n')) {
    const m = /^(\S+) v\S+/.exec(line.trim());
    if (m) names.add(m[1]);
  }
  return names;
}

/** The forbidden crates in a tree. */
export function forbiddenIn(names) {
  return FORBIDDEN.filter(f => names.has(f)).sort();
}

/** The crates of a tree that are neither workspace crates (`arqlet*`) nor in `allowed`. */
export function ownThirdParty(names, allowed) {
  return [...names].filter(n => !n.startsWith('arqlet') && !allowed.has(n)).sort();
}

function tree(crate, target) {
  const args = ['tree', '-p', crate, '-e', 'normal', '--prefix', 'none', '--locked'];
  if (target) args.push('--target', target);
  return crateNames(execFileSync('cargo', args, {cwd: root, encoding: 'utf8', stdio: ['ignore', 'pipe', 'inherit']}));
}

/** Every broken rule as a sentence; empty when the query crates are isolated. */
export function violations(treeOf = tree) {
  const problems = [];
  for (const target of TARGETS) {
    const where = target ?? 'the host';
    for (const crate of QUERY_CRATES) {
      const bad = forbiddenIn(treeOf(crate, target));
      if (bad.length) problems.push(`${crate} depends on ${bad.join(', ')} for ${where}: the query layer must not need the layout engine`);
    }
    const extra = ownThirdParty(treeOf('arqlet-query', target), treeOf('arqlet-model', target));
    if (extra.length) problems.push(`arqlet-query has third-party crates of its own for ${where}: ${extra.join(', ')}`);
  }
  return problems;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const problems = violations();
  if (problems.length) {
    console.error(`Query layer isolation (ADR 0021, item 12) is broken:\n- ${problems.join('\n- ')}`);
    process.exit(1);
  }
  console.log(`ok: ${QUERY_CRATES.join(', ')} are free of the layout engine (host and wasm32)`);
}
