#!/usr/bin/env python3
"""Byte identity of session answers between two `layout-command` builds (#261): the same sequence of reveal (partial and
whole), reveal more, toggle and filter messages in one session on a generated graph, under several first views; every
answer line must be equal. Exit status 1 on the first difference.

    python3 benchmarks/scaling/reveal_identity.py BEFORE_BINARY AFTER_BINARY [--nodes 20000]
"""
import argparse, json, subprocess, sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from series import generate  # noqa: E402

VIEWS = {
    "budget": {"levelOfDetail": {"nodes": 1000, "edges": 3000}, "liftEdges": True},
    "depth": {"levelOfDetail": {"depth": 1}},
    "communities": {"communities": 40, "levelOfDetail": {"depth": 0}},
    "hubs": {"communities": 40, "communityHubs": {"top": 0.05}, "levelOfDetail": {"depth": 0}},
    "budget-communities": {"levelOfDetail": {"nodes": 300, "edges": 1000}},
}


def session(binary, messages):
    lines = [json.dumps({"session": "open"})] + [
        '{"session":1,"request":' + m + "}" for m in messages]
    proc = subprocess.run([binary, "--sessions"], input="\n".join(lines) + "\n", capture_output=True, text=True,
                          timeout=900)
    return proc.stdout.splitlines()[1:]


def main():
    p = argparse.ArgumentParser()
    p.add_argument("before")
    p.add_argument("after")
    p.add_argument("--nodes", type=int, default=20000)
    a = p.parse_args()
    graph = generate(a.nodes, 1)
    parent = {n["id"]: n.get("parentId") for n in graph["nodes"]}
    kids = {}
    for n in graph["nodes"]:
        kids.setdefault(n.get("parentId"), []).append(n["id"])
    symbols = [n["id"] for n in graph["nodes"] if n.get("kind") == "symbol"]
    containers = [c for c in kids if c and len(kids[c]) > 3]
    picks = symbols[:: max(1, len(symbols) // 7)][:7]
    steps = [{"op": "reveal", "nodeIds": [picks[0]]}, {"op": "reveal", "nodeIds": picks[1:3]},
             {"op": "reveal", "more": {"container": parent[picks[0]], "count": 3}},
             {"op": "toggle", "nodeId": parent[picks[0]]},
             {"op": "reveal", "nodeIds": [picks[3]]},
             {"op": "filter", "hide": {"nodeKinds": ["symbol"], "compact": True}},
             {"op": "reveal", "nodeIds": [picks[4]]},
             {"op": "filter", "hide": {}},
             {"op": "reveal", "nodeIds": [picks[5]], "partial": False},
             {"op": "reveal", "nodeIds": [picks[6]]},
             {"op": "reveal", "more": {"container": parent[picks[6]], "count": 1000}}]
    if containers:
        steps.insert(3, {"op": "toggle", "nodeId": containers[len(containers) // 2]})
    bad = 0
    for name, view in VIEWS.items():
        first = json.dumps({"op": "import", "limit": 10000, "options": view, "graph": graph})
        lines = [first] + [json.dumps(s) for s in steps]
        before, after = session(a.before, lines), session(a.after, lines)
        same = before == after
        ok = sum(1 for l in after if json.loads(l).get("ok"))
        print(f"{name}: {len(after)} answers ({ok} ok), {'identical' if same else 'DIFFERENT'}")
        if not same:
            bad = 1
            for i, (x, y) in enumerate(zip(before, after)):
                if x != y:
                    print(f"  first difference at message {i}: {steps[i - 1] if i else 'import'}")
                    break
    sys.exit(bad)


main()
