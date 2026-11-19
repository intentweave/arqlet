#!/usr/bin/env python3
"""Import time of one organic area above and below the dense bound (300 children): rings with a chord from every fifth
node, placed by the dense stress up to 300 and by the scalable stress above (the automatic fallback). Each size is run
`--reps` times with `target/release/snapshot`; the median and the peak memory of the process are printed as a Markdown
table, and every run as a JSON line to `--out`.

    cargo build --release -p arqlet-runtime --bin snapshot
    python3 benchmarks/scaling/organic_fallback.py --sizes 200 300 301 500 1000 1500 2000 --reps 3
"""
import argparse
import json
import os
import re
import statistics
import subprocess
import tempfile
import time


def ring(n):
    nodes = [{"id": f"n{i}", "kind": "ecu", "label": f"n{i}"} for i in range(n)]
    edges = [{"id": f"r{i}", "kind": "link", "source": f"n{i}", "target": f"n{(i + 1) % n}"} for i in range(n)]
    edges += [{"id": f"c{i}", "kind": "link", "source": f"n{i}", "target": f"n{(i * 7 + 3) % n}"} for i in range(0, n, 5)]
    return {"defaultStrategy": "organic", "nodes": nodes, "edges": edges}


def run(binary, graph_path, out_path):
    if os.path.exists(out_path):
        os.remove(out_path)
    start = time.perf_counter()
    done = subprocess.run(["/usr/bin/time", "-l", binary, graph_path, out_path, "10000"], capture_output=True, text=True)
    seconds = time.perf_counter() - start
    if done.returncode != 0:
        lines = [l for l in done.stderr.splitlines() if l.strip() and not l.startswith(" ")]
        return seconds, None, lines[0] if lines else "failed"
    peak = re.search(r"(\d+)\s+maximum resident set size", done.stderr)
    return seconds, int(peak.group(1)) if peak else None, None


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--sizes", type=int, nargs="+", default=[200, 300, 301, 500, 1000, 1500, 2000])
    parser.add_argument("--reps", type=int, default=3)
    parser.add_argument("--binary", default="target/release/snapshot")
    parser.add_argument("--out", default=None)
    args = parser.parse_args()
    rows = []
    with tempfile.TemporaryDirectory() as tmp:
        for n in args.sizes:
            graph_path = os.path.join(tmp, f"ring{n}.json")
            out_path = os.path.join(tmp, f"ring{n}.out.json")
            with open(graph_path, "w") as f:
                json.dump(ring(n), f)
            times, peaks, error = [], [], None
            for rep in range(args.reps):
                seconds, peak, error = run(args.binary, graph_path, out_path)
                if error:
                    break
                times.append(seconds)
                peaks.append(peak or 0)
            scalable = None
            if not error:
                with open(out_path) as f:
                    scalable = bool(json.load(f).get("options", {}).get("scalableStress"))
            row = {"nodes": n, "edges": n + len(range(0, n, 5)), "times": times, "peakBytes": max(peaks) if peaks else None,
                   "scalableStress": scalable, "error": error}
            rows.append(row)
            if args.out:
                with open(args.out, "a") as f:
                    f.write(json.dumps(row) + "\n")
    print("| nodes | edges | placement | median | runs | peak memory |")
    print("|---|---|---|---|---|---|")
    for r in rows:
        if r["error"]:
            print(f"| {r['nodes']} | {r['edges']} | refused | | | ({r['error']}) |")
            continue
        placement = "scalable (fallback)" if r["scalableStress"] else "dense"
        print(f"| {r['nodes']} | {r['edges']} | {placement} | {statistics.median(r['times']):.2f} s | "
              f"{', '.join(f'{t:.2f}' for t in r['times'])} | {r['peakBytes'] / 2**20:.0f} MiB |")


if __name__ == "__main__":
    main()
