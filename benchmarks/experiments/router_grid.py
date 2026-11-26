#!/usr/bin/env python3
"""#203: the router's repair search on views whose grid is over its bound, with and without `sparseRepairGrid`.
Standard library only; needs `node` for the generated views and the datasets of
`python3 benchmarks/generate.py --no-manifest`.

Each view is imported by `layout-command`, once per arm: an arm is a name, a binary and whether the option is set. The
record of a run holds the CPU and wall time, the routes through boxes (a drawn box that is neither an end of the edge nor
holds one; router routes and bus taps apart), whether every edge has a route from its source's box to its target's in
that direction, the engine's geometry and routing issues, the metrics of the `metrics` operation on the drawing (an
explicit bus's edges may share lines), and a digest of the drawing without its options.

Usage: router_grid.py --arm NAME=BINARY[,on] ... --work DIR --out FILE.jsonl [--views SET|NAME,...] [--runs N]
       router_grid.py --summarize FILE.jsonl ... [--base ARM]

With `--runs N` the arms are run in turn N times; every run is a record. `--times` records only the CPU and wall time
and the digest of each run, and `--summarize` then prints the median and range of the CPU times. SET is `report` (the views of the C1 report of
#156), `other` (generated views not used for the design), `corpus`, `large` or `all`.
"""
import argparse
import hashlib
import json
import os
import platform
import statistics
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
EPS = 1e-6

# (units, buses, children, edges, seed, strategy) of bus-mesh.mjs, bus lines as `lines`.
REPORT = [
    (100, 0, 60, 250, 7), (100, 30, 60, 250, 7), (100, 30, 120, 500, 7), (100, 30, 240, 1000, 7),
    (100, 30, 0, 0, 7), (100, 0, 120, 500, 7), (100, 0, 240, 1000, 7),
    (100, 30, 60, 250, 11), (100, 30, 60, 250, 13), (100, 30, 120, 500, 11), (100, 30, 120, 500, 13),
    (200, 60, 120, 500, 7), (50, 15, 120, 500, 7),
]
# Chosen after the search was fixed on the report's views: other seeds, sizes and shares.
OTHER = [
    (100, 30, 60, 250, 17), (100, 30, 120, 500, 19), (100, 30, 240, 1000, 23), (100, 20, 120, 500, 29),
    (150, 45, 120, 500, 7), (150, 0, 200, 600, 7), (80, 0, 300, 900, 7), (200, 60, 240, 800, 31),
    (300, 90, 120, 500, 7), (480, 0, 60, 250, 7), (60, 18, 360, 900, 37),
]
LARGE = [500, 1000, 2000]


def mesh_name(spec):
    return "mesh-{}-{}-{}-{}-s{}".format(*spec)


def corpus_names():
    return sorted(p.stem for p in (ROOT / "benchmarks/datasets").glob("*.json")
                  if not p.stem.startswith("synthetic-20k") and "intentweave" not in p.stem)


def view_sets():
    report = [mesh_name(s) for s in REPORT] + ["corpus:mixed-large"]
    other = [mesh_name(s) for s in OTHER]
    corpus = [f"corpus:{n}" for n in corpus_names()]
    large = [f"large-{n}" for n in LARGE]
    return {"report": report, "other": other, "corpus": corpus, "large": large,
            "all": report + other + [c for c in corpus if c not in report] + large}


def request(name):
    if name.startswith("mesh-"):
        units, buses, children, edges, seed = name[5:].split("-")
        args = ["node", str(ROOT / "benchmarks/experiments/bus-mesh.mjs"), units, buses, "organic", "lines",
                children, edges, seed[1:]]
        return json.loads(subprocess.run(args, check=True, capture_output=True, text=True, timeout=120).stdout)
    if name.startswith("corpus:"):
        graph = json.loads((ROOT / "benchmarks/datasets" / f"{name[7:]}.json").read_text())
        return {"op": "import", "graph": graph, "limit": 10000 if len(graph["nodes"]) <= 600 else 150, "options": {}}
    # The large flat regions of the C1 report: orthogonal, scalable stress.
    sys.path.insert(0, str(ROOT / "benchmarks"))
    import generate

    graph = generate.organic_ladder(int(name.split("-")[1]))
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
                    "cpu": usage.ru_utime + usage.ru_stime}
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


def on_border(p, b, slack=0.5):
    inside_x = b["x"] - slack <= p["x"] <= b["x"] + b["width"] + slack
    inside_y = b["y"] - slack <= p["y"] <= b["y"] + b["height"] + slack
    return (inside_x and (abs(p["y"] - b["y"]) <= slack or abs(p["y"] - b["y"] - b["height"]) <= slack)) or (
        inside_y and (abs(p["x"] - b["x"]) <= slack or abs(p["x"] - b["x"] - b["width"]) <= slack))


def digest(snapshot):
    """A digest of the drawing: the snapshot without its options."""
    rest = {k: v for k, v in snapshot.items() if k != "options"}
    return hashlib.sha256(json.dumps(rest, sort_keys=True).encode()).hexdigest()[:16]


def measure(binary, body, snapshot):
    parent, boxes = absolute(snapshot)
    containers = {p for p in parent.values() if p}

    def up(i):
        while i:
            yield i
            i = parent.get(i)

    graph = snapshot["graph"]
    edges = {e["id"]: e for e in graph["edges"] if e.get("target") is not None}
    taps = {t["edge"] for b in body["graph"].get("buses", []) for t in b.get("taps", [])}
    routes = {r["id"]: r for r in snapshot["routes"]}
    hits = {"router": 0, "tap": 0, "boxHits": 0}
    missing = oblique = 0
    ends_off = []
    straight = set(snapshot.get("straightEdges", []))
    for e, edge in edges.items():
        route = routes.get(e)
        if route is None or len(route["points"]) < 2:
            missing += 1
            continue
        points = route["points"]
        source, target = boxes.get(edge["source"]), boxes.get(edge["target"])
        if source and target and not (on_border(points[0], source) and on_border(points[-1], target)):
            ends_off.append(e)
        if e not in straight:
            oblique += any(abs(p["x"] - q["x"]) > EPS and abs(p["y"] - q["y"]) > EPS
                           for p, q in zip(points, points[1:]))
        skip = set(up(edge["source"])) | set(up(edge["target"]))
        found = sum(1 for i, b in boxes.items() if i not in skip and through(points, b))
        hits["tap" if e in taps else "router"] += bool(found)
        hits["boxHits"] += found
    # The drawing as the `metrics` operation takes it.
    members = {r["id"] for r in snapshot["routes"] if r.get("bundle_of")}
    ports = {(p["edgeId"], p["endpoint"]): p["id"] for p in snapshot.get("resolvedPorts", [])}
    shapes = {n["id"]: n.get("shape") for n in graph["nodes"]}
    drawing = {
        "nodes": [{"id": i, "parentId": parent.get(i), "container": i in containers, **b,
                   **({"shape": shapes[i]} if shapes.get(i) not in (None, "rect") else {})}
                  for i, b in boxes.items()],
        "edges": [{"id": e, "source": edge["source"], "target": edge["target"],
                   "points": routes.get(e, {}).get("points", []), "sourcePort": ports.get((e, "source")),
                   "targetPort": ports.get((e, "target"))}
                  for e, edge in edges.items() if e not in members],
    }
    buses = [b["edges"] for b in graph.get("buses", []) if b.get("edges")]
    if buses:
        drawing["buses"] = buses
    got = subprocess.run([binary], input=json.dumps({"op": "metrics", "drawing": drawing}), capture_output=True,
                         text=True, timeout=600, check=True)
    metrics = json.loads(got.stdout)["metrics"]
    return {
        "edges": len(edges), "boxes": len(boxes), "missingRoutes": missing, "endsOffTheirBoxes": sorted(ends_off),
        "obliqueRoutes": oblique, "through": {**hits, "routes": hits["router"] + hits["tap"]},
        "geometryIssues": len(snapshot.get("geometryIssues", [])),
        "routingIssues": len(snapshot.get("routingIssues", [])),
        "routeConflicts": len(snapshot.get("routeConflicts", [])),
        "metrics": {k: metrics[k] for k in (
            "lineNodeHits", "containerTraversals", "crossings", "sharedEndpointCrossings", "bends", "length",
            "sharedLinePairs", "nearParallelPairs", "hugPairs", "siblingOverlaps", "area")},
        "digest": digest(snapshot),
    }


def summarize(files, base):
    records = [json.loads(line) for f in files for line in Path(f).read_text().splitlines()]
    views, arms = [], []
    for r in records:
        if r["view"] not in views:
            views.append(r["view"])
        if r["arm"] not in arms:
            arms.append(r["arm"])
    base = base or arms[0]
    by = {}
    for r in records:
        by.setdefault((r["view"], r["arm"]), []).append(r)
    if all("through" not in r for r in records):
        summarize_times(by, views, arms, base)
        return

    def first(view, arm):
        rs = by.get((view, arm), [])
        return next((r for r in rs if "through" in r), None)

    def cpu(view, arm):
        values = [r["cpu"] for r in by.get((view, arm), []) if "cpu" in r]
        return statistics.median(values) if values else None

    for r in records:
        if "failed" in r:
            print(f"FAILED {r['view']} {r['arm']}: {r['failed']}")
    for arm in arms:
        for view in views:
            digests = {r.get("digest") for r in by.get((view, arm), [])}
            if len(digests) > 1:
                print(f"NOT REPEATABLE {view} {arm}: {digests}")
    for arm in arms:
        if arm == base:
            continue
        print(f"\n## {arm} against {base}\n")
        print("| View | Through boxes: router, taps | Lines through nodes | Shared line pairs | Crossings | Bends |"
              " Length | CPU s | In objective order |")
        print("| --- |" + " --- |" * 8)
        totals = {a: {} for a in (base, arm)}
        compared = []
        for view in views:
            a, b = first(view, base), first(view, arm)
            if not a or not b:
                continue
            compared.append(view)
            ma, mb = a["metrics"], b["metrics"]
            cross = [m["crossings"] + m["sharedEndpointCrossings"] for m in (ma, mb)]
            values = {
                "router": (a["through"]["router"], b["through"]["router"]),
                "taps": (a["through"]["tap"], b["through"]["tap"]),
                "boxHits": (a["through"]["boxHits"], b["through"]["boxHits"]),
                "lineNodeHits": (ma["lineNodeHits"], mb["lineNodeHits"]),
                "containerTraversals": (ma["containerTraversals"], mb["containerTraversals"]),
                "crossings": tuple(cross),
                "bends": (ma["bends"], mb["bends"]),
                "length": (ma["length"], mb["length"]),
                "sharedLinePairs": (ma["sharedLinePairs"], mb["sharedLinePairs"]),
                "nearParallelPairs": (ma["nearParallelPairs"], mb["nearParallelPairs"]),
                "hugPairs": (ma["hugPairs"], mb["hugPairs"]),
                "siblingOverlaps": (ma["siblingOverlaps"], mb["siblingOverlaps"]),
                "geometryIssues": (a["geometryIssues"], b["geometryIssues"]),
                "routingIssues": (a["routingIssues"], b["routingIssues"]),
                "missingRoutes": (a["missingRoutes"], b["missingRoutes"]),
                "endsOffTheirBoxes": (len(a["endsOffTheirBoxes"]), len(b["endsOffTheirBoxes"])),
                # Edges that end off their boxes with this arm and not with the base.
                "newlyOffTheirBoxes": (0, len(set(b["endsOffTheirBoxes"]) - set(a["endsOffTheirBoxes"]))),
                "obliqueRoutes": (a["obliqueRoutes"], b["obliqueRoutes"]),
                "cpu": (cpu(view, base), cpu(view, arm)),
            }
            for key, (x, y) in values.items():
                totals[base][key] = totals[base].get(key, 0) + x
                totals[arm][key] = totals[arm].get(key, 0) + y

            def pair(key, fmt="{:.0f}"):
                x, y = values[key]
                return fmt.format(x) if x == y else f"{fmt.format(x)} → {fmt.format(y)}"

            # The first criterion of the default objectives that differs decides (the metrics' counts; `crossings` are
            # crossing points).
            verdict = "same drawing"
            if a["digest"] != b["digest"]:
                verdict = "equal"
                for key in ("siblingOverlaps", "lineNodeHits", "sharedLinePairs", "crossings", "hugPairs", "bends",
                            "length"):
                    x, y = values[key]
                    if x != y:
                        verdict = f"{'better' if y < x else 'worse'} ({key})"
                        break
            print(f"| {view} | {pair('router')}, {pair('taps')} | {pair('lineNodeHits')} | "
                  f"{pair('sharedLinePairs')} | {pair('crossings')} | {pair('bends')} | {pair('length')} | "
                  f"{values['cpu'][0]:.2f} → {values['cpu'][1]:.2f} | {verdict} |")
        same = sum(first(v, base)["digest"] == first(v, arm)["digest"] for v in compared)
        print(f"\n{len(compared)} views, {same} drawn the same.\n")
        print("| Sum over the views | " + base + " | " + arm + " |")
        print("| --- | --- | --- |")
        for key in totals[base]:
            x, y = totals[base][key], totals[arm][key]
            print(f"| {key} | {x:.2f} | {y:.2f} |" if key == "cpu" else f"| {key} | {x:.0f} | {y:.0f} |")


def summarize_times(by, views, arms, base):
    """CPU seconds of the runs: the median and the range per view and arm, and the 1-minute loads seen."""
    for (view, arm), records in by.items():
        for r in records:
            if "failed" in r:
                print(f"FAILED {view} {arm}: {r['failed']}")
        if len({r.get("digest") for r in records}) > 1:
            print(f"NOT REPEATABLE {view} {arm}")
    loads = [r["load"][0] for records in by.values() for r in records]
    print(f"1-minute load: {min(loads):.1f} to {max(loads):.1f}, median {statistics.median(loads):.1f}\n")
    print("| View | " + " | ".join(f"CPU s, {a}: median (range)" for a in arms) + " | Ratio of the medians | Runs |")
    print("| --- |" + " --- |" * (len(arms) + 2))
    totals = {a: 0.0 for a in arms}
    for view in views:
        cells, medians, runs = [], {}, 0
        for arm in arms:
            values = [r["cpu"] for r in by.get((view, arm), []) if "cpu" in r]
            if not values:
                cells.append("-")
                continue
            medians[arm] = statistics.median(values)
            runs = len(values)
            cells.append(f"{medians[arm]:.2f} ({min(values):.2f} to {max(values):.2f})")
        if len(medians) == len(arms):
            for arm in arms:
                totals[arm] += medians[arm]
        others = [a for a in arms if a != base and a in medians]
        ratio = ", ".join(f"{medians[a] / medians[base]:.2f}" for a in others) if base in medians else "-"
        print(f"| {view} | " + " | ".join(cells) + f" | {ratio} | {runs} |")
    print("| sum of the medians | " + " | ".join(f"{totals[a]:.2f}" for a in arms) + " | "
          + ", ".join(f"{totals[a] / totals[base]:.2f}" for a in arms if a != base) + " | |")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--summarize", nargs="+")
    parser.add_argument("--base", default="")
    parser.add_argument("--arm", action="append", default=[], help="NAME=BINARY[,on]")
    parser.add_argument("--work", help="a directory for the request and answer files")
    parser.add_argument("--out")
    parser.add_argument("--views", default="report")
    parser.add_argument("--runs", type=int, default=1)
    parser.add_argument("--times", action="store_true", help="only the times and the digests")
    parser.add_argument("--limit", type=int, default=900)
    args = parser.parse_args()
    if args.summarize:
        summarize(args.summarize, args.base)
        return
    work = Path(args.work)
    work.mkdir(parents=True, exist_ok=True)
    sets = view_sets()
    wanted = [v for part in args.views.split(",") if part for v in sets.get(part, [part])]
    arms = []
    for spec in args.arm:
        name, rest = spec.split("=", 1)
        binary, _, flag = rest.partition(",")
        arms.append((name, binary, flag == "on"))

    def sh(*command):
        return subprocess.run(command, capture_output=True, text=True, timeout=60, cwd=ROOT).stdout.strip()

    meta = {
        "machine": sh("sysctl", "-n", "machdep.cpu.brand_string") or platform.processor(),
        "os": platform.platform(),
        "revision": sh("git", "rev-parse", "HEAD"),
        "dirty": bool(sh("git", "status", "--porcelain", "--", "crates")),
        # No paths: the arms are named, and the results file says what each binary is.
        "command": f"router_grid.py --views {args.views} --runs {args.runs}{' --times' if args.times else ''} --arm "
                   + " --arm ".join(f"{n}={Path(b).name}{',on' if on else ''}" for n, b, on in arms),
    }
    with open(args.out, "a") as out:
        for name in wanted:
            body = request(name)
            for turn in range(args.runs):
                for arm, binary, on in arms:
                    sent = json.loads(json.dumps(body))
                    if on:
                        sent.setdefault("options", {})["sparseRepairGrid"] = True
                    record = {**meta, "view": name, "arm": arm, "option": on, "run": turn,
                              "load": [round(v, 2) for v in os.getloadavg()],
                              "date": time.strftime("%Y-%m-%dT%H:%M:%S%z")}
                    got = run(binary, sent, work, args.limit)
                    if "snapshot" not in got:
                        record["failed"] = got
                        print(name, arm, got, file=sys.stderr)
                    elif args.times:
                        record.update(digest=digest(got["snapshot"]), cpu=round(got["cpu"], 3),
                                      wall=round(got["wall"], 3))
                        print(name, arm, turn, "cpu", record["cpu"], "load", record["load"][0], file=sys.stderr,
                              flush=True)
                    else:
                        record.update(measure(binary, body, got["snapshot"]))
                        record["cpu"] = round(got["cpu"], 3)
                        record["wall"] = round(got["wall"], 3)
                        m = record["metrics"]
                        print(name, arm, "through", record["through"]["router"], record["through"]["tap"], "cross",
                              m["crossings"] + m["sharedEndpointCrossings"], "bends", m["bends"], "shared",
                              m["sharedLinePairs"], "cpu", record["cpu"], file=sys.stderr, flush=True)
                    out.write(json.dumps(record) + "\n")
                    out.flush()


if __name__ == "__main__":
    main()
