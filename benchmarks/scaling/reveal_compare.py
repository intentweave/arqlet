#!/usr/bin/env python3
"""Median seconds of toggle and reveal per size, before and after (#261), from two `throughput.py` result files, as a
Markdown table with the range of repetitions and of the 1-minute load before the rows.

    python3 benchmarks/scaling/reveal_compare.py BEFORE.jsonl AFTER.jsonl
"""
import json, statistics, sys
from collections import defaultdict

MESSAGES = ["toggle-open", "toggle-close", "reveal"]


def rows(path):
    done = defaultdict(set)
    out = defaultdict(list)
    for line in open(path):
        r = json.loads(line)
        if r.get("message") == "repDone":
            done[r["size"]].add(r["rep"])
    for line in open(path):
        r = json.loads(line)
        if r.get("mode") == "session" and r.get("ok") and r.get("message") in MESSAGES and r["rep"] in done[r["size"]]:
            out[(r["size"], r["message"])].append(r)
    return out


def cell(rs):
    s = [r["seconds"] for r in rs]
    load = [r["load"][0] for r in rs]
    return f"{statistics.median(s):.2f} ({min(s):.2f}-{max(s):.2f}) · {min(load):.1f}-{max(load):.1f}", statistics.median(s)


before, after = rows(sys.argv[1]), rows(sys.argv[2])
print("| nodes | message | before: s (min-max) · load | after: s (min-max) · load | after / before |")
print("|---|---|---|---|---|")
for size in sorted({k[0] for k in before}):
    for m in MESSAGES:
        if (size, m) in before and (size, m) in after:
            (b, bm), (a, am) = cell(before[(size, m)]), cell(after[(size, m)])
            print(f"| {size:,} | {m} | {b} | {a} | {am / bm:.2f} |")
