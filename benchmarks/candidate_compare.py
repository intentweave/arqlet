#!/usr/bin/env python3
"""Compares the published conflict key with the shadow tier order over every pair of distinct candidates.

  python3 benchmarks/candidate_compare.py [CANDIDATE_DIR] [SPLIT.json]

Reads the candidate logs written by `bench --candidate-log` (one `<name>-<limit>.jsonl` per view) and the fixed
tune/judge split in `benchmarks/calibration-split.json`. For each pair of measured candidates with different keys it
says whether the tier order (fewer hard violations first, then the larger shadow total) agrees with the key order, and
which key objective decided the key order. It reports disagreements with the geometry files for visual checks.

Nothing is fitted: weights and scales are not changed here, and the key is a comparison basis, not the answer.
"""
import itertools
import json
import sys
from collections import Counter
from pathlib import Path

OBJECTIVES = [
    "siblingOverlaps",
    "edgeNodeHits",
    "sharedRoutePairs",
    "nearNodeSegments",
    "routedCrossings",
    "projectedCrossings",
    "bends",
    "cost",
]


def load_view(path):
    candidates = {}
    for line in path.read_text().splitlines():
        row = json.loads(line)
        if row["outcome"] != "Measured" or row["duplicateOf"] is not None:
            continue
        candidates[row["index"]] = row
    return candidates


def key_order(a, b):
    """-1 when `a` ranks above `b` by key (lexicographically smaller is better), 1 when below, 0 when equal."""
    return (a["key"] > b["key"]) - (a["key"] < b["key"])


def tier_order(a, b):
    """-1 when `a` ranks above `b` by tier: fewer hard violations, then the larger shadow total."""
    if a["hardViolations"] != b["hardViolations"]:
        return -1 if a["hardViolations"] < b["hardViolations"] else 1
    return (a["shadowTotal"] < b["shadowTotal"]) - (a["shadowTotal"] > b["shadowTotal"])


def deciding_objective(a, b):
    for i, (x, y) in enumerate(zip(a["key"], b["key"])):
        if x != y:
            return OBJECTIVES[i] if i < len(OBJECTIVES) else f"#{i}"
    return "-"


def main():
    directory = Path(sys.argv[1] if len(sys.argv) > 1 else "benchmarks/results/candidates")
    split_path = Path(sys.argv[2] if len(sys.argv) > 2 else "benchmarks/calibration-split.json")
    split = json.loads(split_path.read_text())
    side = {view: "tune" for view in split["tune"]}
    side.update({view: "judge" for view in split["judge"]})

    totals = {"tune": Counter(), "judge": Counter()}
    disagreements = []
    for path in sorted(directory.glob("*.jsonl")):
        name, limit = path.stem.rsplit("-", 1)
        view = f"{name} {limit}"
        if view not in side:
            print(f"warning: {view} is not in the split", file=sys.stderr)
            continue
        candidates = load_view(path)
        for i, j in itertools.combinations(sorted(candidates), 2):
            a, b = candidates[i], candidates[j]
            if a["key"] == b["key"]:
                continue
            totals[side[view]]["pairs"] += 1
            key = key_order(a, b)
            tier = tier_order(a, b)
            if tier in (0, key):
                totals[side[view]]["agree"] += 1
            else:
                totals[side[view]]["disagree"] += 1
                disagreements.append((side[view], view, a, b, deciding_objective(a, b), key, tier))

    for s in ("tune", "judge"):
        t = totals[s]
        print(f"{s}: pairs {t['pairs']}, agree {t['agree']}, disagree {t['disagree']}")
    print()
    print("disagreements (side, view, candidate pair, deciding key objective, key order, tier order):")
    for side_name, view, a, b, objective, key, tier in disagreements:
        print(
            f"{side_name:5} {view:22} {a['index']:>3} {a['strategy']} vs {b['index']:>3} {b['strategy']}"
            f"  key decided by {objective}  key {key:+d}  tier {tier:+d}"
            f"  total {a['shadowTotal']} vs {b['shadowTotal']}"
            f"  geometry {a['geometryFile']} / {b['geometryFile']}"
        )


if __name__ == "__main__":
    main()
