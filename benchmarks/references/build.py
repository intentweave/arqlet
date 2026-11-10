"""Builds the curated reference corpus: one folder per case, plus manifest.json (the index).

usage: python3 benchmarks/references/build.py [path/to/layout-command]

Cases are created through the layout-command binary, so the stored documents are the engine's own output. Each
document is stored as canonical text, which makes its bytes, and so its sha256, reproducible. Expected validity and
recorded edits are part of the manifest; the Rust test `reference_corpus` checks the corpus in `cargo test`.
"""
import hashlib
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent
FIXTURES = ROOT / "examples" / "fixtures" / "reference"
BINARY = Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / "target" / "debug" / "layout-command"
METRIC_VERSION = 3


def call(request):
    out = subprocess.run([str(BINARY)], input=json.dumps(request), text=True, capture_output=True)
    if out.returncode:
        raise SystemExit(f"layout-command failed: {out.stderr[-300:]}")
    reply = json.loads(out.stdout)
    if not reply.get("ok"):
        raise SystemExit(f"{request['op']}: {reply.get('error')}")
    return reply


def graph(name):
    return json.loads((FIXTURES / f"{name}.graph.json").read_text())


def snapshot_of(name, pins=None):
    imported = call({"op": "import", "graph": graph(name), "limit": 40})
    snapshot = imported["snapshot"]
    if pins:
        configured = call({
            "op": "configure",
            "snapshot": snapshot,
            "expectedRevision": snapshot["revision"],
            "expectedLayoutVersion": snapshot["layoutVersion"],
            "options": {"pins": pins},
        })
        snapshot = configured["snapshot"]
    return snapshot


def reference_of(snapshot, reference_id):
    return call({"op": "referenceCreate", "snapshot": snapshot, "referenceId": reference_id})["reference"]


CASES = [
    {
        "id": "ports-fanout",
        "tags": ["positive", "unedited"],
        "expectValid": True,
        "reference": lambda: reference_of(snapshot_of("ports-fanout"), "ports-fanout"),
    },
    {
        "id": "cycle-selfloop",
        "tags": ["positive", "unedited", "cycles", "self-loops"],
        "expectValid": True,
        "reference": lambda: reference_of(snapshot_of("cycle-selfloop"), "cycle-selfloop"),
    },
    {
        "id": "container-boundary",
        "tags": ["positive", "unedited", "containers"],
        "expectValid": True,
        "reference": lambda: reference_of(snapshot_of("container-boundary"), "container-boundary"),
    },
    {
        "id": "container-boundary-pinned",
        "tags": ["positive", "constrained", "containers"],
        "expectValid": True,
        "pins": ["x"],
        "reference": lambda: reference_of(
            snapshot_of("container-boundary", {"x": {"x": 20.0, "y": 40.0}}), "container-boundary-pinned"
        ),
    },
    {
        "id": "container-boundary-repair",
        "tags": ["positive", "repair", "containers"],
        "expectValid": True,
        "edits": [{"nodeId": "x", "dx": 40.0, "dy": 0.0}],
        "reference": lambda: reference_of(snapshot_of("container-boundary"), "container-boundary-repair"),
    },
    {
        "id": "state-tcp-moved",
        "tags": ["negative", "draft", "user-draft"],
        "expectValid": False,
        "copy": FIXTURES / "state-tcp-moved.lwref.json",
    },
]


def main():
    entries = []
    for case in CASES:
        folder = HERE / case["id"]
        folder.mkdir(exist_ok=True)
        target = folder / "reference.lwref.json"
        if "copy" in case:
            document = json.loads(case["copy"].read_text())
            text = call({"op": "referenceCanonical", "reference": document})["canonical"]
        else:
            document = case["reference"]()
            text = call({"op": "referenceCanonical", "reference": document})["canonical"]
        target.write_text(text)
        reference = json.loads(text)
        entry = {
            "id": case["id"],
            "file": f"{case['id']}/reference.lwref.json",
            "sha256": hashlib.sha256(text.encode()).hexdigest(),
            "referenceId": reference["referenceId"],
            "inputHash": reference["inputHash"],
            "payloadHash": reference["integrity"]["payloadHash"],
            "status": reference["status"],
            "tags": case["tags"],
            "expectValid": case["expectValid"],
        }
        for key in ("pins", "edits"):
            if key in case:
                entry[key] = case[key]
        entries.append(entry)
    manifest = {
        "format": "layerweave-reference-corpus",
        "version": 1,
        "metricVersion": METRIC_VERSION,
        "cases": entries,
    }
    (HERE / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(f"wrote {len(entries)} cases")


if __name__ == "__main__":
    main()
