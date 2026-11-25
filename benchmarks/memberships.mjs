#!/usr/bin/env node
// Baseline of ADR 0022 M3 (holding): how soft membership groups are drawn when nothing holds them, and what the same
// groups cost as containers. No timing; every number is deterministic.
//
// Inputs: the membership fixtures under every choice of the first criterion, and the corpus of `benchmarks/inputs` (one
// import each, at most 200 nodes) with two kinds of planted groups of four leaves, from a fixed seed:
// `near` grows each group along edges, `far` deals shuffled leaves. Planted groups are synthetic: they say how the stages
// behave, not how real groupings look.
//
// A group is `inside` when all its drawn leaves have one container in the view, else `across`. Inside groups are what
// holding within an owner (M3b) can improve, across groups need regions across owners (M3c).
//
// With `--hold`, every view is laid out with `holdMemberships` (M3b) and compared with the same view without it.
//
// Usage: node benchmarks/memberships.mjs [--hold] [--out FILE.jsonl]      (needs target/release/layout-command)
import {readFileSync, writeFileSync, readdirSync} from 'node:fs';
import {spawnSync} from 'node:child_process';
import {ROOT, LAYOUT_COMMAND, normalizeArqlet, metrics, round} from './eval/lib.mjs';

const SEED = 22, SIZE = 4, LIMIT = 200;
const at = process.argv.indexOf('--out');
const HOLD = process.argv.includes('--hold');
const OUT = at >= 0 ? process.argv[at + 1] : null;

function run(request) {
  const result = spawnSync(LAYOUT_COMMAND, [], {input: JSON.stringify(request), encoding: 'utf8', maxBuffer: 256 * 1024 * 1024});
  if (result.status !== 0) throw new Error(result.stderr);
  const answer = JSON.parse(result.stdout);
  if (!answer.ok) throw new Error(answer.error);
  return answer;
}

/** mulberry32: a small seeded generator, so the planted groups are the same on every machine. */
function generator(seed) {
  return () => {
    seed = (seed + 0x6d2b79f5) | 0;
    let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** Disjoint groups of SIZE leaves covering about half the leaves; a group that cannot reach SIZE is dropped. */
function plant(graph, kind) {
  const random = generator(SEED);
  const parents = new Set(graph.nodes.map(n => n.parentId).filter(Boolean));
  const leaves = graph.nodes.filter(n => !parents.has(n.id)).map(n => n.id);
  for (let i = leaves.length - 1; i > 0; i--) {
    const j = Math.floor(random() * (i + 1));
    [leaves[i], leaves[j]] = [leaves[j], leaves[i]];
  }
  const free = new Set(leaves), near = new Map(leaves.map(l => [l, []]));
  for (const e of graph.edges) if (near.has(e.source) && near.has(e.target) && e.source !== e.target) {
    near.get(e.source).push(e.target);
    near.get(e.target).push(e.source);
  }
  const groups = [], wanted = Math.max(1, Math.floor(leaves.length / 2 / SIZE));
  for (const seed of leaves) {
    if (groups.length === wanted) break;
    if (!free.has(seed)) continue;
    const members = [seed];
    if (kind === 'near') {
      for (let i = 0; i < members.length && members.length < SIZE; i++) for (const next of near.get(members[i])) {
        if (members.length < SIZE && free.has(next) && !members.includes(next)) members.push(next);
      }
    } else for (const next of leaves) {
      if (members.length < SIZE && free.has(next) && next !== seed) members.push(next);
    }
    if (members.length < SIZE) continue;
    for (const m of members) free.delete(m);
    groups.push({id: `g${groups.length}`, members});
  }
  return {dimensions: [{id: kind, groups}]};
}

const QUALITY = ['lineNodeHits', 'crossings', 'sharedEndpointCrossings', 'bends', 'length', 'area'];
function view(graph, priority, hold = false) {
  const options = {...(priority.length ? {groupingPriority: priority} : {}), ...(hold ? {holdMemberships: true} : {})};
  const {snapshot} = run({op: 'import', graph, limit: LIMIT, options});
  const m = metrics(normalizeArqlet(snapshot));
  return {snapshot, quality: Object.fromEntries(QUALITY.map(k => [k, round(m[k])]))};
}

/** Per soft dimension: how its groups are drawn, split by whether a group lies inside one container. */
function stages(graph, snapshot, dimension) {
  const {scene} = run({op: 'scene', snapshot});
  const parent = new Map(snapshot.graph.nodes.map(n => [n.id, n.parentId ?? null]));
  const empty = () => ({groups: 0, whole: 0, split: 0, marked: 0, other: 0});
  const out = {inside: empty(), across: empty()};
  for (const g of graph.memberships.dimensions.find(d => d.id === dimension).groups) {
    const shown = g.members.filter(m => parent.has(m));
    if (!shown.length) continue;
    const row = new Set(shown.map(m => parent.get(m))).size === 1 ? out.inside : out.across;
    const hulls = (scene.membershipHulls ?? []).filter(h => h.dimension === dimension && h.group === g.id);
    const marks = (scene.membershipMarks ?? []).filter(m => m.dimension === dimension && m.group === g.id);
    row.groups++;
    row[marks.length ? 'marked' : hulls.length > 1 ? 'split' : hulls.length === 1 ? 'whole' : 'other']++;
  }
  const report = scene.membershipReport.find(r => r.dimension === dimension);
  return {...out, intrudersWhole: report.intrudersWhole, overlappingPairs: report.overlappingPairs};
}

const rows = [], refused = [];
function record(source, input, graph, dimensions) {
  const base = view(graph, []);
  for (const first of ['parent', ...dimensions]) {
    const soft = ['parent', ...dimensions].filter(d => d !== first);
    let snapshot, quality;
    try {
      ({snapshot, quality} = view(graph, [{by: first}, ...soft.map(by => ({by}))]));
    } catch (error) {
      // A projection can break what an arrangement of the graph requires (a tree, for example): a finding, not a failure.
      refused.push({source, input, first, refused: error.message});
      continue;
    }
    const without = quality;
    let held = 0, plain = null;
    if (HOLD) {
      plain = Object.fromEntries(dimensions.filter(d => d !== first).map(d => [d, stages(graph, snapshot, d)]));
      ({snapshot, quality} = view(graph, [{by: first}, ...soft.map(by => ({by}))], true));
      held = (snapshot.heldGroups ?? []).length;
    }
    if (!HOLD && first === 'parent' && JSON.stringify(quality) !== JSON.stringify(base.quality)) throw new Error(`${input}: a soft criterion changed the layout`);
    const row = {source, input, first, quality, baseline: HOLD ? without : base.quality, ...(HOLD ? {held, plain} : {}), soft: {}};
    for (const d of dimensions) if (d !== first) row.soft[d] = stages(graph, snapshot, d);
    rows.push(row);
  }
}

for (const f of readdirSync(`${ROOT}examples/fixtures/memberships`).filter(f => f.endsWith('.json')).sort()) {
  const {groupingPriority: _, ...graph} = JSON.parse(readFileSync(`${ROOT}examples/fixtures/memberships/${f}`, 'utf8'));
  record('fixture', f.slice(0, -5), graph, graph.memberships.dimensions.map(d => d.id));
}
for (const f of readdirSync(`${ROOT}benchmarks/inputs`).filter(f => f.endsWith('.json') && !/manifest|synthetic/.test(f)).sort()) {
  const graph = JSON.parse(readFileSync(`${ROOT}benchmarks/inputs/${f}`, 'utf8'));
  for (const kind of ['near', 'far']) {
    const memberships = plant(graph, kind);
    if (!memberships.dimensions[0].groups.length) { console.log(`${f} ${kind}: no group of ${SIZE} can be planted`); continue; }
    record(kind, f.slice(0, -5), {...graph, memberships}, [kind]);
  }
}

const show = s => `${s.groups}: ${s.whole}/${s.split}/${s.marked}${s.other ? `/+${s.other}` : ''}`;
console.log('\n| Source | Input | First | Soft | Inside: whole/split/marked | Across: whole/split/marked | Overlapping pairs |\n| --- | --- | --- | --- | --- | --- | --- |');
for (const r of rows) for (const [d, s] of Object.entries(r.soft)) console.log(`| ${r.source} | ${r.input} | ${r.first} | ${d} | ${show(s.inside)} | ${show(s.across)} | ${s.overlappingPairs} |`);
console.log('\n| Source | Input | First | Line-node hits | Crossings | Bends | Length | Area |\n| --- | --- | --- | --- | --- | --- | --- | --- |');
for (const r of rows) if (HOLD ? r.held : r.first !== 'parent') {
  const pair = k => `${Math.round(r.baseline[k])} → ${Math.round(r.quality[k])}`;
  const cross = q => q.crossings + q.sharedEndpointCrossings;
  console.log(`| ${r.source} | ${r.input} | ${r.first}${HOLD ? ` (${r.held} held)` : ''} | ${pair('lineNodeHits')} | ${cross(r.baseline)} → ${cross(r.quality)} | ${pair('bends')} | ${pair('length')} | ${pair('area')} |`);
}
const sum = (source, where, key, which) => rows.filter(r => r.source === source && r.first === 'parent').flatMap(r => Object.values(which === 'plain' ? r.plain : r.soft)).reduce((n, s) => n + s[where][key], 0);
console.log('\nParent first, totals (groups: whole/split/marked):');
for (const which of HOLD ? ['plain', 'held'] : ['']) for (const source of ['fixture', 'near', 'far']) for (const where of ['inside', 'across']) console.log(`${which && which + ' '}${source} ${where} ${show(Object.fromEntries(['groups', 'whole', 'split', 'marked', 'other'].map(k => [k, sum(source, where, k, which)])))}`);
for (const r of refused) console.log(`Refused: ${r.source} ${r.input}, ${r.first} first: ${r.refused}`);
if (OUT) writeFileSync(OUT, [...rows, ...refused].map(r => JSON.stringify(r)).join('\n') + '\n');
