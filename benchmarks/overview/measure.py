#!/usr/bin/env python3
"""Times the `overview` operation (ADR 0018) natively, through the `layout-command` process (JSON in and out included),
on the hierarchical graphs of `generate.py`. Writes one JSON line per (size, threads) to
benchmarks/results/overview-<UTC>.jsonl with machine, revision, load and the median, min and max of `--reps` runs after one
unmeasured warm-up. Run on an otherwise idle machine:

    cargo build --release -p arqlet-runtime --bin layout-command --locked
    python3 benchmarks/overview/measure.py --sizes 10000 50000 100000 --threads 1 4 --reps 5
"""
import argparse, datetime, json, os, platform, statistics, subprocess, sys, time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
BINARY = ROOT / "target/release/layout-command"
sys.path.insert(0, str(Path(__file__).resolve().parent))
from generate import generate  # noqa: E402

def machine():
    def sh(*cmd):
        try:
            return subprocess.run(cmd, capture_output=True, text=True, check=True).stdout.strip()
        except Exception:
            return None
    return {"platform": platform.platform(), "cpu": sh("sysctl", "-n", "machdep.cpu.brand_string") or platform.processor(),
            "cores": os.cpu_count(), "revision": sh("git", "-C", str(ROOT), "rev-parse", "--short", "HEAD"),
            "dirty": bool(sh("git", "-C", str(ROOT), "status", "--porcelain")), "build": "release (lto thin)"}

def main():
    p = argparse.ArgumentParser()
    p.add_argument("--sizes", type=int, nargs="+", default=[10000, 50000, 100000])
    p.add_argument("--threads", type=int, nargs="+", default=[1, 4])
    p.add_argument("--reps", type=int, default=5)
    p.add_argument("--seed", type=int, default=1)
    a = p.parse_args()
    if not BINARY.exists():
        sys.exit(f"Build first: cargo build --release -p arqlet-runtime --bin layout-command --locked ({BINARY} missing)")
    stamp = datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    out = ROOT / f"benchmarks/results/overview-{stamp}.jsonl"
    info = machine()
    for n in a.sizes:
        graph = generate(n, a.seed)
        for threads in a.threads:
            request = json.dumps({"op": "overview", "graph": graph, "threads": threads})
            times, stats, fingerprint = [], None, None
            for rep in range(a.reps + 1):
                started = time.perf_counter()
                result = subprocess.run([str(BINARY)], input=request, capture_output=True, text=True, check=True)
                elapsed = time.perf_counter() - started
                response = json.loads(result.stdout)
                if not response["ok"]:
                    sys.exit(response["error"])
                stats = response["overview"]["stats"]
                current = hash(result.stdout)
                if fingerprint is not None and current != fingerprint:
                    sys.exit("Output changed between runs: the overview must be deterministic")
                fingerprint = current
                if rep:
                    times.append(elapsed)
            row = {"dataset": f"overview-code-{n}", "seed": a.seed, "threads": threads, "reps": a.reps,
                   "medianSeconds": round(statistics.median(times), 4), "minSeconds": round(min(times), 4),
                   "maxSeconds": round(max(times), 4), "loadAverage": os.getloadavg(), "stats": stats,
                   "scope": "process: JSON parse, overview, JSON write", **info}
            print(json.dumps(row))
            with open(out, "a") as f:
                f.write(json.dumps(row) + "\n")
    print(f"Wrote {out}")

if __name__ == "__main__":
    main()
