#!/usr/bin/env python3
"""Deterministic hierarchical graphs for the cluster overview (ADR 0018), shaped like a code index: directories hold
sub-directories and files, files hold symbols, and call edges stay mostly local (same file, then same directory), with a
share of edges across the whole graph to popular targets. Standard library only.

    python3 benchmarks/overview/generate.py --nodes 100000 --seed 1 --out benchmarks/datasets/overview-100000.json

Sizes vary by a seeded heavy-tailed draw, so containers range from a few to a few hundred children. The output is a
graph in Arqlet's input form (nodes with parentId, edges with source and target); `--stats` prints its shape.
"""
import argparse, json, random

def generate(nodes, seed, edges_per_symbol=1.6):
    rng = random.Random(seed)
    out_nodes, out_edges = [], []
    files = []          # (file id, directory id, [symbol ids])
    dirs = []           # (directory id, parent id or None)

    def add_dir(parent, depth):
        did = f"d{len(dirs)}"
        dirs.append((did, parent))
        out_nodes.append({"id": did, "kind": "directory", "label": did, "parentId": parent})
        return did

    roots = [add_dir(None, 0) for _ in range(max(3, int((nodes / 5000) ** 0.5 * 4)))]
    frontier = [(r, 0) for r in roots]
    count = len(out_nodes)
    i = 0
    while count < nodes:
        parent, depth = frontier[i % len(frontier)]
        i += 1
        if depth < 3 and rng.random() < 0.25:
            frontier.append((add_dir(parent, depth + 1), depth + 1))
            count += 1
            continue
        fid = f"f{len(files)}"
        out_nodes.append({"id": fid, "kind": "file", "label": fid, "parentId": parent})
        size = min(400, max(2, int(rng.paretovariate(1.3) * 6)))
        size = min(size, nodes - count - 1)
        syms = [f"{fid}/s{k}" for k in range(max(size, 0))]
        for s in syms:
            out_nodes.append({"id": s, "kind": "symbol", "label": s.rsplit("/", 1)[1], "parentId": fid})
        files.append((fid, parent, syms))
        count += 1 + len(syms)
    by_dir = {}
    for fid, did, syms in files:
        by_dir.setdefault(did, []).extend(syms)
    symbols = [s for _, _, syms in files for s in syms]
    popular = symbols[: max(1, len(symbols) // 50)]
    e = 0
    for fid, did, syms in files:
        for s in syms:
            for _ in range(int(edges_per_symbol) + (rng.random() < edges_per_symbol % 1)):
                roll = rng.random()
                if roll < 0.6 and len(syms) > 1:
                    t = rng.choice(syms)
                elif roll < 0.85:
                    t = rng.choice(by_dir[did])
                elif roll < 0.95:
                    t = rng.choice(popular)
                else:
                    t = rng.choice(symbols)
                out_edges.append({"id": f"e{e}", "kind": "call", "source": s, "target": t})
                e += 1
    # A share of unresolved calls, as in real exports (excluded by the engine).
    for k in range(len(symbols) // 100):
        out_edges.append({"id": f"u{k}", "kind": "call", "source": rng.choice(symbols), "target": None})
    return {"nodes": out_nodes, "edges": out_edges}

def flatten(g):
    """The symbols only, at the top level, with the calls between them: the containment is gone, so the overview must find
    the clusters itself (community detection)."""
    symbols = [dict(n, parentId=None) for n in g["nodes"] if n["kind"] == "symbol"]
    keep = {n["id"] for n in symbols}
    edges = [e for e in g["edges"] if e["source"] in keep and (e["target"] is None or e["target"] in keep)]
    return {"nodes": symbols, "edges": edges}

def main():
    p = argparse.ArgumentParser()
    p.add_argument("--nodes", type=int, required=True)
    p.add_argument("--seed", type=int, default=1)
    p.add_argument("--out", required=True)
    p.add_argument("--stats", action="store_true")
    p.add_argument("--flat", action="store_true", help="symbols only, without directories and files (a flat call graph)")
    a = p.parse_args()
    g = generate(a.nodes, a.seed)
    if a.flat:
        g = flatten(g)
    with open(a.out, "w") as f:
        json.dump(g, f, separators=(",", ":"))
    if a.stats:
        kids = {}
        for n in g["nodes"]:
            kids.setdefault(n["parentId"], 0)
            kids[n["parentId"]] += 1
        sizes = sorted(kids.values())
        print(json.dumps({"nodes": len(g["nodes"]), "edges": len(g["edges"]), "containers": len(kids) - 1,
                          "largestContainer": sizes[-1], "medianContainer": sizes[len(sizes) // 2]}))

if __name__ == "__main__":
    main()
