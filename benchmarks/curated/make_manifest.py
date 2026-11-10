"""Writes benchmarks/curated/manifest.json: the corpus cases for the curated-reference milestone (C0/C1).

Each case has a role (benchmark, reference fixture or planned diagnostic), the file it uses with its SHA-256, the failure
it targets, and its status. Planned cases have no file yet and are not evidence. Run from the repository root:

    python3 benchmarks/curated/make_manifest.py
"""
import hashlib
import json
import os

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))


def sha256(path):
    with open(os.path.join(ROOT, path), "rb") as f:
        return hashlib.sha256(f.read()).hexdigest()


BENCHMARK_INPUTS = [
    ("state-tcp", "cycles, back edges, compact state graph"),
    ("workflow-dag", "layering, long connections, varying path lengths"),
    ("network-vlan", "dense connections, shared channels"),
    ("uml-class", "varied relationship kinds and port constellations"),
    ("bpmn-pools", "container boundaries (pools are not swimlane support)"),
    ("org-chart", "plain tree, control case: no needless worsening"),
    ("layerweave-code", "real nesting, uneven port counts"),
    ("synthetic-2k", "effort and size growth; visible projection reported separately"),
]
REFERENCE_FIXTURES = [
    ("ports-fanout", "parallel fan-out and port order"),
    ("cycle-selfloop", "cycles and self-loops"),
    ("container-boundary", "container boundary attachments"),
]
PLANNED = [
    ("diagnostic-port-density", "high port density on one side"),
    ("diagnostic-narrow-channel", "narrow channels between nodes"),
    ("diagnostic-explicit-ports", "explicit ports fixed among automatic ones"),
    ("diagnostic-pins", "pinned nodes"),
    ("diagnostic-labels", "labels reserving space"),
    ("real-excerpt", "frozen real Intentweave excerpt with its boundary context (no export in this checkout)"),
]

cases = []
for name, target in BENCHMARK_INPUTS:
    path = f"benchmarks/inputs/{name}.json"
    cases.append({"id": name, "role": "benchmark", "file": path, "sha256": sha256(path),
                  "targets": target, "status": "present"})
for name, target in REFERENCE_FIXTURES:
    path = f"examples/fixtures/reference/{name}.graph.json"
    cases.append({"id": name, "role": "reference-fixture", "file": path, "sha256": sha256(path),
                  "targets": target, "status": "present"})
state_moved = "examples/fixtures/reference/state-tcp-moved.lwref.json"
cases.append({"id": "state-tcp-moved", "role": "reference-fixture", "file": state_moved,
              "sha256": sha256(state_moved), "targets": "hand-edited curated reference (round-trip)",
              "status": "present"})
for name, target in PLANNED:
    cases.append({"id": name, "role": "planned", "file": None, "sha256": None, "targets": target,
                  "status": "planned"})

manifest = {"format": "arqlet-corpus-manifest", "schemaVersion": 1, "cases": cases}
path = os.path.join(ROOT, "benchmarks", "curated", "manifest.json")
with open(path, "w") as f:
    json.dump(manifest, f, indent=1)
    f.write("\n")
print(f"{sum(c['status'] == 'present' for c in cases)} present, {sum(c['status'] == 'planned' for c in cases)} planned -> {path}")
