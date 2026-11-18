#!/usr/bin/env python3
"""Scaling step 3 (roadmap section 7): the abstract start of a large graph, from 20K to 1M nodes.

For each size, a hierarchical graph of `benchmarks/overview/generate.py` (shaped like a code index, seed 1) and three
requests, each one `layout-command` process (JSON parse and write included):

  overview   the cluster overview (`{op: "overview", graph, threads}`)
  first      the first structured view within a budget (`{op: "import", graph, limit, options}`, options below)
  open       opening one collapsed container of that view (`{op: "toggle", snapshot, nodeId, ...}`): the first one the view
             shows, in the order of its collapsed ids

Per request: wall time, the process's peak resident memory (`/usr/bin/time -l` on macOS, `-v` on Linux), request and
response bytes, and what the view shows (boxes, frames, edges, delegated routes, line-node issues). Graphs and requests
are written to a temporary directory, never into the repository. Writes one JSON line per (size, request) to
benchmarks/results/scaling-<UTC>.jsonl, after a first line with machine, revision and load. Single runs; for timings to
compare, run on an otherwise idle machine:

    cargo build --release -p arqlet-runtime --bin layout-command --locked
    python3 benchmarks/scaling/series.py --sizes 20000 50000 100000 300000 1000000
"""
import argparse, datetime, json, os, platform, re, subprocess, sys, tempfile, time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
BINARY = ROOT / "target/release/layout-command"
sys.path.insert(0, str(ROOT / "benchmarks/overview"))
from generate import generate  # noqa: E402

FIRST_VIEW = {"levelOfDetail": {"nodes": 1000, "edges": 3000}, "liftEdges": True}


def sh(*cmd):
    try:
        return subprocess.run(cmd, capture_output=True, text=True, check=True).stdout.strip()
    except Exception:
        return None


def machine():
    return {"cpu": sh("sysctl", "-n", "machdep.cpu.brand_string") or platform.processor(), "cores": os.cpu_count(),
            "memory": sh("sysctl", "-n", "hw.memsize"), "platform": platform.platform(),
            "revision": sh("git", "-C", str(ROOT), "rev-parse", "--short", "HEAD"),
            "dirty": bool(sh("git", "-C", str(ROOT), "status", "--porcelain", "--untracked-files=no")),
            "rustc": sh("rustc", "--version"), "build": "release (lto thin)", "load": os.getloadavg(),
            "utc": datetime.datetime.now(datetime.timezone.utc).isoformat(timespec="seconds")}


def run(request_path, response_path, timeout):
    """Runs one request; returns (seconds, peak resident bytes or None, exit status)."""
    linux = platform.system() == "Linux"
    cmd = ["/usr/bin/time", "-v" if linux else "-l", str(BINARY)]
    started = time.perf_counter()
    with open(request_path, "rb") as stdin, open(response_path, "wb") as stdout:
        try:
            done = subprocess.run(cmd, stdin=stdin, stdout=stdout, stderr=subprocess.PIPE, timeout=timeout)
        except subprocess.TimeoutExpired:
            return time.perf_counter() - started, None, "timeout"
    seconds = time.perf_counter() - started
    err = done.stderr.decode(errors="replace")
    if linux:
        m = re.search(r"Maximum resident set size \(kbytes\): (\d+)", err)
        peak = int(m.group(1)) * 1024 if m else None
    else:
        m = re.search(r"(\d+)\s+maximum resident set size", err)
        peak = int(m.group(1)) if m else None
    return seconds, peak, done.returncode


def view_shape(snapshot):
    frames = {n.get("parentId") for n in snapshot["graph"]["nodes"] if n.get("parentId")}
    passes = [re.match(r"Single pass: (\d+) constructed, (\d+) delegated", d) for d in snapshot.get("layoutDiagnostics", [])]
    passes = [p for p in passes if p]
    return {"boxes": len(snapshot["graph"]["nodes"]) - len(frames), "frames": len(frames),
            "edges": len(snapshot["graph"]["edges"]), "delegated": int(passes[0].group(2)) if passes else None,
            "routingIssues": len(snapshot.get("routingIssues", [])), "collapsed": len(snapshot.get("collapsedIds", []))}


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--sizes", type=int, nargs="+", default=[20000, 50000, 100000, 300000, 1000000])
    p.add_argument("--seed", type=int, default=1)
    p.add_argument("--threads", type=int, default=4)
    p.add_argument("--timeout", type=int, default=1800, help="seconds per request")
    p.add_argument("--out", type=Path, default=None)
    a = p.parse_args()
    if not BINARY.exists():
        sys.exit(f"Build first: cargo build --release -p arqlet-runtime --bin layout-command --locked ({BINARY} missing)")
    stamp = datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    out = a.out or ROOT / f"benchmarks/results/scaling-{stamp}.jsonl"
    with open(out, "a") as f:
        f.write(json.dumps({"machine": machine(), "firstView": FIRST_VIEW, "seed": a.seed, "threads": a.threads}) + "\n")
    work = Path(tempfile.mkdtemp(prefix="arqlet-scaling-"))
    for n in a.sizes:
        graph = generate(n, a.seed)
        base = {"size": n, "graphNodes": len(graph["nodes"]), "graphEdges": len(graph["edges"]),
                "containers": len({x.get("parentId") for x in graph["nodes"] if x.get("parentId")})}
        requests = {
            "overview": {"op": "overview", "graph": graph, "threads": a.threads},
            "first": {"op": "import", "graph": graph, "limit": 10000, "options": FIRST_VIEW},
        }
        snapshot = None
        for name in ["overview", "first", "open"]:
            if name == "open":
                if snapshot is None:
                    break
                visible = {x["id"] for x in snapshot["graph"]["nodes"]}
                target = next((c for c in snapshot.get("collapsedIds", []) if c in visible), None)
                if target is None:
                    break
                requests["open"] = {"op": "toggle", "snapshot": snapshot, "nodeId": target,
                                    "expectedRevision": snapshot["revision"],
                                    "expectedLayoutVersion": snapshot["layoutVersion"]}
            request_path, response_path = work / f"{n}-{name}.json", work / f"{n}-{name}.out.json"
            request_path.write_text(json.dumps(requests[name]))
            seconds, peak, status = run(request_path, response_path, a.timeout)
            row = {**base, "request": name, "seconds": round(seconds, 2), "peakBytes": peak, "status": status,
                   "requestBytes": request_path.stat().st_size, "responseBytes": response_path.stat().st_size,
                   "load": os.getloadavg()}
            if status == 0:
                response = json.loads(response_path.read_text())
                if not response.get("ok"):
                    row["error"] = response.get("error")
                elif name == "overview":
                    row["overview"] = response["overview"].get("stats")
                else:
                    snap = response["snapshot"]
                    row.update(view_shape(snap))
                    if name == "first":
                        snapshot = snap
                    else:
                        row["opened"] = requests["open"]["nodeId"]
            print(json.dumps(row), flush=True)
            with open(out, "a") as f:
                f.write(json.dumps(row) + "\n")
            request_path.unlink()
            response_path.unlink(missing_ok=True)
    print(f"Wrote {out}")


if __name__ == "__main__":
    main()
