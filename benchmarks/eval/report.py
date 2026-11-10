#!/usr/bin/env python3
"""Markdown tables from an eval JSONL written by compare.mjs: per-dataset rows and per-engine means."""
import json
import statistics
import sys
from collections import defaultdict

rows = [json.loads(line) for line in open(sys.argv[1]) if line.strip()]
failures = [r for r in rows if "failed" in r]
rows = [r for r in rows if "failed" not in r]
engines = list(dict.fromkeys(r["engine"] for r in rows))
datasets = list(dict.fromkeys(r["dataset"] for r in rows))
metrics = [("crossings", "crossings"), ("lineNodeHits", "line–node hits"), ("siblingOverlaps", "sibling overlaps"),
           ("crossBranchOverlaps", "cross-branch overlaps"), ("sharedLinePairs", "shared lines"), ("hugPairs", "border hugging"),
           ("bends", "bends"), ("area", "area"), ("length", "length"), ("layoutMs", "layout ms")]

print("## Per dataset\n")
print("| dataset | view nodes/edges | engine | " + " | ".join(label for _, label in metrics) + " |")
print("|---|---|---|" + "---|" * len(metrics))
for name in datasets:
    for row in [r for r in rows if r["dataset"] == name]:
        values = [f"{row[k]:g}" if isinstance(row[k], (int, float)) else str(row[k]) for k, _ in metrics]
        print(f"| {name} | {row['viewNodes']}/{row['viewEdges']} | {row['engine']} | " + " | ".join(values) + " |")

print("\n## Mean over datasets\n")
print("| engine | datasets | " + " | ".join(label for _, label in metrics) + " | datasets with zero edge–node hits |")
print("|---|---|" + "---|" * len(metrics) + "---|")
for engine in engines:
    subset = [r for r in rows if r["engine"] == engine]
    means = [statistics.fmean(r[k] for r in subset) for k, _ in metrics]
    zero = sum(1 for r in subset if r["lineNodeHits"] == 0)
    print(f"| {engine} | {len(subset)} | " + " | ".join(f"{m:.1f}" for m in means) + f" | {zero}/{len(subset)} |")

if failures:
    print("\n## Engine failures on this input\n")
    print("| dataset | engine | error |")
    print("|---|---|---|")
    for r in failures:
        print(f"| {r['dataset']} | {r['engine']} | {r['failed']} |")
env = rows[0]["environment"]
print(f"\nMachine: {env.get('cpuModel')} · node {env.get('node')} · layout-command release build.")
