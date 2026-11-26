#!/usr/bin/env python3
"""#203: why bus taps run through boxes. Needs a `layout-command` built with `bus-tap-causes.patch` (applied to main
`231c010`), which logs how every tap was drawn. Standard library and `node` only.

For each generated view it counts the tap routes through boxes twice, with any depth (as `router_grid.py` counts) and
deeper than half a unit (the tolerance of the tap drawing and of the engine's own route conflicts), and splits the deeper
ones by the part of the line that is in a box: the member's tap, the head's tap, or the stretch along the bar. A line is
the member's tap to the bar's middle, along it, and the head's tap back.

Usage: bus_tap_causes.py --bin BINARY --work DIR [--views NAME,...]
"""
import argparse
import collections
import json
import os
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import router_grid  # noqa: E402


def segment_in(p, q, box, depth):
    return (max(p[0], q[0]) > box["x"] + depth and min(p[0], q[0]) < box["x"] + box["width"] - depth
            and max(p[1], q[1]) > box["y"] + depth and min(p[1], q[1]) < box["y"] + box["height"] - depth)


def path_in(points, box, depth):
    return any(segment_in(p, q, box, depth) for p, q in zip(points, points[1:]))


def to_bar(points, node, horizontal, middle):
    """A tap as `bars_to_lines` takes it: from its member to the bar's middle."""
    def at_node(p):
        return (node["x"] - 1 <= p[0] <= node["x"] + node["width"] + 1
                and node["y"] - 1 <= p[1] <= node["y"] + node["height"] + 1)

    points = list(points)
    if not at_node(points[0]):
        points.reverse()
    last, before = points[-1], points[-2]
    axis = 1 if horizontal else 0
    if abs(before[axis] - last[axis]) >= 1e-9:
        points.pop()
    points.append((last[0], middle) if horizontal else (middle, last[1]))
    return points


def analyse(snapshot, log):
    parent, boxes = router_grid.absolute(snapshot)

    def up(i):
        while i:
            yield i
            i = parent.get(i)

    drawn, bars, members = {}, {}, {}
    for line in log:
        field = line.split("\t")
        if field[0] == "EXPTAP":
            drawn[field[2]] = (field[4], field[6].split(" ")[0] if len(field) > 6 else "")
        elif field[0] == "EXPBAR":
            x, y, w, h = map(float, field[3].split(","))
            taps = {}
            for tap in field[5].split("|"):
                edge, points = tap.split(":", 1)
                taps[edge] = [tuple(map(float, p.split(","))) for p in points.split(";")]
            bars[field[1]] = (field[2], field[4] == "1", y + h / 2 if field[4] == "1" else x + w / 2, taps)
        elif field[0] == "EXPBUS":
            members[field[1]] = dict(t.split("=") for t in field[2].split(";"))
    edges = {e["id"]: e for e in snapshot["graph"]["edges"]}
    routes = {r["id"]: [(p["x"], p["y"]) for p in r["points"]] for r in snapshot["routes"]}
    count = collections.Counter()
    for bus, (head, horizontal, middle, taps) in bars.items():
        ways = {node: to_bar(taps[e], boxes[node], horizontal, middle)
                for e, node in members[bus].items() if e in taps}
        head_edge = next(e for e, node in members[bus].items() if node == head)
        for edge, node in members[bus].items():
            if node == head or edge not in routes or node not in ways or head not in ways:
                continue
            skip = set(up(edges[edge]["source"])) | set(up(edges[edge]["target"]))
            foreign = [b for i, b in boxes.items() if i not in skip]
            count["tap routes"] += 1
            count["through, any depth"] += any(path_in(routes[edge], b, 1e-6) for b in foreign)
            deep = [b for b in foreign if path_in(routes[edge], b, 0.5001)]
            if not deep:
                continue
            count["through, deeper than 0.5"] += 1
            stretch = [ways[node][-1], ways[head][-1]]
            parts = {name: any(path_in(points, b, 0.5001) for b in deep)
                     for name, points in (("member's tap", ways[node]), ("head's tap", ways[head]),
                                          ("along the bar", stretch))}
            failed = []
            for name, tap in (("member's tap", edge), ("head's tap", head_edge)):
                method, why = drawn.get(tap, ("unknown", ""))
                if parts[name]:
                    failed.append(why if method == "failed" else f"drawn clear ({method})")
            if failed:
                count["  a tap through a box: " + ", ".join(sorted(set(failed)))] += 1
            else:
                count["  only the stretch along the bar"] += 1
    return count


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin", required=True)
    parser.add_argument("--work", required=True)
    parser.add_argument("--views", default="report")
    args = parser.parse_args()
    work = Path(args.work)
    work.mkdir(parents=True, exist_ok=True)
    sets = router_grid.view_sets()
    total = collections.Counter()
    for name in [v for part in args.views.split(",") for v in sets.get(part, [part])]:
        if not name.startswith("mesh-") or name.split("-")[2] == "0":
            continue
        body = router_grid.request(name)
        source, answer, log = work / "request.json", work / "answer.json", work / "log.txt"
        source.write_text(json.dumps(body))
        with source.open() as i, answer.open("w") as o, log.open("w") as e:
            subprocess.run([args.bin], stdin=i, stdout=o, stderr=e, timeout=600, check=True,
                           env={**os.environ, "ARQLET_TAP_LOG": "1"})
        count = analyse(json.loads(answer.read_text())["snapshot"], log.read_text().splitlines())
        print(name, json.dumps(count))
        total += count
    print("sum")
    for key, value in sorted(total.items()):
        print(f"  {key}: {value}")


if __name__ == "__main__":
    main()
