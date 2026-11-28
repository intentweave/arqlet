#!/usr/bin/env python3
"""Runs the request-peak measurement (benchmarks/session-memory, bin request-peak) over the calibration configurations of
the memory estimate (ADR 0024, C3) and appends every row to a JSON-lines file, one configuration at a time (a time limit
per configuration, retried once). Usage:

    python3 benchmarks/memory-estimate/sweep.py OUT.jsonl [--binary target/release/request-peak] [--only NAME,...]
    python3 benchmarks/memory-estimate/sweep.py OUT.jsonl --wasm target/wasm32-unknown-unknown/release/arqlet_wasm.wasm

Each row gets the machine's load averages at the start of its configuration (`load`).
"""
import argparse, json, os, subprocess, sys, time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
D = ROOT / "benchmarks" / "datasets"
DEPTH0 = '{"levelOfDetail":{"depth":0}}'
DEPTH1 = '{"levelOfDetail":{"depth":1}}'
ALL = "import,toggle,direction,patch,refine,refine4"

REFERENCE = ["activity-order", "arqlet-pipeline", "bpmn-pools", "can-bus", "cloud-architecture", "deployment-cluster",
             "er-diagram", "fixed-ports", "layerweave-code", "metro-map", "mixed-large", "mixed-layouts",
             "nested-tree-radial", "network-vlan", "org-chart", "petri-net", "petri-net-shapes", "self-loops",
             "state-order", "state-tcp", "uml-class", "vehicle-buses", "vehicle-metro", "vehicle-trunk",
             "vehicle-zonal", "workflow-dag", "organic-300", "tree-300"]


def configs():
    for name in REFERENCE:
        yield name, 100, "{}", ALL
    for name in ("synthetic-2k", "synthetic-5k", "synthetic-20k", "synthetic-50k", "synthetic-100k"):
        yield name, 100, DEPTH0, ALL
        yield name, 300, DEPTH1, ALL
    for name in ("synthetic-2k", "synthetic-5k", "synthetic-20k"):
        yield name, 1000, "{}", ALL
    yield "synthetic-2k", 2286, "{}", ALL
    yield "full-1000", 1100, "{}", ALL
    yield "full-3000", 3300, "{}", ALL
    for name in ("code-10000", "code-50000", "code-100000"):
        yield name, 100, DEPTH0, ALL + ",overview"
    for name in ("synthetic-2k", "synthetic-20k", "synthetic-100k"):
        yield name, 100, DEPTH0, "overview"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("out", type=Path)
    ap.add_argument("--binary", default=str(ROOT / "target" / "release" / "request-peak"))
    ap.add_argument("--only", default="")
    ap.add_argument("--skip", default="", help="NAME:LIMIT,... configurations to leave out (done already, or too slow)")
    ap.add_argument("--wasm", default="", help="measure the WASM module at this path (wasm-peak.mjs) instead")
    ap.add_argument("--timeout", type=int, default=1800)
    ap.add_argument("--env", action="append", default=[], help="KEY=VALUE for the measured process")
    args = ap.parse_args()
    only = set(filter(None, args.only.split(",")))
    env = dict(os.environ)
    for kv in args.env:
        k, v = kv.split("=", 1)
        env[k] = v
    for name, limit, options, ops in configs():
        if only and name not in only:
            continue
        if f"{name}:{limit}" in set(filter(None, args.skip.split(","))):
            continue
        path = D / f"{name}.json"
        if not path.exists():
            print(f"missing {path}", file=sys.stderr)
            continue
        load = list(os.getloadavg())
        for attempt in (1, 2):
            try:
                t = time.time()
                command = ([args.binary] if not args.wasm else
                           ["node", "--max-old-space-size=8192", str(ROOT / "benchmarks" / "memory-estimate" / "wasm-peak.mjs"),
                            args.wasm])
                out = subprocess.run(command + [str(path), str(limit), options, ops], capture_output=True,
                                     text=True, timeout=args.timeout, env=env)
                if out.returncode != 0:
                    raise RuntimeError(out.stderr[-500:])
                with args.out.open("a") as f:
                    for line in out.stdout.splitlines():
                        row = json.loads(line)
                        row["load"] = load
                        f.write(json.dumps(row) + "\n")
                print(f"{name} {limit} {options}: {time.time() - t:.1f} s", file=sys.stderr, flush=True)
                break
            except Exception as e:  # noqa: BLE001 (a hung or failed run is retried once, then reported)
                print(f"{name} {limit} {options} attempt {attempt}: {e}", file=sys.stderr, flush=True)
                if attempt == 2:
                    with args.out.open("a") as f:
                        f.write(json.dumps({"graph": name, "limit": limit, "options": options, "failed": str(e)[:300],
                                            "load": load}) + "\n")


if __name__ == "__main__":
    main()
