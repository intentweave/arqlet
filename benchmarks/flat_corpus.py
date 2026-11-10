"""Flat-region candidate on the frozen corpus: baseline (lesbar) against the flat option, same metric, same budgets.

For each input: containers present, skeleton biconnected (the candidate's precondition), metrics of both results,
strategy, run time and box aspect ratio. Inputs outside the precondition are reported as such, not as failures.
Writes benchmarks/results/flat-corpus-<timestamp>.json.

    python3 benchmarks/flat_corpus.py     (needs target/release/layout-command)
"""
import datetime
import json
import os
import sys
import time

sys.path.insert(0, os.path.dirname(__file__))
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__))))
import flat_length as fl  # noqa: E402

INPUTS = ["state-tcp", "workflow-dag", "network-vlan", "uml-class", "bpmn-pools", "org-chart",
          "layerweave-code", "synthetic-2k"]


def precondition(graph):
    """Containers present, and whether the simple skeleton (targets only, loops and parallels dropped) is biconnected."""
    nested = any(n.get("parentId") for n in graph["nodes"])
    ids = [n["id"] for n in graph["nodes"]]
    index = {v: i for i, v in enumerate(ids)}
    adj = {i: set() for i in range(len(ids))}
    for e in graph["edges"]:
        t = e.get("target")
        if t and t != e["source"]:
            a, b = index[e["source"]], index[t]
            adj[a].add(b)
            adj[b].add(a)

    def connected(skip):
        vertices = [v for v in adj if v != skip]
        if not vertices:
            return True
        seen, stack = {vertices[0]}, [vertices[0]]
        while stack:
            v = stack.pop()
            for u in adj[v]:
                if u != skip and u not in seen:
                    seen.add(u)
                    stack.append(u)
        return len(seen) == len(vertices)

    biconnected = len(ids) >= 3 and connected(None) and all(connected(v) for v in adj)
    return {"containers": nested, "biconnected": biconnected}


def aspect(snap):
    boxes = snap["boxes"]
    w = max(b["x"] + b["width"] for b in boxes.values()) - min(b["x"] for b in boxes.values())
    h = max(b["y"] + b["height"] for b in boxes.values()) - min(b["y"] for b in boxes.values())
    return round(w, 1), round(h, 1), round(w / h, 2) if h else None


def metrics_of(snap):
    m = fl.measure(snap)
    return {"width": m["width"], "height": m["height"], "aspect": aspect(snap)[2],
            "total_routed": m["total_routed"], "routes": len(m["rows"])}


def metrics(drawing):
    import subprocess
    out = subprocess.run([fl.BIN], input=json.dumps({"op": "metrics", "drawing": drawing}), capture_output=True,
                         text=True, timeout=1800).stdout
    reply = json.loads(out)
    if not reply.get("ok"):
        raise SystemExit("metrics failed: " + str(reply.get("error")))
    return reply["metrics"]


def run_input(name):
    graph = json.load(open(os.path.join(fl.ROOT, "benchmarks", "inputs", f"{name}.json")))
    preset = json.load(open(os.path.join(fl.ROOT, "presets", "lesbar.json")))["configSet"]
    snap = fl.run({"op": "import", "graph": graph, "limit": 100})
    conf = fl.run({"op": "configure", "snapshot": snap, "expectedRevision": snap["revision"],
                   "expectedLayoutVersion": snap["layoutVersion"], "options": {"configSet": preset}})
    t0 = time.time()
    base = fl.refined(conf)
    t_base = time.time() - t0
    t0 = time.time()
    flat = fl.refined(conf, {"flatRegion": True})
    t_flat = time.time() - t0
    import profile_runner
    base_metrics = metrics(profile_runner.drawing_of(base))
    flat_metrics = metrics(profile_runner.drawing_of(flat))
    return {"input": name, "precondition": precondition(graph),
            "baseline": {"strategy": base["optimization"]["strategy"], "seconds": round(t_base, 2),
                         "hits": base_metrics["lineNodeHits"] + base_metrics["containerTraversals"],
                         "shared": base_metrics["sharedLinePairs"], "hug": base_metrics["hugPairs"],
                         "cross": base_metrics["crossingPairs"] + base_metrics["sharedEndpointCrossingPairs"],
                         "bends": base_metrics["bends"], "length": round(base_metrics["length"], 1),
                         "area": round(base_metrics["area"], 1), **metrics_of(base)},
            "flat": {"strategy": flat["optimization"]["strategy"], "seconds": round(t_flat, 2),
                     "hits": flat_metrics["lineNodeHits"] + flat_metrics["containerTraversals"],
                     "shared": flat_metrics["sharedLinePairs"], "hug": flat_metrics["hugPairs"],
                     "cross": flat_metrics["crossingPairs"] + flat_metrics["sharedEndpointCrossingPairs"],
                     "bends": flat_metrics["bends"], "length": round(flat_metrics["length"], 1),
                     "area": round(flat_metrics["area"], 1), **metrics_of(flat)}}


def main():
    rows = []
    for name in INPUTS:
        row = run_input(name)
        rows.append(row)
        b, f, p = row["baseline"], row["flat"], row["precondition"]
        print(f"{name:16} containers={p['containers']!s:5} biconnected={p['biconnected']!s:5} "
              f"base len {b['length']:>9} cross {b['cross']} hug {b['hug']} | "
              f"flat {f['strategy']:14} len {f['length']:>9} cross {f['cross']} hug {f['hug']} "
              f"aspect {f['aspect']} {f['seconds']}s", flush=True)
    report = {"machine": fl.machine_line(), "date": datetime.date.today().isoformat(), "profile": "lesbar",
              "candidate": "flatRegion", "rows": rows}
    path = os.path.join(fl.ROOT, "benchmarks", "results",
                        f"flat-corpus-{datetime.datetime.now().strftime('%Y%m%dT%H%M%SZ')}.json")
    json.dump(report, open(path, "w"), indent=1)
    print("written", path, file=sys.stderr)


if __name__ == "__main__":
    main()
