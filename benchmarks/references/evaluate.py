"""Evaluates the curated reference corpus in its three modes and writes a results file.

usage: python3 benchmarks/references/evaluate.py [path/to/layout-command]

Modes (curated-references design, section 9.1):
- validation: the stored document is valid under the current engine and metric version;
- constrained: the engine lays out the input with the curator's pins; the pins must be honoured;
- unconstrained: the same input without pins, measured like the reference;
- repair: the recorded edits are applied, the affected routes rerouted, and the result measured.

Results contain quality metrics only; no timings are recorded here, so no performance claim follows from them.
The file is written to benchmarks/references/results/ with the engine binary hash and the git revision.
"""
import datetime
import hashlib
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent
BINARY = Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / "target" / "debug" / "layout-command"


def call(request):
    out = subprocess.run([str(BINARY)], input=json.dumps(request), text=True, capture_output=True)
    if out.returncode:
        raise SystemExit(f"layout-command failed: {out.stderr[-300:]}")
    return json.loads(out.stdout)


def revision():
    head = subprocess.run(["git", "rev-parse", "--short", "HEAD"], cwd=ROOT, text=True, capture_output=True)
    dirty = subprocess.run(["git", "status", "--porcelain"], cwd=ROOT, text=True, capture_output=True)
    return head.stdout.strip() + (" (uncommitted changes)" if dirty.stdout.strip() else "")


def deltas(reference, measured):
    """Numeric metric differences measured minus reference. Modes are reported separately, never summed."""
    return {
        key: measured[key] - value
        for key, value in reference.items()
        if isinstance(value, (int, float)) and not isinstance(value, bool) and isinstance(measured.get(key), (int, float))
    }


def evaluate(case, document):
    entry = {"id": case["id"], "expectValid": case["expectValid"], "tags": case["tags"]}
    validation = call({"op": "referenceValidate", "reference": document})
    if not validation.get("ok"):
        entry["validation"] = {"ok": False, "error": validation.get("error")}
        return entry
    report = validation["report"]
    entry["validation"] = {
        "valid": report["valid"],
        "hardViolations": report["hardViolations"],
        "pendingRoutes": report["pendingRoutes"],
        "metrics": report["metrics"],
    }
    unconstrained = call({"op": "referenceReproduce", "mode": "unconstrained", "reference": document})
    if unconstrained.get("ok"):
        entry["unconstrained"] = unconstrained["candidate"]
        entry["unconstrained"]["deltaVsCurated"] = deltas(report["metrics"], unconstrained["candidate"]["metrics"])
    else:
        entry["unconstrained"] = {"error": unconstrained.get("error")}
    if case.get("pins"):
        constrained = call({"op": "referenceReproduce", "mode": "constrained", "reference": document})
        if constrained.get("ok"):
            entry["constrained"] = {
                "pins": constrained["pins"],
                "candidate": constrained["candidate"],
                "deltaVsCurated": deltas(report["metrics"], constrained["candidate"]["metrics"]),
            }
        else:
            entry["constrained"] = {"error": constrained.get("error")}
    if case.get("edits"):
        repaired = call({"op": "referenceRepair", "reference": document, "edits": case["edits"]})
        if repaired.get("ok"):
            entry["repair"] = {key: repaired[key] for key in ("before", "after", "pendingRoutes", "movedElsewhere")}
            entry["repair"]["deltaVsBefore"] = deltas(repaired["before"]["metrics"], repaired["after"]["metrics"])
        else:
            entry["repair"] = {"error": repaired.get("error")}
    return entry


def main():
    manifest = json.loads((HERE / "manifest.json").read_text())
    binary_hash = hashlib.sha256(BINARY.read_bytes()).hexdigest()
    results = []
    for case in manifest["cases"]:
        document = json.loads((HERE / case["file"]).read_text())
        results.append(evaluate(case, document))
    stamp = datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    output = {
        "stamp": stamp,
        "engine": {"binary": str(BINARY.relative_to(ROOT)), "binarySha256": binary_hash, "revision": revision()},
        "metricVersion": manifest["metricVersion"],
        "cases": results,
    }
    folder = HERE / "results"
    folder.mkdir(exist_ok=True)
    target = folder / f"evaluation-{stamp}.json"
    target.write_text(json.dumps(output, indent=2) + "\n")
    for entry in results:
        valid = entry.get("validation", {}).get("valid")
        print(f"{entry['id']}: valid={valid} expected={entry['expectValid']}")
    print(f"wrote {target.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
