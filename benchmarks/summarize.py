#!/usr/bin/env python3
"""Print a Markdown table for a results JSONL file written by benchmarks/run.sh."""
import json
import sys

rows = [json.loads(line) for line in open(sys.argv[1]) if line.strip()]
print("| dataset | limit | view (nodes/edges) | import ms | refine ms (threads: median) | speedup max/1 | edit ms | snapshot KB | output identical |")
print("|---|---|---|---|---|---|---|---|---|")
for r in rows:
    if "status" in r:
        print(f"| {r['dataset']} | {r['limit']} | {r['status']} | | | | | | |")
        continue
    refine = r["refineMs"]
    parts = ", ".join(f"{t}: {v['medianMs']}" for t, v in refine.items()) or "not measured"
    speedup = "" 
    if len(refine) >= 2:
        keys = sorted(refine, key=int)
        speedup = f"{refine[keys[0]]['medianMs'] / refine[keys[-1]]['medianMs']:.2f}×"
    print(f"| {r['dataset']} | {r['limit']} | {r['viewNodes']}/{r['viewEdges']} | {r['importMs']['medianMs']} | {parts} | {speedup} | {r['editMs']['medianMs']} | {r['snapshotBytes'] / 1024:.0f} | {'yes' if r['threadsIdentical'] else 'no'} |")
env = rows[0]["environment"] if rows and "environment" in rows[0] else {}
print()
print(f"Machine: {env.get('cpuModel')} · {env.get('logicalCpus')} logical CPUs · {env.get('os')}/{env.get('arch')} · {env.get('rustc')} · {env.get('profile')} · git {env.get('gitRevision')}{' (dirty)' if env.get('gitDirty') else ''}")
