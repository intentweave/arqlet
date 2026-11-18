#!/usr/bin/env python3
"""Scaling step 3, the resident path (ADR 0019 step 3): the same graphs and first view as `series.py`, through one
`layout-command --sessions` process that keeps the snapshot. Per size, the messages of one session:

  import         the first view (`levelOfDetail: {nodes: 1000, edges: 3000}`, `liftEdges`), with its scene
  open           `toggle` of the first collapsed container the scene shows, with the scene
  close          `toggle` of the same container again, without the scene (`"scene": false`)
  columns        the scene as columns (the numbers stay in the session; the answer describes them)
  export         the whole snapshot, for comparison with what a one-shot request carries

Per message: wall time from writing the line to reading the answer, and the bytes of message and answer; after the
last message, the process's resident memory (`ps`). Graphs stay in memory and in the pipe, never in the repository.
Writes benchmarks/results/sessions-<UTC>.jsonl (first line: machine). Single runs; for timings to compare, run on an
otherwise idle machine:

    cargo build --release -p arqlet-runtime --bin layout-command --locked
    python3 benchmarks/scaling/sessions.py --sizes 20000 100000 300000 1000000
"""
import argparse, datetime, json, os, subprocess, sys, time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
BINARY = ROOT / "target/release/layout-command"
sys.path.insert(0, str(Path(__file__).resolve().parent))
from series import FIRST_VIEW, machine, generate  # noqa: E402


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--sizes", type=int, nargs="+", default=[20000, 100000, 300000, 1000000])
    p.add_argument("--seed", type=int, default=1)
    p.add_argument("--out", type=Path, default=None)
    a = p.parse_args()
    stamp = datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    out = a.out or ROOT / f"benchmarks/results/sessions-{stamp}.jsonl"
    with open(out, "a") as f:
        f.write(json.dumps({"machine": machine(), "firstView": FIRST_VIEW, "seed": a.seed}) + "\n")
    for n in a.sizes:
        graph = generate(n, a.seed)
        proc = subprocess.Popen([str(BINARY), "--sessions"], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)

        def send(message):
            line = json.dumps(message)
            started = time.perf_counter()
            proc.stdin.write(line + "\n")
            proc.stdin.flush()
            answer = proc.stdout.readline()
            return time.perf_counter() - started, len(line), answer

        _, _, opened = send({"session": "open"})
        sid = json.loads(opened)["session"]
        rows = []

        def record(name, message):
            seconds, sent, answer = send({"session": sid, "request": message})
            value = json.loads(answer)
            row = {"size": n, "graphNodes": len(graph["nodes"]), "graphEdges": len(graph["edges"]), "message": name,
                   "seconds": round(seconds, 2), "messageBytes": sent, "answerBytes": len(answer), "ok": value.get("ok"),
                   "load": os.getloadavg()}
            if not value.get("ok"):
                row["error"] = value.get("error")
            rows.append(row)
            print(json.dumps(row), flush=True)
            return value

        first = record("import", {"op": "import", "graph": graph, "limit": 10000, "options": FIRST_VIEW})
        shown = {node["id"] for node in first.get("scene", {}).get("nodes", [])}
        target = next((c for c in first.get("collapsedIds", []) if c in shown), None)
        if target:
            record("open", {"op": "toggle", "nodeId": target})
            record("close", {"op": "toggle", "nodeId": target, "scene": False})
        record("columns", {"op": "columns"})
        record("export", {"op": "export"})
        rss = subprocess.run(["ps", "-o", "rss=", "-p", str(proc.pid)], capture_output=True, text=True).stdout.strip()
        for row in rows:
            row["processResidentBytes"] = int(rss) * 1024 if rss else None
        proc.stdin.close()
        proc.wait()
        with open(out, "a") as f:
            for row in rows:
                f.write(json.dumps(row) + "\n")
    print(f"Wrote {out}")


if __name__ == "__main__":
    main()
