// Random graphs in ELK's JSON form with ports on all four sides (`FIXED_SIDE`), laid out by `elkLayout` (one or two
// `layout-command` binaries, the second with optional Arqlet options) and optionally by elk-rs, all measured by the same
// `metrics` operation. Finding and results: benchmarks/results/elk-format-20261108.md, elk-port-sides-20261108.md.
// node benchmarks/eval/elk-side-ports.mjs [--options JSON] [--elk] [--graphs 40] BINARY [BINARY]
import {spawnSync} from "node:child_process";
import ElkRs from "@openedges/elk-layout";

const argv = process.argv.slice(2);
const take = flag => { const i = argv.indexOf(flag); return i < 0 ? null : argv.splice(i, 2)[1]; };
const options = take("--options");
const graphs = Number(take("--graphs") ?? 40);
const elkAt = argv.indexOf("--elk");
const withElk = elkAt >= 0 && argv.splice(elkAt, 1);
const [first, second = first] = argv;
const call = (bin, request) => {
  const p = spawnSync(bin, [], {input: JSON.stringify(request), encoding: "utf8", maxBuffer: 1 << 28});
  if (p.status) throw new Error(p.stderr);
  return JSON.parse(p.stdout);
};
let seed = 7;
const rnd = () => ((seed = (seed * 1103515245 + 12345) % 2147483648) / 2147483648);
const sides = ["NORTH", "SOUTH", "EAST", "WEST"];
const elkRs = new ElkRs();
const keys = ["crossings", "lineNodeHits", "sharedLinePairs", "hugPairs", "siblingOverlaps", "bends"];
const total = {};

// The drawing of a laid-out flat ELK graph, for `metrics`.
function drawing(graph, laid) {
  const owner = new Map(graph.children.flatMap(c => c.ports.map(p => [p.id, c.id])));
  return {
    nodes: laid.children.map(n => ({id: n.id, parentId: null, container: false, x: n.x, y: n.y, width: n.width, height: n.height})),
    edges: laid.edges.map(e => {
      const s = e.sections[0];
      return {id: e.id, source: owner.get(e.sources[0]), target: owner.get(e.targets[0]),
        points: [s.startPoint, ...(s.bendPoints ?? []), s.endPoint], sourcePort: null, targetPort: null};
    }),
  };
}

for (let g = 0; g < graphs; g++) {
  const n = 6 + Math.floor(rnd() * 14);
  const children = [...Array(n)].map((_, i) => ({id: `n${i}`, width: 80, height: 40,
    layoutOptions: {"elk.portConstraints": "FIXED_SIDE"}, ports: []}));
  const edges = [];
  for (let k = 0; k < n * 1.4; k++) {
    const s = Math.floor(rnd() * n), t = Math.floor(rnd() * n);
    if (s === t) continue;
    const ps = {id: `p${k}s`, layoutOptions: {"elk.port.side": sides[Math.floor(rnd() * 4)]}};
    const pt = {id: `p${k}t`, layoutOptions: {"elk.port.side": sides[Math.floor(rnd() * 4)]}};
    children[s].ports.push(ps); children[t].ports.push(pt);
    edges.push({id: `e${k}`, sources: [ps.id], targets: [pt.id]});
  }
  const graph = {id: "root", layoutOptions: {"elk.direction": rnd() < 0.5 ? "RIGHT" : "DOWN"}, children, edges};
  const arms = [["before", first, null], ["after", second, options && JSON.parse(options)]];
  if (withElk) arms.push(["elkRs", null, null]);
  for (const [name, bin, extra] of arms) {
    const t = (total[name] ??= {graphs: 0, constructed: 0, delegated: 0, ms: 0});
    let laid;
    const start = performance.now();
    if (bin) {
      const r = call(bin, {op: "elkLayout", graph, snapshot: true, ...(extra ? {options: extra} : {})});
      if (!r.ok) { t.failed = (t.failed ?? 0) + 1; continue; }
      laid = r.graph;
      const sp = (r.snapshot.layoutDiagnostics ?? []).join(" ").match(/Single pass: (\d+) constructed, (\d+) delegated/);
      if (sp) { t.constructed += +sp[1]; t.delegated += +sp[2]; }
    } else {
      const input = structuredClone(graph);
      Object.assign(input.layoutOptions, {"elk.algorithm": "layered", "elk.edgeRouting": "ORTHOGONAL"});
      try { laid = await elkRs.layout(input); } catch (error) { t.failed = (t.failed ?? 0) + 1; continue; }
    }
    t.ms += performance.now() - start;
    t.graphs++;
    const m = call(first, {op: "metrics", drawing: drawing(graph, laid)});
    if (!m.ok) throw new Error(m.error);
    for (const k of keys) t[k] = (t[k] ?? 0) + (m.metrics[k] ?? 0);
  }
}
for (const t of Object.values(total)) t.ms = Math.round(t.ms);
console.log(JSON.stringify(total, null, 1));
