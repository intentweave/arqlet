#!/usr/bin/env python3
"""Profile runner: the four fixed configurations, measured with one metric set.

  python3 benchmarks/profile_runner.py freeze            copy the chosen views into benchmarks/inputs/
  python3 benchmarks/profile_runner.py run [--reps N]    measure every frozen input with every profile

A run refuses an input whose SHA-256 differs from benchmarks/inputs/manifest.json, so the test graphs
cannot change silently. Each profile is a versioned preset in presets/ (shared with the browser app). Results go to
benchmarks/results/profiles-<UTC time>.jsonl and a Markdown table next to it. The metrics come from
the engine's `metrics` operation for every run, including the first image, so a value is never
"not computed". Standard library only.
"""
import argparse
import datetime
import hashlib
import json
import os
import platform
import re
import signal
import shutil
import statistics
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
BIN = ROOT / "target/release/layout-command"
INPUTS = ROOT / "benchmarks/inputs"
PRESETS = ROOT / "presets"  # the same files the browser app loads
RESULTS = ROOT / "benchmarks/results"
DATASETS = ROOT / "benchmarks/datasets"
PROFILE_ORDER = ["vorschau", "raster", "lesbar", "qualitaet"]
# Frozen views: dataset, view limit (nodes taken into the view), and why it is in the set.
VIEWS = [
    ("network-vlan", 150, "dense infrastructure view, containers"),
    ("state-tcp", 100, "cyclic state machine, back edges"),
    ("workflow-dag", 100, "layered DAG with many edges"),
    ("synthetic-2k", 50, "generated view, many shared stubs"),
    ("org-chart", 100, "tree with a container"),
    ("bpmn-pools", 100, "pools and lanes"),
    ("uml-class", 100, "inheritance and association"),
    ("layerweave-code", 100, "this repository's own call graph (frozen at its generation)"),
]


def sha256(path):
    digest = hashlib.sha256()
    with open(path, "rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def git(*args):
    return subprocess.run(["git", *args], cwd=ROOT, capture_output=True, text=True).stdout.strip()


def freeze():
    INPUTS.mkdir(parents=True, exist_ok=True)
    entries = []
    for name, limit, why in VIEWS:
        source = DATASETS / f"{name}.json"
        target = INPUTS / f"{name}.json"
        if not source.exists():
            sys.exit(f"missing {source}: run benchmarks/generate.py first")
        shutil.copyfile(source, target)
        graph = json.loads(target.read_text())
        entries.append({
            "name": name,
            "file": target.name,
            "viewLimit": limit,
            "why": why,
            "sha256": sha256(target),
            "bytes": target.stat().st_size,
            "nodes": len(graph["nodes"]),
            "edges": len(graph["edges"]),
        })
    manifest = {"version": 1, "frozenFrom": "benchmarks/datasets", "frozenAt": git("rev-parse", "--short", "HEAD"), "inputs": entries}
    (INPUTS / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    for e in entries:
        print(f"{e['name']:18} {e['sha256'][:12]}  {e['nodes']} nodes  {e['edges']} edges  limit {e['viewLimit']}")


def run_process(command, payload, timeout):
    """Kill the entire timer/engine process group on timeout, then reap it."""
    with subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                          stderr=subprocess.PIPE, text=True, start_new_session=True) as process:
        try:
            stdout, stderr = process.communicate(payload, timeout=timeout)
        except subprocess.TimeoutExpired:
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            process.communicate()
            raise
        return subprocess.CompletedProcess(command, process.returncode, stdout, stderr)


def snapshot_hash(snapshot):
    return hashlib.sha256(json.dumps(snapshot, sort_keys=True, separators=(",", ":"), allow_nan=False).encode()).hexdigest()


def call(request, timeout):
    """Runs one engine operation. Returns (response, milliseconds, peak resident bytes)."""
    time_flag = ["-l"] if platform.system() == "Darwin" else ["-v"]
    timer = ["/usr/bin/time", *time_flag] if os.path.exists("/usr/bin/time") else []
    started = time.perf_counter()
    result = run_process([*timer, str(BIN)], json.dumps(request), timeout)
    elapsed = (time.perf_counter() - started) * 1000
    peak = peak_bytes(result.stderr)
    if result.returncode != 0 and not timer:
        raise RuntimeError(result.stderr.strip()[-400:])
    stdout = result.stdout
    if timer and result.returncode != 0 and not stdout:
        raise RuntimeError(result.stderr.strip()[-400:])
    response = json.loads(stdout)
    return response, elapsed, peak


def peak_bytes(stderr):
    # macOS `time -l` reports bytes; GNU `time -v` reports kilobytes.
    match = re.search(r"(\d+)\s+maximum resident set size", stderr)
    if match:
        return int(match.group(1))
    match = re.search(r"Maximum resident set size \(kbytes\):\s*(\d+)", stderr)
    return int(match.group(1)) * 1024 if match else None


def drawing_of(snapshot):
    """Absolute node rectangles and edge polylines of a snapshot (bundle members are not drawn)."""
    graph = snapshot["graph"]
    parents = {n["id"]: n.get("parentId") for n in graph["nodes"]}
    containers = {p for p in parents.values() if p}

    def absolute(node_id):
        box = dict(snapshot["boxes"][node_id])
        parent = parents[node_id]
        while parent:
            box["x"] += snapshot["boxes"][parent]["x"]
            box["y"] += snapshot["boxes"][parent]["y"]
            parent = parents[parent]
        return box

    routes = {r["id"]: r for r in snapshot["routes"]}
    nodes = []
    for n in graph["nodes"]:
        box = absolute(n["id"])
        nodes.append({"id": n["id"], "parentId": parents[n["id"]], "container": n["id"] in containers,
                      "x": box["x"], "y": box["y"], "width": box["width"], "height": box["height"]})
    # Port identities: only an explicit port shared by two edges may share a stub (metrics v3).
    ports = {(p["edgeId"], p["endpoint"]): p["id"] for p in snapshot.get("resolvedPorts", [])}
    edges = []
    for e in graph["edges"]:
        if e.get("target") is None:
            continue
        route = routes.get(e["id"], {})
        leader = route.get("bundle_of")
        drawn = routes.get(leader, {}) if leader else route
        if len(drawn.get("points", [])) < 2:
            raise RuntimeError(f"Missing or incomplete route: {e['id']}")
        if leader:
            continue
        edges.append({"id": e["id"], "source": e["source"], "target": e["target"], "points": routes[e["id"]]["points"],
                      "sourcePort": ports.get((e["id"], "source")), "targetPort": ports.get((e["id"], "target"))})
    return {"nodes": nodes, "edges": edges}


def measure(snapshot, timeout):
    response, ms, peak = call({"op": "metrics", "drawing": drawing_of(snapshot)}, timeout)
    if not response.get("ok"):
        raise RuntimeError(response.get("error", "metrics failed"))
    return response["metrics"], ms, peak


def run_once(graph, limit, profile, timeout):
    """Import, configure (first arrangement), measure, then refine when the profile asks for it."""
    peaks, timings = [], {}
    response, timings["importMs"], peak = call({"op": "import", "graph": graph, "limit": limit}, timeout)
    peaks.append(peak)
    if not response.get("ok"):
        raise RuntimeError(response.get("error"))
    snapshot = response["snapshot"]
    first_metrics, _, peak = measure(snapshot, timeout)
    peaks.append(peak)
    response, timings["configureMs"], peak = call({
        "op": "configure", "snapshot": snapshot, "expectedRevision": snapshot["revision"],
        "expectedLayoutVersion": snapshot["layoutVersion"], "options": profile["options"],
    }, timeout)
    peaks.append(peak)
    if not response.get("ok"):
        raise RuntimeError(response.get("error"))
    snapshot = response["snapshot"]
    arranged_metrics, _, peak = measure(snapshot, timeout)
    peaks.append(peak)
    timings["refineMs"] = None
    if profile["refine"]:
        response, timings["refineMs"], peak = call({
            "op": "refine", "snapshot": snapshot, "expectedRevision": snapshot["revision"],
            "expectedLayoutVersion": snapshot["layoutVersion"], "threads": profile.get("threads", 1),
            **({"budgetMs": max(0, int(profile["budgetMs"] - sum(v or 0 for v in timings.values())))}
               if "budgetMs" in profile else {}),
            # The deterministic cut: the work budget counts candidate jobs and is set so that it covers every job.
            **({"workBudget": profile["workBudget"]} if "workBudget" in profile else {}),
        }, timeout)
        peaks.append(peak)
        if not response.get("ok"):
            raise RuntimeError(response.get("error"))
        snapshot = response["snapshot"]
    final_metrics, _, peak = measure(snapshot, timeout)
    peaks.append(peak)
    known = [p for p in peaks if p]
    return {
        "first": first_metrics,
        "arranged": arranged_metrics,
        "final": final_metrics,
        "timings": timings,
        "peakRssMb": round(max(known) / 1e6, 1) if known else None,
        "strategy": snapshot.get("optimization", {}).get("strategy"),
        "interrupted": snapshot.get("optimization", {}).get("interrupted", False),
        "stopReason": snapshot.get("optimization", {}).get("stopReason"),
        "snapshot": snapshot,
    }


def load_profiles():
    """Each preset is sent as a strict config set; run policy (refine, budget, direction) stays outside it."""
    profiles = {}
    for name in PROFILE_ORDER:
        path = PRESETS / f"{name}.json"
        preset = json.loads(path.read_text())
        profiles[name] = {
            "id": preset["id"],
            "version": preset["version"],
            "title": preset["title"],
            "graph": preset["graph"],
            "refine": preset["refine"],
            "budgetMs": preset["budgetMs"],
            "workBudget": preset.get("workBudget"),
            "options": {"configSet": preset["configSet"]},
            "sha256": sha256(path),
        }
    return profiles


def environment():
    dirty = len([l for l in git("status", "--porcelain").splitlines() if l])
    cpu = subprocess.run(["sysctl", "-n", "machdep.cpu.brand_string"], capture_output=True, text=True).stdout.strip()
    rustc = subprocess.run(["rustc", "--version"], capture_output=True, text=True).stdout.strip()
    return {
        "gitRevision": git("rev-parse", "--short", "HEAD"),
        "gitDirty": dirty > 0,
        "uncommittedFiles": dirty,
        "platform": f"{platform.system()} {platform.machine()}",
        "cpu": cpu,
        "rustc": rustc,
        "python": platform.python_version(),
        "build": "release",
        "binarySha256": sha256(BIN),
    }


def run(reps, threads, timeout, selected_inputs=None, selected_profiles=None):
    if reps < 1 or threads < 1 or timeout <= 0:
        raise ValueError("reps, threads and timeout must be positive")
    stamp = datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    RESULTS.mkdir(parents=True, exist_ok=True)
    out = RESULTS / f"profiles-{stamp}.jsonl"
    snapshots = RESULTS / f"profiles-{stamp}-snapshots"
    snapshots.mkdir(exist_ok=True)
    manifest = json.loads((INPUTS / "manifest.json").read_text())
    profiles = load_profiles()
    env = environment()
    rows = []
    with out.open("w") as f:
        for entry in manifest["inputs"]:
            if selected_inputs and entry["name"] not in selected_inputs:
                continue
            path = INPUTS / entry["file"]
            if sha256(path) != entry["sha256"]:
                sys.exit(f"{entry['name']}: input changed since it was frozen ({path})")
            base_graph = json.loads(path.read_text())
            for name in PROFILE_ORDER:
                if selected_profiles and name not in selected_profiles:
                    continue
                profile = dict(profiles[name], threads=threads)
                graph = dict(base_graph, **profile["graph"])
                runs = []
                row = {"input": entry["name"], "inputSha256": entry["sha256"], "viewLimit": entry["viewLimit"],
                       "profile": name, "profileVersion": profile["version"], "profileSha256": profile["sha256"],
                       "profileOptions": profile["options"], "profileGraph": profile["graph"], "refine": profile["refine"],
                       "threads": threads, "reps": reps, "budgetMs": profile["budgetMs"],
                       "budgetPolicy": "cooperative-refinement-v1", "env": env}
                try:
                    for _ in range(reps):
                        runs.append(run_once(graph, entry["viewLimit"], profile, timeout))
                except (RuntimeError, subprocess.TimeoutExpired) as error:
                    row["status"] = "failed"
                    row["error"] = str(error)[-400:]
                    f.write(json.dumps(row) + "\n")
                    rows.append(row)
                    print(f"{entry['name']:18} {name:9} FAILED: {row['error'][:120]}")
                    continue
                first = runs[0]
                row["status"] = "ok"
                row.update({k: first[k] for k in ("first", "arranged", "final", "peakRssMb", "strategy", "interrupted")})
                row["interruptedRuns"] = [r["interrupted"] for r in runs]
                row["stopReasons"] = [r["stopReason"] for r in runs]
                row["snapshotSha256"] = snapshot_hash(first["snapshot"])
                row["deterministic"] = all(snapshot_hash(r["snapshot"]) == row["snapshotSha256"] for r in runs)
                totals = [sum(v for v in r["timings"].values() if v) for r in runs]
                row["timings"] = first["timings"]
                row["totalMs"] = {"median": round(statistics.median(totals), 1), "all": [round(t, 1) for t in totals]}
                row["overBudget"] = statistics.median(totals) > profile["budgetMs"]
                snap_path = snapshots / f"{entry['name']}-{name}.snapshot.json"
                snap_path.write_text(json.dumps(first["snapshot"]))
                row["snapshot"] = str(snap_path.relative_to(ROOT))
                f.write(json.dumps(row) + "\n")
                rows.append(row)
                fm = row["final"]
                print(f"{entry['name']:18} {name:9} crossings {fm['crossings']:4}  line-node {fm['lineNodeHits']:3}  "
                      f"overlaps {fm['siblingOverlaps'] + fm['crossBranchOverlaps']:3}  shared {fm['sharedLinePairs']:3}  "
                      f"hug {fm['hugPairs']:4}  bends {fm['bends']:4}  length {fm['length']:9.0f}  {row['totalMs']['median']:9.1f} ms")
    write_markdown(out.with_suffix(".md"), rows, env, reps, threads)
    print(f"results: {out.relative_to(ROOT)}")


def write_markdown(path, rows, env, reps, threads):
    lines = [
        "# Profile run", "",
        f"Revision `{env['gitRevision']}`{' (uncommitted changes)' if env['gitDirty'] else ''}, {env['cpu']}, {env['rustc']}, "
        f"{env['platform']}, threads {threads}, repetitions {reps}.",
        "Metrics are for the final layout. Time is the median total over repetitions; the first image is the "
        "configure step. Snapshot determinism is checked across repetitions; one repetition alone does not establish it. Time is machine-specific.", "",
        "| input | profile | crossings | line–node | sibling ovl. | cross-branch ovl. | shared lines | hugging pairs | bends | length | mean edge | max edge | total ms | budget | RSS MB | status |",
        "|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|",
    ]
    for row in rows:
        if row["status"] != "ok":
            lines.append(f"| {row['input']} | {row['profile']} | | | | | | | | | | | | | | failed: {row['error'][:60]} |")
            continue
        fm = row["final"]
        lines.append(
            f"| {row['input']} | {row['profile']} | {fm['crossings']} | {fm['lineNodeHits']} | {fm['siblingOverlaps']} | "
            f"{fm['crossBranchOverlaps']} | {fm['sharedLinePairs']} | {fm['hugPairs']} | {fm['bends']} | "
            f"{fm['length']:.0f} | {fm['lengthMean']:.0f} | {fm['lengthMax']:.0f} | {row['totalMs']['median']:.0f} | "
            f"{row['budgetMs']} | {row['peakRssMb']} | {'interrupted' if any(row.get('interruptedRuns', [])) else ('over budget' if row['overBudget'] else 'ok')} |"
        )
    path.write_text("\n".join(lines) + "\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("freeze")
    runner = sub.add_parser("run")
    runner.add_argument("--reps", type=int, default=1)
    runner.add_argument("--threads", type=int, default=1)
    runner.add_argument("--timeout", type=int, default=600, help="seconds per engine operation")
    runner.add_argument("--inputs", nargs="+")
    runner.add_argument("--profiles", nargs="+", choices=PROFILE_ORDER)
    args = parser.parse_args()
    if args.command == "freeze":
        freeze()
    else:
        run(args.reps, args.threads, args.timeout, args.inputs, args.profiles)


if __name__ == "__main__":
    main()
