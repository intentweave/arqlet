// A synthetic bus view (#149): `units` nodes on `buses` buses given as data (each unit on 1 to 3 buses, seeded), as an
// import request for `layout-command`. Public and deterministic.
//
// Usage: node benchmarks/experiments/bus-mesh.mjs UNITS BUSES [organic|layered] [lines|bar] [CHILDREN EDGES] > request.json
//
// With CHILDREN and EDGES, a drill-down into the first bus (#156): CHILDREN nodes under up to 11 of its members, and
// EDGES edges from those children to the other members (as frames to their receivers). With BUSES 0 there is no bus
// and the children go under the first 11 units.
const [units, nbus] = [Number(process.argv[2] ?? 100), Number(process.argv[3] ?? 30)];
const strategy = process.argv[4] ?? 'organic';
const style = process.argv[5] ?? 'lines';
let seed = 7;
const rnd = () => (seed = (seed * 1103515245 + 12345) % 2147483648) / 2147483648;
const nodes = [];
const buses = [];
for (let i = 0; i < units; i++) nodes.push({id: `u${i}`, kind: 'unit', label: `U${i}`});
for (let b = 0; b < nbus; b++) buses.push({id: `bus${b}`, kind: 'bus', label: `Bus ${b}`, taps: []});
for (let i = 0; i < units && nbus > 0; i++) {
  const k = 1 + Math.floor(rnd() * 3);
  for (let j = 0; j < k; j++) {
    const b = buses[Math.floor(rnd() * nbus)];
    if (!b.taps.some(t => t.node === `u${i}`)) b.taps.push({edge: `t${b.id}-${i}`, node: `u${i}`, toBus: true, kind: 'on'});
  }
}
const graph = {nodes, edges: [], buses: buses.filter(b => b.taps.length)};
const [children, edgeCount] = [Number(process.argv[6] ?? 0), Number(process.argv[7] ?? 0)];
if (children > 0) {
  // The members of the first bus, or without buses (BUSES 0) the first 11 units.
  const members = (graph.buses[0]?.taps.map(t => t.node) ?? nodes.map(n => n.id)).slice(0, 11);
  for (let k = 0; k < children; k++) nodes.push({id: `f${k}`, kind: 'frame', label: `F${k}`, parentId: members[k % members.length]});
  for (let k = 0; k < edgeCount; k++) {
    const f = k % children;
    const to = members[(f + 1 + (k % (members.length - 1))) % members.length];
    graph.edges.push({id: `rx${k}`, kind: 'rx', source: `f${f}`, target: to});
  }
}
if (strategy === 'organic') Object.assign(graph, {defaultStrategy: 'organic', defaultDirection: 'auto'});
process.stdout.write(JSON.stringify({op: 'import', graph, limit: 10000, options: {busStyle: style}}));
