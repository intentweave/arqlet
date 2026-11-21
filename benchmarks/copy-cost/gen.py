"""Generate the graphs of the copy-cost measurement with the repository's own generator.

    python3 benchmarks/copy-cost/gen.py            # writes benchmarks/datasets/synthetic-{2k,20k,100k,200k}.json

2k and 20k use the seeds of the repository's datasets (11 and 12); 100k and 200k use seeds 13 and 14 chosen for this
measurement. The files are written compactly and are git-ignored (`/benchmarks/datasets/`).
"""
import importlib.util
import json
import os

here = os.path.dirname(os.path.abspath(__file__))
repo = os.path.dirname(os.path.dirname(here))
out = os.path.join(repo, "benchmarks", "datasets")
os.makedirs(out, exist_ok=True)

spec = importlib.util.spec_from_file_location("generate", os.path.join(repo, "benchmarks", "generate.py"))
generate = importlib.util.module_from_spec(spec)
spec.loader.exec_module(generate)

for name, leaves, seed in [("synthetic-2k", 2000, 11), ("synthetic-20k", 20000, 12), ("synthetic-100k", 100000, 13), ("synthetic-200k", 200000, 14)]:
    graph = generate.synthetic(leaves, seed)
    with open(os.path.join(out, f"{name}.json"), "w") as f:
        json.dump(graph, f, separators=(",", ":"))
    unresolved = sum(1 for e in graph["edges"] if e.get("target") is None)
    print(f"{name}: leaves={leaves} seed={seed} nodes={len(graph['nodes'])} edges={len(graph['edges'])} unresolved={unresolved}")
