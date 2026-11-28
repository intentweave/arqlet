#!/usr/bin/env python3
"""Louvain against Leiden (#237, ADR 0018 addendum of 2026-11-28) on generated graphs and the reference views.

Two parts:
1. The algorithms alone, per container that `with_communities` splits (the ignored test
   `crates/layout/tests/community_eval.rs`, release build): modularity of the final partition, communities without a
   link between their parts, number and size of communities, levels, work units and time; and per graph the whole
   clustering (clusters added, work, time).
2. The `overview` operation end to end through the release `layout-command` (JSON in and out included), Louvain and
   Leiden, median of `--reps` runs after one unmeasured warm-up; the answer must be the same in every run.

Writes benchmarks/results/communities-<UTC>.jsonl (one line per row, the first line the machine) and prints a
Markdown summary. The machine may be shared: the load average is recorded before and after.

    python3 benchmarks/generate.py --no-manifest
    cargo build --release -p arqlet-runtime --bin layout-command --locked
    python3 benchmarks/overview/communities.py
"""
import argparse, datetime, json, os, platform, statistics, subprocess, sys, time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
BINARY = ROOT / "target/release/layout-command"
DATASETS = ROOT / "benchmarks/datasets"
sys.path.insert(0, str(Path(__file__).resolve().parent))
from generate import generate, flatten  # noqa: E402

REFERENCE = ["layerweave-code", "synthetic-2k", "synthetic-20k", "organic-300", "mixed-large"]


def sh(*cmd):
    try:
        return subprocess.run(cmd, capture_output=True, text=True, check=True, timeout=30).stdout.strip()
    except Exception:
        return None


def load():
    return sh("sysctl", "-n", "vm.loadavg") or (" ".join(f"{x:.2f}" for x in os.getloadavg()))


def native_platform():
    """The machine's own platform: Python may run translated (Rosetta on Apple silicon) and then names x86_64."""
    translated = sh("sysctl", "-n", "sysctl.proc_translated") == "1"
    arm64 = sh("sysctl", "-n", "hw.optional.arm64") == "1"
    arch = "arm64" if arm64 else platform.machine()
    system = f"{platform.system()} {platform.mac_ver()[0] or platform.release()}"
    return f"{system} {arch}" + (" (this Python runs translated)" if translated else "")


def machine():
    return {"row": "machine", "platform": native_platform(),
            "cpu": sh("sysctl", "-n", "machdep.cpu.brand_string") or platform.processor(), "cores": os.cpu_count(),
            "revision": sh("git", "-C", str(ROOT), "rev-parse", "--short", "HEAD"),
            # Tracked files only: the results file this run writes is untracked.
            "dirty": bool(sh("git", "-C", str(ROOT), "status", "--porcelain", "--untracked-files=no")),
            "rustc": sh("rustc", "--version"), "build": "release (lto thin)", "loadBefore": load()}


def generated(sizes, flat_sizes, seed):
    out = []
    DATASETS.mkdir(parents=True, exist_ok=True)
    for n in sizes:
        path = DATASETS / f"overview-{n}.json"
        if not path.exists():
            path.write_text(json.dumps(generate(n, seed), separators=(",", ":")))
        out.append((f"overview-{n}", path))
    for n in flat_sizes:
        path = DATASETS / f"overview-flat-{n}.json"
        if not path.exists():
            path.write_text(json.dumps(flatten(generate(n, seed)), separators=(",", ":")))
        out.append((f"overview-flat-{n}", path))
    return out


def algorithms_alone(graphs, threshold, reps):
    env = dict(os.environ, ARQLET_COMMUNITY_EVAL=";".join(f"{n}={p}" for n, p in graphs),
               ARQLET_COMMUNITY_THRESHOLD=str(threshold), ARQLET_COMMUNITY_REPS=str(reps),
               CARGO_BUILD_JOBS=os.environ.get("CARGO_BUILD_JOBS", "2"))
    cmd = ["cargo", "test", "--release", "--locked", "-p", "arqlet-layout", "--test", "community_eval", "--",
           "--ignored", "--nocapture", "--test-threads", "1"]
    for attempt in range(2):
        try:
            r = subprocess.run(cmd, cwd=ROOT, env=env, capture_output=True, text=True, timeout=7200)
            if r.returncode == 0:
                break
            print(r.stderr[-2000:], file=sys.stderr)
        except subprocess.TimeoutExpired:
            print("timed out", file=sys.stderr)
    else:
        sys.exit("community_eval failed twice")
    rows = []
    for line in r.stdout.splitlines():
        # The test harness may print its own text before the first row on the same line.
        start = line.find('{"')
        if start >= 0:
            row = json.loads(line[start:])
            row["threshold"] = threshold
            rows.append(row)
    return rows


def end_to_end(graphs, reps):
    rows = []
    for dataset, path in graphs:
        graph = json.loads(path.read_text())
        for algorithm in ["louvain", "leiden"]:
            request = json.dumps({"op": "overview", "graph": graph, "threads": 1,
                                  "options": {"communityAlgorithm": algorithm}})
            times, answer = [], None
            for rep in range(reps + 1):
                started = time.perf_counter()
                result = subprocess.run([str(BINARY)], input=request, capture_output=True, text=True, check=True,
                                        timeout=1800)
                elapsed = time.perf_counter() - started
                response = json.loads(result.stdout)
                if not response["ok"]:
                    sys.exit(response["error"])
                if answer is not None and result.stdout != answer:
                    sys.exit("Output changed between runs: the overview must be deterministic")
                answer = result.stdout
                if rep:
                    times.append(elapsed)
            stats = json.loads(answer)["overview"]["stats"]
            rows.append({"row": "overview", "dataset": dataset, "algorithm": algorithm, "threads": 1, "reps": reps,
                         "medianSeconds": round(statistics.median(times), 4), "minSeconds": round(min(times), 4),
                         "maxSeconds": round(max(times), 4), "clusters": stats.get("communities"),
                         "nodes": stats.get("nodes")})
    return rows


def summary(rows):
    lines = []
    containers = [r for r in rows if r["row"] == "container"]
    keyed = {}
    for r in containers:
        keyed.setdefault((r["dataset"], r["threshold"]), {}).setdefault(r["algorithm"], []).append(r)
    lines.append("| dataset | threshold | containers | algorithm | mean modularity | disconnected (final) | members in them | "
                 "disconnected (any level) | communities ≥ 2 | ≥ 8 | largest | levels (max) | work | ms (sum) |")
    lines.append("|---|---|---|---|---|---|---|---|---|---|---|---|---|---|")
    for (dataset, threshold), by in keyed.items():
        for algorithm in ["louvain", "leiden"]:
            rs = by.get(algorithm, [])
            if not rs:
                continue
            lines.append("| {} | {} | {} | {} | {:.4f} | {} of {} | {} of {} | {} | {} | {} | {} | {} | {} | {:.1f} |".format(
                dataset, threshold, len(rs), algorithm, statistics.mean(r["modularity"] for r in rs),
                sum(r["disconnected"] for r in rs), sum(r["communities"] for r in rs),
                sum(r["disconnectedMembers"] for r in rs), sum(r["linked"] for r in rs),
                sum(r["disconnectedAnyLevel"] for r in rs), sum(r["communities"] for r in rs),
                sum(r["communitiesAtLeast8"] for r in rs), max(r["largest"] for r in rs), max(r["levels"] for r in rs),
                sum(r["work"] for r in rs), sum(r["medianMs"] for r in rs)))
    lines.append("")
    lines.append("| dataset | threshold | algorithm | clusters added | clustering work | clustering ms |")
    lines.append("|---|---|---|---|---|---|")
    for r in rows:
        if r["row"] == "graph":
            lines.append(f"| {r['dataset']} | {r['threshold']} | {r['algorithm']} | {r['clustersAdded']} | {r['work']} | {r['medianMs']} |")
    lines.append("")
    lines.append("| dataset | algorithm | overview s (median) | min | max | clusters |")
    lines.append("|---|---|---|---|---|---|")
    for r in rows:
        if r["row"] == "overview":
            lines.append(f"| {r['dataset']} | {r['algorithm']} | {r['medianSeconds']} | {r['minSeconds']} | {r['maxSeconds']} | {r['clusters']} |")
    return "\n".join(lines)


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--sizes", type=int, nargs="+", default=[10000, 50000, 100000])
    p.add_argument("--flat", type=int, nargs="+", default=[2000, 10000, 50000])
    p.add_argument("--reps", type=int, default=3)
    p.add_argument("--seed", type=int, default=1)
    p.add_argument("--no-overview", action="store_true", help="skip the end-to-end overview timings")
    a = p.parse_args()
    stamp = datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    out = ROOT / f"benchmarks/results/communities-{stamp}.jsonl"
    info = machine()
    graphs = generated(a.sizes, a.flat, a.seed)
    reference = [(n, DATASETS / f"{n}.json") for n in REFERENCE if (DATASETS / f"{n}.json").exists()]
    rows = algorithms_alone(graphs + reference, 120, a.reps)
    # The layout's grouping is used with smaller thresholds (40 in the structured view's results).
    rows += algorithms_alone(reference + [g for g in graphs if "flat-2000" in g[0]], 40, a.reps)
    if not a.no_overview:
        if not BINARY.exists():
            sys.exit(f"Build first: cargo build --release -p arqlet-runtime --bin layout-command --locked ({BINARY} missing)")
        rows += end_to_end(graphs, a.reps)
    info["loadAfter"] = load()
    with open(out, "w") as f:
        for row in [info] + rows:
            f.write(json.dumps(row, sort_keys=True) + "\n")
    print(f"Results: {out}")
    print(json.dumps(info))
    print(summary(rows))


if __name__ == "__main__":
    main()
