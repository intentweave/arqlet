// A dense flat graph (#152): S sources, T targets and E distinct edges source -> target (seeded, public), as an import
// request for `layout-command`. Nearly every node lands in one rank, as in a wide flat ELK view.
//
// Usage: node benchmarks/experiments/dense-flat.mjs S T E [sharedTracks value] > request.json
const [S, T, E] = [Number(process.argv[2] ?? 12), Number(process.argv[3] ?? 488), Number(process.argv[4] ?? 3138)];
const shared = process.argv[5];
let seed = 11;
const rnd = () => (seed = (seed * 48271) % 2147483647) / 2147483647;
const nodes = [];
for (let i = 0; i < S; i++) nodes.push({id: `s${i}`, kind: 'k', label: `S${i}`});
for (let j = 0; j < T; j++) nodes.push({id: `t${j}`, kind: 'k', label: `T${j}`});
const seen = new Set();
const edges = [];
while (edges.length < E) {
  const a = Math.floor(rnd() * S);
  const b = Math.floor(rnd() * T);
  const key = `${a}:${b}`;
  if (seen.has(key)) continue;
  seen.add(key);
  edges.push({id: `e${edges.length}`, kind: 'k', source: `s${a}`, target: `t${b}`});
}
const options = shared ? {sharedTracks: shared} : {};
process.stdout.write(JSON.stringify({op: 'import', graph: {nodes, edges}, limit: 10000, options}));
