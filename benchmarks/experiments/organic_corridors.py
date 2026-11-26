#!/usr/bin/env python3
"""Step C1 of the organic corridors (#156, ADR 0020 amendment): does the planned demand predict where the routes of the
current pipeline go through boxes? Standard library only; needs `node` for the generated views and the datasets of
`python3 benchmarks/generate.py --no-manifest`.

Each view is laid out twice by `layout-command`, without and with `organicCorridors`; the two drawings must be equal apart
from the report. "Today" is the drawing without the option; the prediction comes from the report alone (and, for the
baselines, from the boxes and edges, without a plan).

The ADR names no threshold for C1. The reading below was fixed before the first run of this script, over the routes the
router draws (bus taps, which are drawn from the bus bars afterwards, are reported apart). A pair is (edge, box of the
root area that holds neither end); it is a hit when the edge's route goes through the box. A corridor is short when its
need is larger than its width.

- R1, where: the hits whose planned way runs beside the box along a short corridor are at least half of all hits, and such
  pairs are hits more often (lift) than pairs whose planned way merely runs beside the box.
- R2, which routes: the best score from the plan's demand separates routes through boxes from the others better (AUC) than
  chance and than the best baseline without a plan.
- R3, which boxes: the same for the boxes that routes go through.

The demand predicts today's hits if R1, R2 and R3 hold.

Usage: organic_corridors.py --bin target/release/layout-command --work DIR --out FILE.jsonl
       [--costs 1] [--views NAME,...] [--timing RUNS]
       organic_corridors.py --summarize FILE.jsonl ...

With a `layout-command` built with `corridor-router-failures.patch` the records also say why routes end up through boxes.
`--today` records only the drawing without the report, to compare binaries (the router without its grid bound).
"""
import argparse
import json
import os
import platform
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
EPS = 1e-6

# (units, buses, children, edges, seed) of bus-mesh.mjs, bus lines as `lines`. The seed chooses the bus memberships only,
# so it makes no second view without buses.
MESH = [
    (100, 0, 60, 250, 7), (100, 30, 60, 250, 7), (100, 30, 120, 500, 7), (100, 30, 240, 1000, 7),
    (100, 30, 0, 0, 7), (100, 0, 120, 500, 7), (100, 0, 240, 1000, 7),
    (100, 30, 60, 250, 11), (100, 30, 60, 250, 13), (100, 30, 120, 500, 11), (100, 30, 120, 500, 13),
    (200, 60, 120, 500, 7), (50, 15, 120, 500, 7),
]
# Organic views of the corpus. As shipped most draw straight edges, which the plan leaves out; here every edge is
# orthogonal (the routing settings of the graph are removed).
CORPUS = [
    "organic-25", "organic-50", "organic-100", "organic-200", "organic-300", "mixed-large", "mixed-layouts",
    "self-loops", "petri-net-shapes", "vehicle-buses-organic", "can-bus",
]
# Large flat regions (the scalable stress), for the time and the sizes.
LARGE = [500, 1000, 2000]


def views():
    out = {}
    for units, buses, children, edges, seed in MESH:
        name = f"mesh-{units}-{buses}-{children}-{edges}-s{seed}"
        out[name] = ("mesh", (units, buses, children, edges, seed))
    for name in CORPUS:
        out[name] = ("corpus", name)
    for n in LARGE:
        out[f"large-{n}"] = ("large", n)
    return out


def request(kind, spec):
    if kind == "mesh":
        units, buses, children, edges, seed = spec
        args = ["node", str(ROOT / "benchmarks/experiments/bus-mesh.mjs"), str(units), str(buses), "organic", "lines",
                str(children), str(edges), str(seed)]
        return json.loads(subprocess.run(args, check=True, capture_output=True, text=True, timeout=120).stdout)
    if kind == "corpus":
        graph = json.loads((ROOT / "benchmarks/datasets" / f"{spec}.json").read_text())
        graph.pop("defaultRouting", None)
        graph.pop("containerRouting", None)
        return {"op": "import", "graph": graph, "limit": 10000, "options": {}}
    sys.path.insert(0, str(ROOT / "benchmarks"))
    import generate

    graph = generate.organic_ladder(spec)
    graph.pop("defaultRouting", None)
    return {"op": "import", "graph": graph, "limit": 10000, "options": {"scalableStress": True}}


def run(binary, body, work, limit):
    """One `layout-command` process: its answer, wall and CPU seconds. A process without CPU time after two minutes is
    hung (seen on the machine the results were recorded on) and is started again."""
    source, target, errors = work / "request.json", work / "answer.json", work / "errors.txt"
    source.write_text(json.dumps(body))
    for attempt in range(3):
        with source.open() as stdin, target.open("w") as stdout, errors.open("w") as stderr:
            started = time.time()
            process = subprocess.Popen([binary], stdin=stdin, stdout=stdout, stderr=stderr)
            checked = False
            while True:
                pid, status, usage = os.wait4(process.pid, os.WNOHANG)
                if pid:
                    process.returncode = os.waitstatus_to_exitcode(status)
                    break
                elapsed = time.time() - started
                if elapsed > 120 and not checked:
                    checked = True
                    cpu = subprocess.run(["ps", "-o", "time=", "-p", str(process.pid)], capture_output=True, text=True,
                                         timeout=20).stdout.strip()
                    if cpu.strip("0:.,") == "":
                        process.kill()
                        os.wait4(process.pid, 0)
                        process.returncode = -9
                        break
                if elapsed > limit:
                    process.kill()
                    os.wait4(process.pid, 0)
                    process.returncode = -9
                    return {"timeout": limit}
                time.sleep(0.02)
        if process.returncode == 0:
            answer = json.loads(target.read_text())
            if not answer.get("ok"):
                return {"error": answer.get("error")}
            return {"snapshot": answer["snapshot"], "wall": time.time() - started,
                    "cpu": usage.ru_utime + usage.ru_stime,
                    "log": [line.split("\t")[1:] for line in errors.read_text().splitlines() if line.startswith("EXP\t")]}
        print(f"  attempt {attempt + 1} ended with {process.returncode}", file=sys.stderr)
    return {"error": "no answer"}


def absolute(snapshot):
    """The drawn boxes in absolute coordinates. A bus drawn as lines has no box here, although the router saw one."""
    parent = {n["id"]: n.get("parentId") for n in snapshot["graph"]["nodes"]}
    boxes = snapshot["boxes"]
    out = {}

    def of(i):
        if i not in out:
            b = boxes[i]
            p = parent.get(i)
            o = of(p) if p else {"x": 0, "y": 0}
            out[i] = {"x": b["x"] + o["x"], "y": b["y"] + o["y"], "width": b["width"], "height": b["height"]}
        return out[i]

    for i in boxes:
        of(i)
    return parent, out


def through(points, b):
    for p, q in zip(points, points[1:]):
        if (max(p["x"], q["x"]) > b["x"] + EPS and min(p["x"], q["x"]) < b["x"] + b["width"] - EPS
                and max(p["y"], q["y"]) > b["y"] + EPS and min(p["y"], q["y"]) < b["y"] + b["height"] - EPS):
            return True
    return False


def centre(b):
    return (b["x"] + b["width"] / 2, b["y"] + b["height"] / 2)


def chord_crosses(p, q, b):
    """Whether the straight segment p-q enters the box (Liang-Barsky)."""
    lo, hi = 0.0, 1.0
    for axis, (start, size) in enumerate(((b["x"], b["width"]), (b["y"], b["height"]))):
        d = q[axis] - p[axis]
        if abs(d) < 1e-12:
            if not start < p[axis] < start + size:
                return False
            continue
        a, c = (start - p[axis]) / d, (start + size - p[axis]) / d
        lo, hi = max(lo, min(a, c)), min(hi, max(a, c))
    return lo < hi


def chord_distance(p, q, c):
    dx, dy = q[0] - p[0], q[1] - p[1]
    length = dx * dx + dy * dy
    t = 0 if length == 0 else max(0, min(1, ((c[0] - p[0]) * dx + (c[1] - p[1]) * dy) / length))
    return ((c[0] - p[0] - t * dx) ** 2 + (c[1] - p[1] - t * dy) ** 2) ** 0.5


def ranks(values):
    order = sorted(range(len(values)), key=values.__getitem__)
    out = [0.0] * len(values)
    i = 0
    while i < len(order):
        j = i
        while j + 1 < len(order) and values[order[j + 1]] == values[order[i]]:
            j += 1
        for k in range(i, j + 1):
            out[order[k]] = (i + j) / 2 + 1
        i = j + 1
    return out


def auc(scores, positive):
    n = sum(positive)
    m = len(positive) - n
    if n == 0 or m == 0:
        return None
    r = ranks(scores)
    return (sum(v for v, p in zip(r, positive) if p) - n * (n + 1) / 2) / (n * m)


def spearman(a, b):
    if len(a) < 3 or len(set(a)) < 2 or len(set(b)) < 2:
        return None
    x, y = ranks(a), ranks(b)
    mx, my = sum(x) / len(x), sum(y) / len(y)
    top = sum((i - mx) * (j - my) for i, j in zip(x, y))
    bottom = (sum((i - mx) ** 2 for i in x) * sum((j - my) ** 2 for j in y)) ** 0.5
    return top / bottom


def width(c):
    return c["rect"]["width" if c["axis"] == "x" else "height"]


def deficit(c):
    """How much a corridor between two boxes lacks; the outer corridors grow with the area."""
    if "before" not in c or "after" not in c:
        return 0.0
    d = c["need"] - width(c)
    return d if d > EPS else 0.0


def percentile(values, share):
    if not values:
        return 0
    s = sorted(values)
    return s[min(len(s) - 1, int(share * len(s)))]


def plan_summary(plan):
    corridors = plan["corridors"]
    inner = [c for c in corridors if "before" in c and "after" in c]
    used = [c for c in inner if c["demand"] > 0]
    short = [c for c in inner if deficit(c) > 0]
    pairs = {}
    for c in inner:
        key = (c["axis"], c["before"], c["after"])
        have = pairs.get(key, (float("inf"), 0, 0))
        pairs[key] = (min(have[0], width(c)), max(have[1], c["need"]), max(have[2], c["demand"]))
    short_pairs = [(w, need, d) for w, need, d in pairs.values() if need > w + EPS]
    systems = {}
    for axis in ("x", "y"):
        s = plan[axis + "System"]
        joint = s["nodePairs"] + s["segmentNode"] + s["segmentPairs"]
        systems[axis] = {
            **s,
            "jointVariables": s["nodeVariables"] + s["segmentVariables"],
            "jointConstraints": joint,
            # `vpsc::project_axis` allows 8,000,000 constraint updates, 4096 sweeps at most.
            "jointSweepBudget": max(1, min(4096, 8_000_000 // max(1, joint))),
            "separationSweepBudget": max(1, min(4096, 8_000_000 // max(1, s["nodePairs"]))),
        }
    return {
        "area": plan["area"],
        "boxes": plan["boxes"],
        "corridors": len(corridors),
        "innerCorridors": len(inner),
        "usedCorridors": len([c for c in corridors if c["demand"] > 0]),
        "junctions": plan["junctions"],
        "routes": len(plan["routes"]),
        "skipped": plan["skipped"],
        "demandMax": max((c["demand"] for c in corridors), default=0),
        "innerDemandMax": max((c["demand"] for c in used), default=0),
        "innerDemandMedian": percentile([c["demand"] for c in used], 0.5),
        "innerDemandP95": percentile([c["demand"] for c in used], 0.95),
        "shortCorridors": len(short),
        "pairs": len(pairs),
        "shortPairs": len(short_pairs),
        "needExtraTotal": sum(need - w for w, need, _ in short_pairs),
        "needExtraMedian": percentile([need - w for w, need, _ in short_pairs], 0.5),
        "needExtraP95": percentile([need - w for w, need, _ in short_pairs], 0.95),
        "needExtraMax": max((need - w for w, need, _ in short_pairs), default=0),
        "gapNeededMax": max((c["need"] for c in used), default=plan["gap"]),
        "length": sum(r["length"] for r in plan["routes"]),
        "turns": sum(r["turns"] for r in plan["routes"]),
        "systems": systems,
    }


def router_failures(request_body, snapshot, log):
    """With the instrumented router of `corridor-router-failures.patch`: why each route that ends up through a box does.
    A line of its log is one repair of a route: its id, the obstacles, whether a way was found, and, when not, whether one
    exists on the grid of the whole view (no bound on the grid's size). The last repair of a route counts."""
    last = {}
    for edge, _, found, unbounded in log:
        last[edge] = (found == "1", unbounded == "1")
    parent, boxes = absolute(snapshot)

    def up(i):
        while i:
            yield i
            i = parent.get(i)

    edges = {e["id"]: e for e in snapshot["graph"]["edges"]}
    taps = {t["edge"]: b for b in request_body["graph"].get("buses", [])
            for t in b.get("taps", [])}
    failed = [e for e, (found, _) in last.items() if not found]
    out = {"repaired": len(last), "failed": len(failed), "failedWayExists": sum(last[e][1] for e in failed),
           "through": {}}
    for route in snapshot["routes"]:
        edge = edges.get(route["id"])
        if edge is None:
            continue
        skip = set(up(edge["source"])) | set(up(edge["target"]))
        if not any(i not in skip and through(route["points"], b) for i, b in boxes.items()):
            continue
        e = route["id"]
        kind = "router"
        ids = [e]
        if e in taps:
            kind = "tap"
            ends = (edge["source"], edge["target"])
            ids = [t["edge"] for t in taps[e]["taps"] if t["node"] in ends or t["edge"] == e]
        records = [last[i] for i in ids if i in last]
        if any(not found and not unbounded for found, unbounded in records):
            why = "noWayOnTheWholeGrid"
        elif any(not found for found, _ in records):
            why = "gridBound"
        elif records:
            why = "changedAfterAWayWasFound"
        else:
            why = "neverRepaired"
        key = f"{kind}:{why}"
        out["through"][key] = out["through"].get(key, 0) + 1
    return out


def measure(request_body, snapshot, plans):
    """Today's routes through boxes against the plan of the root area."""
    parent, boxes = absolute(snapshot)
    roots = sorted(i for i in boxes if not parent.get(i))
    plan = next((p for p in plans if p["area"] == "<root>"), None)

    def up(i):
        while i:
            yield i
            i = parent.get(i)

    def top(i):
        return list(up(i))[-1]

    edges = {e["id"]: e for e in snapshot["graph"]["edges"]}
    taps = {t["edge"]: b for b in request_body["graph"].get("buses", [])
            for t in b.get("taps", [])}
    straight = set(snapshot.get("straightEdges", []))
    routes = [r for r in snapshot["routes"] if r["id"] in edges and r["id"] not in straight]
    total = {"routes": 0, "of": len(routes), "boxHits": 0}
    by_kind = {k: {"routes": 0, "through": 0, "rootHits": 0} for k in ("router", "tap")}
    hit_pairs = {}
    for route in routes:
        edge = edges[route["id"]]
        skip = set(up(edge["source"])) | set(up(edge["target"]))
        found = [i for i, b in boxes.items() if i not in skip and through(route["points"], b)]
        total["routes"] += bool(found)
        total["boxHits"] += len(found)
        kind = by_kind["tap" if route["id"] in taps else "router"]
        kind["routes"] += 1
        kind["through"] += bool(found)
        hits = {i for i in found if not parent.get(i)}
        kind["rootHits"] += len(hits)
        hit_pairs[route["id"]] = hits
    out = {"today": {**total, "byKind": by_kind}}
    if plan is None:
        return out
    corridors = plan["corridors"]
    planned = {r["edge"]: r for r in plan["routes"]}
    # The side each planned end takes against the side today's route leaves its box on (router edges between root boxes).
    sides = {(p["edgeId"], p["endpoint"]): p["side"] for p in snapshot.get("resolvedPorts", [])}
    agree = ends = 0
    for e, r in planned.items():
        if e in taps or e not in edges:
            continue
        for end, key in (("source", "sourceSide"), ("target", "targetSide")):
            if not parent.get(edges[e][end]) and (e, end) in sides:
                ends += 1
                agree += sides[(e, end)] == r[key]
    out["portSides"] = {"ends": ends, "same": agree}
    for kind in ("router", "tap"):
        ids = []
        way = {}
        for route in routes:
            e = route["id"]
            if (e in taps) != (kind == "tap"):
                continue
            if kind == "router":
                if e in planned:
                    ids.append(e)
                    way[e] = planned[e]["corridors"]
                continue
            # A tap is drawn as the member's way to its bar and on along the bar to the head: both planned taps.
            bus = taps[e]
            ends_of = (edges[e]["source"], edges[e]["target"])
            parts = [t["edge"] for t in bus["taps"] if t["node"] in ends_of or t["edge"] == e]
            used = [c for t in parts if t in planned for c in planned[t]["corridors"]]
            if used:
                ids.append(e)
                way[e] = used
        if not ids:
            continue
        ids = [e for e in ids if all(top(edges[e][end]) in boxes for end in ("source", "target"))]
        if not ids:
            continue
        chords = {e: (centre(boxes[top(edges[e]["source"])]), centre(boxes[top(edges[e]["target"])])) for e in ids}
        pair_scores = {"planDeficit": [], "planBeside": [], "chordCrosses": [], "chordNear": []}
        pair_hit = []
        counts = {"hits": 0, "pairs": 0, "short": 0, "shortHits": 0, "beside": 0, "besideHits": 0,
                  "chord": 0, "chordHits": 0}
        crossing = {i: 0 for i in roots}
        chord_boxes = {}
        for e in ids:
            ends_of = {top(edges[e]["source"]), top(edges[e]["target"])}
            beside = {}
            for c in way[e]:
                d = deficit(corridors[c])
                for side in ("before", "after"):
                    if side in corridors[c]:
                        i = corridors[c][side]
                        beside[i] = max(beside.get(i, 0), d)
            p, q = chords[e]
            chord_boxes[e] = 0
            for i in roots:
                if i in ends_of:
                    continue
                hit = i in hit_pairs.get(e, ())
                crossed = chord_crosses(p, q, boxes[i])
                chord_boxes[e] += crossed
                crossing[i] += crossed
                near = i in beside
                short = near and beside[i] > 0
                counts["pairs"] += 1
                counts["hits"] += hit
                counts["beside"] += near
                counts["besideHits"] += near and hit
                counts["short"] += short
                counts["shortHits"] += short and hit
                counts["chord"] += crossed
                counts["chordHits"] += crossed and hit
                pair_hit.append(hit)
                pair_scores["planDeficit"].append(beside.get(i, -1))
                pair_scores["planBeside"].append(int(near))
                pair_scores["chordCrosses"].append(int(crossed))
                pair_scores["chordNear"].append(-chord_distance(p, q, centre(boxes[i])))
        result = {"pairs": counts, "pairAuc": {k: auc(v, pair_hit) for k, v in pair_scores.items()}}
        # Which routes go through a box.
        degree = {}
        for e in edges.values():
            for i in (e["source"], e["target"]):
                degree[top(i)] = degree.get(top(i), 0) + 1
        route_hit = [bool(hit_pairs.get(e)) for e in ids]
        route_scores = {k: [] for k in ("planMaxDeficit", "planSumDeficit", "planShortCorridors", "planMaxDemand",
                                        "baseLength", "baseChordBoxes", "baseEndArea", "baseEndDegree")}
        for e in ids:
            used = [corridors[c] for c in way[e]]
            p, q = chords[e]
            two = [top(edges[e]["source"]), top(edges[e]["target"])]
            route_scores["planMaxDeficit"].append(max((deficit(c) for c in used), default=0))
            route_scores["planSumDeficit"].append(sum(deficit(c) for c in used))
            route_scores["planShortCorridors"].append(sum(deficit(c) > 0 for c in used))
            route_scores["planMaxDemand"].append(max((c["demand"] for c in used), default=0))
            route_scores["baseLength"].append(((q[0] - p[0]) ** 2 + (q[1] - p[1]) ** 2) ** 0.5)
            route_scores["baseChordBoxes"].append(chord_boxes[e])
            route_scores["baseEndArea"].append(sum(boxes[i]["width"] * boxes[i]["height"] for i in two))
            route_scores["baseEndDegree"].append(sum(degree.get(i, 0) for i in two))
        result["routeAuc"] = {k: auc(v, route_hit) for k, v in route_scores.items()}
        result["routesThrough"] = sum(route_hit)
        result["routes"] = len(ids)
        # Which boxes routes go through.
        times = {i: 0 for i in roots}
        for e in ids:
            for i in hit_pairs.get(e, ()):
                times[i] += 1
        beside_box = {i: [] for i in roots}
        for c in corridors:
            for side in ("before", "after"):
                if c.get(side) in beside_box:
                    beside_box[c[side]].append(c)
        box_scores = {
            "planMaxDeficit": [max((deficit(c) for c in beside_box[i]), default=0) for i in roots],
            "planSumDeficit": [sum(deficit(c) for c in beside_box[i]) for i in roots],
            "planSumDemand": [sum(c["demand"] for c in beside_box[i]) for i in roots],
            "planMaxDemand": [max((c["demand"] for c in beside_box[i]), default=0) for i in roots],
            "baseArea": [boxes[i]["width"] * boxes[i]["height"] for i in roots],
            "baseDegree": [degree.get(i, 0) for i in roots],
            "baseChordLoad": [crossing[i] for i in roots],
        }
        hit_box = [times[i] > 0 for i in roots]
        result["boxAuc"] = {k: auc(v, hit_box) for k, v in box_scores.items()}
        result["boxSpearman"] = {k: spearman(v, [times[i] for i in roots]) for k, v in box_scores.items()}
        result["boxesThrough"] = sum(hit_box)
        result["boxes"] = len(roots)
        out[kind] = result
    return out


def mean(values):
    values = [v for v in values if v is not None]
    return sum(values) / len(values) if values else None


def fmt(value, digits=2):
    return "-" if value is None else f"{value:.{digits}f}"


def summarize(files):
    """The tables of the results file, from the records."""
    records = [json.loads(line) for f in files for line in Path(f).read_text().splitlines()]
    measured = [r for r in records if "areas" in r]
    for r in records:
        if "failed" in r:
            print(f"FAILED {r['view']} {r.get('routeCost')}: {r['failed']}")
    costs = sorted({r["routeCost"] for r in measured})
    print("drawing unchanged in", sum(r["drawingUnchanged"] for r in measured), "of", len(measured), "records")
    for cost in costs:
        of = [r for r in measured if r["routeCost"] == cost]
        print(f"\n## route cost {cost:g}\n")
        print("| View | Routes through boxes: router, taps | Pairs of boxes: all, short | Demand on them: p95, max |"
              " Missing width: median, max | Length, turns against cost 0 |")
        print("| --- | --- | --- | --- | --- | --- |")
        for r in of:
            root = next((a for a in r["areas"] if a["area"] == "<root>"), None)
            if root is None:
                continue
            zero = next((q for q in measured if q["view"] == r["view"] and q["routeCost"] == 0), None)
            ratio = "-"
            if zero:
                z = next(a for a in zero["areas"] if a["area"] == "<root>")
                if z["length"] and z["turns"]:
                    ratio = f"{root['length'] / z['length']:.2f}, {root['turns'] / z['turns']:.2f}"
            kinds = r["today"]["byKind"]
            print(f"| {r['view']} | {kinds['router']['through']} of {kinds['router']['routes']}, "
                  f"{kinds['tap']['through']} of {kinds['tap']['routes']} | {root['pairs']}, {root['shortPairs']} | "
                  f"{root['innerDemandP95']}, {root['innerDemandMax']} | {root['needExtraMedian']:.0f}, "
                  f"{root['needExtraMax']:.0f} | {ratio} |")
        for kind in ("router", "tap"):
            with_hits = [r for r in of if kind in r and r[kind]["pairs"]["hits"]]
            if not with_hits:
                continue
            total = {k: sum(r[kind]["pairs"][k] for r in with_hits) for k in with_hits[0][kind]["pairs"]}
            base = total["hits"] / total["pairs"]
            print(f"\n### {kind}: {len(with_hits)} views with hits, {total['hits']} hits in {total['pairs']} pairs\n")
            print("| Flag on a pair | Pairs flagged | Hits among them | Recall | Precision | Lift |")
            print("| --- | --- | --- | --- | --- | --- |")
            for label, flagged, hits in (("planned way beside the box along a short corridor", "short", "shortHits"),
                                         ("planned way beside the box", "beside", "besideHits"),
                                         ("straight line between the ends crosses the box (no plan)", "chord",
                                          "chordHits")):
                precision = total[hits] / total[flagged] if total[flagged] else None
                print(f"| {label} | {total[flagged]} | {total[hits]} | {fmt(total[hits] / total['hits'])} | "
                      f"{fmt(precision, 3)} | {fmt(precision / base if precision is not None else None, 1)} |")
            verdict = (total["short"] and total["shortHits"] / total["hits"] >= 0.5 and total["beside"]
                       and total["shortHits"] / total["short"] > total["besideHits"] / total["beside"])
            holding = sum(
                1 for r in with_hits for p in [r[kind]["pairs"]]
                if p["short"] and p["shortHits"] / p["hits"] >= 0.5
                and p["shortHits"] / p["short"] > p["besideHits"] / p["beside"])
            print(f"\nR1 pooled: {'holds' if verdict else 'does not hold'}; holds on {holding} of "
                  f"{len(with_hits)} views.")
            for rule, key, what in (("R2", "routeAuc", "routes"), ("R3", "boxAuc", "boxes"), ("", "pairAuc", "pairs")):
                names = list(with_hits[0][kind][key])
                usable = [r for r in with_hits if any(v is not None for v in r[kind][key].values())]
                means = {n: mean([r[kind][key][n] for r in usable]) for n in names}
                print(f"\nAUC over {what}, mean of {len(usable)} views:\n")
                print("| " + " | ".join(names) + " |")
                print("|" + " --- |" * len(names))
                print("| " + " | ".join(fmt(means[n]) for n in names) + " |")
                if not rule:
                    continue
                plan = max((n for n in names if n.startswith("plan")), key=lambda n: means[n] or 0)
                other = max((n for n in names if n.startswith("base")), key=lambda n: means[n] or 0)
                wins = sum(1 for r in usable if r[kind][key][plan] is not None
                           and r[kind][key][plan] > max(0.5, r[kind][key][other] or 0))
                holds = (means[plan] or 0) > max(0.5, means[other] or 0)
                print(f"\n{rule}: {'holds' if holds else 'does not hold'} on the mean ({plan} {fmt(means[plan])} "
                      f"against {other} {fmt(means[other])}); {plan} is ahead on {wins} of {len(usable)} views.")
            if kind == "router":
                names = list(with_hits[0][kind]["boxSpearman"])
                print("\nSpearman of the box scores with the number of routes through the box, mean:\n")
                print("| " + " | ".join(names) + " |")
                print("|" + " --- |" * len(names))
                print("| " + " | ".join(fmt(mean([r[kind]["boxSpearman"][n] for r in with_hits])) for n in names) + " |")
        sides = [r["portSides"] for r in of if "portSides" in r]
        ends = sum(p["ends"] for p in sides)
        if ends:
            print(f"\nPlanned port side equal to the side of today's route: "
                  f"{sum(p['same'] for p in sides)} of {ends} ends.")
    default = [r for r in measured if r["routeCost"] == (1 if 1 in costs else costs[0])]
    print("\n## Sizes (root area) and the report\n")
    print("| View | Boxes | Routes | Corridors | Junctions | x: variables, constraints (joint) | y: the same |"
          " Separation by demand: variables, constraints per axis | CPU s off, on | Report, KB |")
    print("| --- |" + " --- |" * 9)
    for r in default:
        a = next((a for a in r["areas"] if a["area"] == "<root>"), None)
        if a is None:
            continue
        x, y = a["systems"]["x"], a["systems"]["y"]
        print(f"| {r['view']} | {a['boxes']} | {a['routes']} | {a['corridors']} | {a['junctions']} | "
              f"{x['jointVariables']}, {x['jointConstraints']} | {y['jointVariables']}, {y['jointConstraints']} | "
              f"{x['nodeVariables']}, {x['nodePairs']} / {y['nodePairs']} | {r['cpuOff']}, {r['cpuOn']} | "
              f"{r['reportBytes'] / 1000:.0f} |")
    failures = [r for r in default if "routerFailures" in r]
    if failures:
        print("\n## Why routes end up through boxes (instrumented router)\n")
        total = {}
        for r in failures:
            f = r["routerFailures"]
            print(r["view"], json.dumps(f))
            for k, v in f["through"].items():
                total[k] = total.get(k, 0) + v
        print("total", json.dumps(total, sort_keys=True))


def main():
    if len(sys.argv) > 2 and sys.argv[1] == "--summarize":
        summarize(sys.argv[2:])
        return
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin", default=str(ROOT / "target/release/layout-command"))
    parser.add_argument("--work", required=True, help="a directory for the request and answer files")
    parser.add_argument("--out", required=True)
    parser.add_argument("--costs", default="1", help="route costs, comma separated")
    parser.add_argument("--views", default="")
    parser.add_argument("--timing", type=int, default=0, help="only times: this many runs with and without the option")
    parser.add_argument("--today", action="store_true",
                        help="only today's drawing (no report): routes through boxes and CPU time, for another binary")
    parser.add_argument("--limit", type=int, default=900)
    args = parser.parse_args()
    work = Path(args.work)
    work.mkdir(parents=True, exist_ok=True)
    wanted = [v for v in args.views.split(",") if v] or list(views())

    def sh(*command):
        return subprocess.run(command, capture_output=True, text=True, timeout=60, cwd=ROOT).stdout.strip()

    meta = {
        "machine": sh("sysctl", "-n", "machdep.cpu.brand_string") or platform.processor(),
        "os": platform.platform(),
        "revision": sh("git", "rev-parse", "HEAD"),
        "dirty": bool(sh("git", "status", "--porcelain", "--", "crates")),
        # A binary outside the repository is named by its file name only.
        "binary": (str(Path(args.bin).resolve().relative_to(ROOT)) if Path(args.bin).resolve().is_relative_to(ROOT)
                   else Path(args.bin).name),
        # The arguments that decide what is measured; no paths.
        "command": ("organic_corridors.py --today" if args.today else f"organic_corridors.py --costs {args.costs}")
                   + (f" --views {args.views}" if args.views else "")
                   + (f" --timing {args.timing}" if args.timing else ""),
    }
    with open(args.out, "a") as out:
        for name in wanted:
            kind, spec = views()[name]
            body = request(kind, spec)
            record = {**meta, "view": name, "load": os.getloadavg(),
                      "date": time.strftime("%Y-%m-%dT%H:%M:%S%z")}
            with_plan = json.loads(json.dumps(body))
            if args.timing:
                times = {"off": [], "on": []}
                for _ in range(args.timing):
                    for label, cost in (("off", None), ("on", float(args.costs.split(",")[0]))):
                        if cost is None:
                            got = run(args.bin, body, work, args.limit)
                        else:
                            with_plan["options"]["organicCorridors"] = {"routeCost": cost}
                            got = run(args.bin, with_plan, work, args.limit)
                        times[label].append({k: got[k] for k in ("wall", "cpu")} if "snapshot" in got else got)
                        plans = got.get("snapshot", {}).get("corridorPlans")
                        if plans:
                            record["reportBytes"] = len(json.dumps(plans, separators=(",", ":")))
                            record["sizes"] = next((plan_summary(p) for p in plans if p["area"] == "<root>"), None)
                record["times"] = times
                record["loadAfter"] = os.getloadavg()
                out.write(json.dumps(record) + "\n")
                out.flush()
                print(name, json.dumps(times), file=sys.stderr)
                continue
            base = run(args.bin, body, work, args.limit)
            if "snapshot" not in base:
                out.write(json.dumps({**record, "failed": base}) + "\n")
                print(name, base, file=sys.stderr)
                continue
            if args.today:
                today = measure(body, base["snapshot"], [])["today"]
                line = {**record, "cpuOff": round(base["cpu"], 3), "today": today}
                out.write(json.dumps(line) + "\n")
                out.flush()
                print(name, "through", today["routes"], "router", today["byKind"]["router"]["through"], "taps",
                      today["byKind"]["tap"]["through"], "cpu", line["cpuOff"], file=sys.stderr)
                continue
            for cost in (float(c) for c in args.costs.split(",")):
                with_plan["options"]["organicCorridors"] = {"routeCost": cost}
                got = run(args.bin, with_plan, work, args.limit)
                if "snapshot" not in got:
                    out.write(json.dumps({**record, "routeCost": cost, "failed": got}) + "\n")
                    print(name, cost, got, file=sys.stderr)
                    continue
                snapshot = got["snapshot"]
                plans = snapshot.pop("corridorPlans", [])
                # Everything but the options: default options are left out of an answer altogether, so only with the
                # report's option present can a view with otherwise default options differ there.
                options = snapshot.pop("options", {})
                options.pop("organicCorridors", None)
                reference = dict(base["snapshot"])
                expected = reference.pop("options", None)
                line = {
                    **record,
                    "routeCost": cost,
                    "drawingUnchanged": snapshot == reference and expected in (None, options),
                    "cpuOff": round(base["cpu"], 3),
                    "cpuOn": round(got["cpu"], 3),
                    "reportBytes": len(json.dumps(plans, separators=(",", ":"))),
                    "areas": [plan_summary(p) for p in plans if p["routes"] or p["area"] == "<root>"],
                    "otherAreas": sum(not (p["routes"] or p["area"] == "<root>") for p in plans),
                    **measure(body, base["snapshot"], plans),
                }
                if base["log"]:
                    line["routerFailures"] = router_failures(body, base["snapshot"], base["log"])
                out.write(json.dumps(line) + "\n")
                out.flush()
                root = next((a for a in line["areas"] if a["area"] == "<root>"), {})
                print(name, cost, "same" if line["drawingUnchanged"] else "CHANGED", "through", line["today"]["routes"],
                      "short", root.get("shortPairs"), "cpu", line["cpuOff"], line["cpuOn"], file=sys.stderr)


if __name__ == "__main__":
    main()
