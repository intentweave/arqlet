"""Where the length of the flat-region candidate comes from (state-tcp, lesbar).

Per edge: Manhattan distance between its two ports (a lower bound with fixed ports), routed length, their ratio.
Also the drawing's bounding box and the largest gaps between node boxes. Writes a results file next to the others.

    python3 benchmarks/flat_length.py            # uses target/release/layout-command (build it first)
"""
import datetime
import json
import os
import platform
import subprocess
import sys

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
BIN = os.path.join(ROOT, "target", "release", "layout-command")


def run(request):
    out = subprocess.run([BIN], input=json.dumps(request), capture_output=True, text=True, timeout=1800).stdout
    reply = json.loads(out)
    if not reply.get("ok"):
        raise SystemExit("layout-command failed: " + str(reply.get("error")))
    return reply["snapshot"]


def lesbar_snapshot():
    graph = json.load(open(os.path.join(ROOT, "benchmarks", "inputs", "state-tcp.json")))
    preset = json.load(open(os.path.join(ROOT, "presets", "lesbar.json")))["configSet"]
    snap = run({"op": "import", "graph": graph, "limit": 100})
    return run({"op": "configure", "snapshot": snap, "expectedRevision": snap["revision"],
                "expectedLayoutVersion": snap["layoutVersion"], "options": {"configSet": preset}})


def refined(snap, options=None):
    if options is not None:
        snap = run({"op": "configure", "snapshot": snap, "expectedRevision": snap["revision"],
                    "expectedLayoutVersion": snap["layoutVersion"], "options": options})
    return run({"op": "refine", "snapshot": snap, "expectedRevision": snap["revision"],
                "expectedLayoutVersion": snap["layoutVersion"], "threads": 1})


def measure(snap):
    ports = {(p["edgeId"], p["endpoint"]): p["position"] for p in snap["resolvedPorts"] if not p.get("explicit")}
    rows = []
    for route in snap["routes"]:
        pts = route["points"]
        length = sum(abs(a["x"] - b["x"]) + abs(a["y"] - b["y"]) for a, b in zip(pts, pts[1:]))
        src = ports.get((route["id"], "source"))
        dst = ports.get((route["id"], "target"))
        manhattan = abs(src["x"] - dst["x"]) + abs(src["y"] - dst["y"]) if src and dst else None
        rows.append({"edge": route["id"], "manhattan": manhattan, "routed": length,
                     "ratio": (length / manhattan) if manhattan else None})
    boxes = snap["boxes"]
    xs = sorted({(b["x"], b["x"] + b["width"]) for b in boxes.values()})
    ys = sorted({(b["y"], b["y"] + b["height"]) for b in boxes.values()})
    def gaps(intervals):
        spans = sorted(intervals)
        out = []
        reach = spans[0][1]
        for lo, hi in spans[1:]:
            if lo > reach:
                out.append(round(lo - reach, 1))
            reach = max(reach, hi)
        return sorted(out, reverse=True)[:5]
    width = max(b["x"] + b["width"] for b in boxes.values()) - min(b["x"] for b in boxes.values())
    height = max(b["y"] + b["height"] for b in boxes.values()) - min(b["y"] for b in boxes.values())
    return {"rows": rows, "width": round(width, 1), "height": round(height, 1),
            "largest_x_gaps": gaps(xs), "largest_y_gaps": gaps(ys),
            "total_routed": round(sum(r["routed"] for r in rows), 1),
            "total_manhattan": round(sum(r["manhattan"] or 0 for r in rows), 1)}


def machine_line():
    rev = subprocess.run(["git", "-C", ROOT, "rev-parse", "--short", "HEAD"], capture_output=True, text=True).stdout.strip()
    return f"{platform.system()} {platform.machine()}, revision {rev}"


def main():
    base = lesbar_snapshot()
    flat = refined(base, {"flatRegion": True})
    report = {"machine": machine_line(), "date": datetime.date.today().isoformat(),
              "input": "benchmarks/inputs/state-tcp.json", "profile": "lesbar",
              "baseline": measure(refined(base)), "flat": measure(flat)}
    stamp = datetime.datetime.now().strftime("%Y%m%dT%H%M%SZ")
    path = os.path.join(ROOT, "benchmarks", "results", f"flat-length-{stamp}.json")
    with open(path, "w") as f:
        json.dump(report, f, indent=1)
    for label in ("baseline", "flat"):
        m = report[label]
        print(f"{label}: routed {m['total_routed']} manhattan {m['total_manhattan']} "
              f"width {m['width']} height {m['height']} x-gaps {m['largest_x_gaps']} y-gaps {m['largest_y_gaps']}")
        for r in sorted(m["rows"], key=lambda r: -(r["ratio"] or 0))[:5]:
            print(f"  {r['edge']}: routed {r['routed']} manhattan {r['manhattan']} ratio {r['ratio'] and round(r['ratio'], 2)}")
    print("written", path, file=sys.stderr)


if __name__ == "__main__":
    main()
