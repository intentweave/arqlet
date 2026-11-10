#!/usr/bin/env python3
"""Summarize an option-grid JSONL: mean metrics per variant, and wins against the default variant."""
import json
import statistics
import sys
from collections import defaultdict

rows = [json.loads(line) for line in open(sys.argv[1]) if line.strip()]
variants = list(dict.fromkeys(r["variant"] for r in rows))
by = defaultdict(dict)
for r in rows:
    by[r["variant"]][r["dataset"]] = r
base = by["default"]

print("| variant | crossings | node hits | length | area | refine ms (mean) | refine ms (total) | better crossings than default | fewer node hits | zero-hit views |")
print("|---|---|---|---|---|---|---|---|---|---|")
for v in variants:
    data = by[v]
    common = [d for d in data if d in base]
    mean = lambda k: statistics.fmean(data[d][k] for d in common)
    better_cross = sum(1 for d in common if data[d]["crossings"] < base[d]["crossings"])
    fewer_hits = sum(1 for d in common if data[d]["lineNodeHits"] < base[d]["lineNodeHits"])
    zero = sum(1 for d in common if data[d]["lineNodeHits"] == 0)
    total = sum(data[d]["refineMs"] for d in common)
    print(f"| {v} | {mean('crossings'):.1f} | {mean('lineNodeHits'):.1f} | {mean('length'):,.0f} | {mean('area'):,.0f} | {mean('refineMs'):.1f} | {total:,.0f} | {better_cross}/{len(common)} | {fewer_hits}/{len(common)} | {zero}/{len(common)} |")
