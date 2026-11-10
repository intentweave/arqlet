#!/usr/bin/env python3
"""Compares two benchmark result files configuration by configuration and says why a fingerprint changed.

  python3 benchmarks/fingerprint_compare.py OLD.jsonl NEW.jsonl

Each configuration is classified as one of:
  identical            every fingerprint matches
  metadata only        geometryFingerprint matches; only the full snapshot differs (counters, labels, strategy)
  geometry changed     boxes or routes differ
  input changed        the graph size differs (the input is not the same graph)
  not comparable       one side has no geometryFingerprint (recorded before this field existed)
Changes are never merged into one verdict, so a metadata change cannot hide a geometry change.
"""
import json
import sys
from pathlib import Path


def load(path):
    rows = {}
    for line in Path(path).read_text().splitlines():
        if not line.strip():
            continue
        row = json.loads(line)
        if "refineFingerprint" not in row:
            continue
        rows.setdefault((row["dataset"], row["limit"]), row)
    return rows


def classify(old, new):
    if (old.get("graphNodes"), old.get("graphEdges")) != (new.get("graphNodes"), new.get("graphEdges")):
        return "input changed"
    if old.get("geometryFingerprint") is None or new.get("geometryFingerprint") is None:
        return "not comparable" if old.get("refineFingerprint") != new.get("refineFingerprint") else "identical"
    if old["geometryFingerprint"] != new["geometryFingerprint"]:
        return "geometry changed"
    if old["refineFingerprint"] != new["refineFingerprint"]:
        return "metadata only"
    return "identical"


def main():
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    old, new = load(sys.argv[1]), load(sys.argv[2])
    counts = {}
    for key in sorted(set(old) & set(new)):
        verdict = classify(old[key], new[key])
        counts[verdict] = counts.get(verdict, 0) + 1
        detail = ""
        if verdict == "metadata only":
            detail = f"candidatesEvaluated {old[key]['optimization']['candidatesEvaluated']} -> {new[key]['optimization']['candidatesEvaluated']}, strategy {old[key]['optimization']['strategy']} -> {new[key]['optimization']['strategy']}"
        print(f"{key[0]:20} {key[1]:>4}  {verdict:16} {detail}")
    print("summary:", ", ".join(f"{k} {v}" for k, v in sorted(counts.items())))
    only = (set(old) ^ set(new))
    if only:
        print("not in both files:", sorted(only))


if __name__ == "__main__":
    main()
