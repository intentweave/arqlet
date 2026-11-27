#!/usr/bin/env python3
"""Table of `overlap-index.mjs` rows: medians per request and build, the speedup, and whether the answers are identical.

  python3 benchmarks/experiments/overlap_index_table.py ROWS.jsonl
"""
import json
import statistics
import sys
from collections import defaultdict


def main():
    rows = [json.loads(line) for line in open(sys.argv[1]) if line.strip()]
    by = defaultdict(list)
    order = []
    for r in rows:
        if r["request"] not in order:
            order.append(r["request"])
        by[(r["request"], r["build"])].append(r)
    print("| request | runs | second pass base (ms) | second pass new (ms) | speedup | total base (ms) | total new (ms) "
          "| speedup | load (1 min) | answers identical |")
    print("|---|---|---|---|---|---|---|---|---|---|")
    for request in order:
        base, new = by[(request, "base")], by[(request, "new")]
        med = lambda rs, k: statistics.median(r[k] for r in rs) if rs and all(r.get(k) is not None for r in rs) else None
        fmt = lambda v: "-" if v is None else f"{v:,.0f}".replace(",", " ")
        sp_b, sp_n = med(base, "secondPassMs"), med(new, "secondPassMs")
        t_b, t_n = med(base, "totalMs"), med(new, "totalMs")
        ratio = lambda a, b: "-" if a is None or b is None or b == 0 else f"{a / b:.2f}"
        loads = [r["load"] for r in base + new]
        answers = {r["answer"] for r in base + new}
        print(f"| {request} | {len(base)}+{len(new)} | {fmt(sp_b)} | {fmt(sp_n)} | {ratio(sp_b, sp_n)} | {fmt(t_b)} | "
              f"{fmt(t_n)} | {ratio(t_b, t_n)} | {min(loads):.1f}-{max(loads):.1f} | "
              f"{'yes' if len(answers) == 1 else 'NO'} |")


if __name__ == "__main__":
    main()
