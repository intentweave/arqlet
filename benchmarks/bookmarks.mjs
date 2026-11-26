#!/usr/bin/env node
// Sizes of view bookmarks (#151): the long form (`encodeBookmark`) against the compact one (`compactBookmark`), in
// characters of URL text, for explored views of generated graphs. Sizes only, no timing: the output depends on the engine
// and this script, not on the machine.
//
//   cargo build -p arqlet-wasm --target wasm32-unknown-unknown --release --locked
//   node benchmarks/bookmarks.mjs > benchmarks/results/bookmarks-<date>.md
import {readFileSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {createEngine} from '../packages/engine-client/wasm-client.js';
import {createSession} from '../packages/engine-client/session.js';
import {createViewController, encodeBookmark} from '../packages/engine-client/view-controller.js';

const root = new URL('../', import.meta.url);
const engine = await createEngine(readFileSync(new URL('target/wasm32-unknown-unknown/release/arqlet_wasm.wasm', root)));

const two = n => String(n).padStart(2, '0');
const pkg = p => `services/domain-${two(p)}`;
const mod = (p, m) => `${pkg(p)}/module-${two(m)}.ts`;
const fn = (p, m, f) => `${mod(p, m)}#function-${two(f)}`;

// Packages of modules of functions with ids that are paths; calls along each module, from module to module and from
// package to package. No randomness.
function large(packages, modules, functions) {
  const nodes = [];
  const edges = [];
  const edge = (source, target) => edges.push({id: `call-${edges.length}`, kind: 'calls', source, target});
  for (let p = 0; p < packages; p++) {
    nodes.push({id: pkg(p), kind: 'package', label: `domain ${p}`});
    for (let m = 0; m < modules; m++) {
      nodes.push({id: mod(p, m), kind: 'module', label: `module ${m}`, parentId: pkg(p)});
      for (let f = 0; f < functions; f++) {
        nodes.push({id: fn(p, m, f), kind: f % 4 === 3 ? 'test' : 'function', label: `function ${f}`, parentId: mod(p, m)});
        if (f > 0) edge(fn(p, m, f - 1), fn(p, m, f));
      }
      if (m > 0) edge(fn(p, m - 1, functions - 1), fn(p, m, 0));
    }
    if (p > 0) edge(fn(p - 1, 0, 1), fn(p, 0, 0));
  }
  return {nodes, edges};
}

const cases = [
  {name: 'as imported', size: [24, 24, 10], explore: async () => {}},
  {name: 'explored (12 opened)', size: [12, 12, 10], explore: explore(12)},
  {name: 'explored (12 opened)', size: [24, 24, 10], explore: explore(12)},
  {name: 'explored (12 opened)', size: [20, 40, 10], explore: explore(12)},
  {name: 'explored (50 opened)', size: [24, 24, 10], explore: explore(50)},
  {name: 'explored (200 opened)', size: [24, 24, 10], explore: explore(200)},
  {name: 'every container closed', size: [24, 24, 10], explore: async view => {
    for (let p = 0; p < 24; p++) await ok(view.close(pkg(p)));
  }},
];

// Opens `count` modules (in package order), pages one, filters, traces a path and selects a node.
function explore(count) {
  return async (view, [packages, modules]) => {
    const open = [];
    for (let i = 0; i < count; i++) open.push(mod(i % packages, Math.floor(i / packages) % modules));
    // One step for all of them: the decisions are the same as after one `open` each.
    await ok(view.reveal(open.map((id, i) => fn(i % packages, Math.floor(i / packages) % modules, 0)), {partial: false}));
    await ok(view.reveal([fn(packages - 1, modules - 1, 5)]));
    await ok(view.filter({nodeKinds: ['test'], nodes: [fn(0, 0, 0)]}));
    await ok(view.trace(fn(0, 0, 1), fn(0, 0, 6)));
    await ok(view.select({nodeId: fn(1, 0, 2)}));
  };
}

async function ok(step) {
  const result = await step;
  if (!result.ok) throw Error(result.error ?? 'refused');
  return result;
}

const rows = [];
for (const {name, size, explore: run} of cases) {
  const graph = large(...size);
  const init = {limit: 10000, options: {levelOfDetail: {depth: 1}}};
  const view = createViewController(createSession(engine, {resident: true}), {scenes: false});
  await ok(view.import(graph, init));
  await run(view, size);
  const {bookmark} = await ok(view.bookmark());
  const long = encodeBookmark(bookmark);
  const {text} = await ok(view.compactBookmark());
  // Checked: a fresh view opens it with nothing left out and arrives at the same decisions.
  const again = createViewController(createSession(engine, {resident: true}), {scenes: false});
  await ok(again.import(graph, init));
  const opened = await ok(again.openBookmark(text));
  const same = JSON.stringify((await ok(again.bookmark())).bookmark.intent) === JSON.stringify(bookmark.intent);
  if (!same || opened.missing.length || opened.ignored.length) throw Error(`${name}: the view differs`);
  rows.push([name, graph.nodes.length, bookmark.intent.collapsedIds.length, long.length, text.length]);
}

const git = args => execFileSync('git', args, {cwd: root}).toString().trim();
const dirty = git(['status', '--porcelain', '--', 'crates', 'packages', 'benchmarks/bookmarks.mjs']) ? ' with local changes' : '';
console.log(`# View bookmark sizes (#151)

Revision ${git(['rev-parse', '--short', 'HEAD'])}${dirty}; engine from \`cargo build -p arqlet-wasm --target
wasm32-unknown-unknown --release --locked\`; Node ${process.version} on ${process.platform}-${process.arch}. Sizes only: they do
not depend on the machine. Command: \`node benchmarks/bookmarks.mjs\`.

Graphs generated by the script (packages of modules of ten functions, ids that are paths such as
\`${fn(3, 7, 2)}\`), imported with \`levelOfDetail: {depth: 1}\` and a limit of 10000. "Explored": the named
number of modules opened, one more shown in part, a filter by kind and one node, a path highlighted and a node selected.
Characters of URL text; every compact bookmark was opened on a fresh import and gave the same decisions.

| View | Source nodes | Collapsed containers | Long form | Compact |
| --- | ---: | ---: | ---: | ---: |`);
for (const row of rows) console.log(`| ${row.join(' | ')} |`);
