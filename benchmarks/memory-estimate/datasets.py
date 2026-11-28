#!/usr/bin/env python3
"""The generated graphs of the memory-estimate calibration (ADR 0024, C3), written to benchmarks/datasets/ (git-ignored):

- synthetic-5k, -50k, -100k: `generate.py::synthetic` (seeds 14, 15, 13; 100k as in session-memory-20261121.md);
- full-1000, full-3000: n leaves in containers of ten with 1.3·n random edges (cycles, self-loops, parallel edges), the
  shape of benchmarks/node-adapter.mjs (PR #232), laid out whole (limit n plus the containers);
- code-10000, code-50000, code-100000: `overview/generate.py` code indexes (seed 1), for the overview.

Standard library only; deterministic.
"""
import json, random, sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "benchmarks"))
import generate as g  # noqa: E402

OUT = ROOT / "benchmarks" / "datasets"


def full(n, seed=7):
    rng = random.Random(seed + n)
    nodes, edges = [], []
    containers = (n + 9) // 10
    for c in range(containers):
        nodes.append({"id": f"c{c}", "kind": "module", "label": f"module {c}"})
    for i in range(n):
        nodes.append({"id": f"n{i}", "kind": "function", "label": f"f{i}", "parentId": f"c{i // 10}"})
    for e in range(int(1.3 * n)):
        s = rng.randrange(n)
        roll = rng.random()
        t = s if roll < 0.02 else (min(n - 1, s + rng.randint(1, 30)) if roll < 0.8 else rng.randrange(n))
        edges.append({"id": f"e{e}", "kind": "calls", "source": f"n{s}", "target": f"n{t}"})
        if rng.random() < 0.02:
            edges.append({"id": f"e{e}b", "kind": "calls", "source": f"n{s}", "target": f"n{t}"})
    return {"nodes": nodes, "edges": edges}


def write(name, graph):
    OUT.mkdir(parents=True, exist_ok=True)
    (OUT / f"{name}.json").write_text(json.dumps(graph, separators=(",", ":")))
    print(f"{name}: {len(graph['nodes'])} nodes, {len(graph['edges'])} edges")


if __name__ == "__main__":
    for name, n, seed in [("synthetic-5k", 5000, 14), ("synthetic-50k", 50000, 15), ("synthetic-100k", 100000, 13)]:
        write(name, g.synthetic(n, seed))
    for n in (1000, 3000):
        write(f"full-{n}", full(n))
    import importlib.util
    spec = importlib.util.spec_from_file_location("ovgen", ROOT / "benchmarks" / "overview" / "generate.py")
    ov = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(ov)
    for n in (10000, 50000, 100000):
        write(f"code-{n}", ov.generate(n, 1))
