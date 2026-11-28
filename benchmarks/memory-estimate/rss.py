#!/usr/bin/env python3
"""Resident memory of the native engine for single requests (ADR 0024, C3), next to the engine's estimate: one
`layout-command` process per request (stdin to stdout), its peak resident set size from `/usr/bin/time -l` (macOS) or
`-v` (Linux), less that of a trivial request (`optionDefaults`) in the same binary. The estimate counts heap bytes; RSS
adds the allocator's own overhead and fragmentation, which this measures.

    python3 benchmarks/memory-estimate/rss.py OUT.jsonl [--binary target/release/layout-command]

Each request has a time limit and is retried once; rows are appended as they finish.
"""
import argparse, json, os, platform, re, subprocess, sys, tempfile, time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
D = ROOT / "benchmarks" / "datasets"
DEPTH0 = {"levelOfDetail": {"depth": 0}}
CONFIGS = [
    ("synthetic-2k", 100, DEPTH0), ("synthetic-20k", 100, DEPTH0), ("full-1000", 1100, {}), ("full-3000", 3300, {}),
    ("code-50000", 100, DEPTH0),
]


def run(binary, text, timeout):
    flag = "-l" if platform.system() == "Darwin" else "-v"
    with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as f:
        f.write(text)
        path = f.name
    try:
        for attempt in (1, 2):
            try:
                with open(path) as stdin:
                    out = subprocess.run(["/usr/bin/time", flag, binary], stdin=stdin, capture_output=True, text=True,
                                         timeout=timeout)
                m = re.search(r"(\d+)\s+maximum resident set size", out.stderr) or \
                    re.search(r"Maximum resident set size \(kbytes\): (\d+)", out.stderr)
                rss = int(m.group(1)) * (1 if platform.system() == "Darwin" else 1024)
                return out.stdout, rss
            except Exception as e:  # noqa: BLE001 (retried once, then raised)
                if attempt == 2:
                    raise
                print(f"retry: {e}", file=sys.stderr)
    finally:
        os.unlink(path)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("out", type=Path)
    ap.add_argument("--binary", default=str(ROOT / "target" / "release" / "layout-command"))
    ap.add_argument("--timeout", type=int, default=1800)
    args = ap.parse_args()
    _, base = run(args.binary, '{"op":"optionDefaults"}', 60)
    for name, limit, options in CONFIGS:
        graph = (D / f"{name}.json").read_text().strip()
        load = list(os.getloadavg())
        requests = [("import", f'{{"op":"import","limit":{limit},"options":{json.dumps(options)},"graph":{graph}}}')]
        snapshot = None
        for op, text in requests:
            t = time.time()
            answer, rss = run(args.binary, text, args.timeout)
            est, _ = run(args.binary, f'{{"op":"estimate","request":{text}}}', args.timeout)
            row = {"graph": name, "limit": limit, "options": options, "op": op, "textBytes": len(text), "rss": rss,
                   "rssAboveBase": rss - base, "base": base, "estimate": json.loads(est).get("estimate"),
                   "seconds": round(time.time() - t, 1), "load": load}
            with args.out.open("a") as f:
                f.write(json.dumps(row) + "\n")
            print(name, op, rss // 2**20, "MB", file=sys.stderr, flush=True)
            if op == "import":
                snapshot = json.loads(answer)["snapshot"]
                collapsed = [c for c in snapshot.get("collapsedIds", []) if not c.startswith("rest:")]
                shown = {n["id"] for n in snapshot["graph"]["nodes"]}
                target = next((c for c in sorted(collapsed) if c in shown), None)
                edit = {"op": "toggle", "nodeId": target} if target else {"op": "direction", "direction": "lr"}
                edit.update({"expectedRevision": snapshot["revision"],
                             "expectedLayoutVersion": snapshot["layoutVersion"]})
                requests.append((edit["op"], json.dumps(edit)[:-1] + ',"snapshot":' + json.dumps(snapshot) + "}"))


if __name__ == "__main__":
    main()
