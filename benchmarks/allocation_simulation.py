#!/usr/bin/env python3
"""Simulates first-stage job allocation under a job budget, from candidate logs.

  python3 benchmarks/allocation_simulation.py CANDIDATE_DIR [SPLIT.json]

For each budget k, it asks whether the first k jobs reach the best first-stage key that all jobs reach. Two orders are
compared: the planned order, and a learned order. The learned order ranks each job by how often it gave the best
first-stage candidate on the tune views (`benchmarks/calibration-split.json`). The result is judged on the judge views
only. Work counters are not used: under the default engine they are zero for first-stage jobs, so no deterministic cost
signal exists yet.
"""
import collections
import glob
import json
import sys


def load_views(directory):
    views = {}
    for path in sorted(glob.glob(f"{directory}/*.jsonl")):
        name, limit = path.rsplit("/", 1)[1][:-len(".jsonl")].rsplit("-", 1)
        rows = [json.loads(line) for line in open(path)]
        jobs = [r for r in rows if r["phase"] == "candidate" and r["id"] is not None]
        views[f"{name} {limit}"] = sorted(jobs, key=lambda r: r["index"])
    return views


def job_name(row):
    return f'{row["id"]["strategy"]}{row["id"]["parameter"]}'


def best_within(jobs, k):
    keys = [r["key"] for r in jobs[:k] if r["outcome"] == "Measured"]
    return min(keys) if keys else None


def main():
    directory = sys.argv[1]
    split_path = sys.argv[2] if len(sys.argv) > 2 else "benchmarks/calibration-split.json"
    split = json.load(open(split_path))
    tune, judge = set(split["tune"]), set(split["judge"])
    views = load_views(directory)

    wins, seen = collections.Counter(), collections.Counter()
    for view in tune:
        jobs = views[view]
        keys = [r["key"] for r in jobs if r["outcome"] == "Measured"]
        if not keys:
            continue
        best = min(keys)
        for r in jobs:
            seen[job_name(r)] += 1
            if r["outcome"] == "Measured" and r["key"] == best:
                wins[job_name(r)] += 1
    rate = {job: wins[job] / seen[job] for job in seen}
    print("win rate on tune views:", {job: round(rate[job], 2) for job in sorted(rate)})

    def learned(jobs):
        return sorted(jobs, key=lambda r: (-rate.get(job_name(r), 0.), r["index"]))

    def cost(row):
        # Deterministic legacy work (candidate evaluations plus grid pops); absent in logs from before the counters.
        work = row["work"]
        return work.get("legacyCandidates", 0) + work.get("legacyGridPops", 0)

    def learned_per_cost(jobs):
        # Win rate divided by cost relative to the cheapest job of the same view. Cost only reorders when it differs.
        lowest = min(cost(r) for r in jobs) or 1
        return sorted(jobs, key=lambda r: (-(rate.get(job_name(r), 0.) + 1e-9) / (cost(r) / lowest), r["index"]))

    for k in range(1, 5):
        hits = collections.Counter()
        total = 0
        for view in judge:
            jobs = views[view]
            full = best_within(jobs, len(jobs))
            if full is None:
                continue
            total += 1
            hits["planned"] += best_within(jobs, k) == full
            hits["learned"] += best_within(learned(jobs), k) == full
            hits["learned per cost"] += best_within(learned_per_cost(jobs), k) == full
        print(f"budget {k}, judge views reaching the full first-stage best of {total}: "
              + ", ".join(f"{name} {count}" for name, count in hits.items()))


if __name__ == "__main__":
    main()
