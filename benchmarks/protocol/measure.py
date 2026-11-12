"""ADR 0019, step 2: where the time of a request goes, natively and in WASM, at several graph sizes.

Generates `benchmarks/overview/generate.py` graphs (seed 1) of the sizes given, waits until the machine is quiet (load
average below `--max-load`), runs the native timing example, the WASM timing script and the resident-session comparison on each, and writes
benchmarks/results/protocol-<UTC>.jsonl: one line with the machine and revision, then one line per request and engine
(medians of `--reps` runs after a warm-up). Run on an otherwise idle machine:

    cargo build --release -p arqlet-runtime --example protocol_timing --locked
    cargo build -p arqlet-wasm --target wasm32-unknown-unknown --release --locked
    python3 benchmarks/protocol/measure.py --sizes 10000 50000 100000 --reps 5
"""
import argparse, datetime, json, os, platform, subprocess, sys, time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def sh(*cmd):
    return subprocess.run(cmd, capture_output=True, text=True).stdout.strip()


def load():
    return os.getloadavg()[0]


def machine():
    return {"machine": sh("sysctl", "-n", "machdep.cpu.brand_string") or platform.processor(),
            "os": f"{platform.system()} {platform.release()}", "cores": os.cpu_count(),
            "revision": sh("git", "-C", str(ROOT), "rev-parse", "--short", "HEAD"),
            "dirty": bool(sh("git", "-C", str(ROOT), "status", "--porcelain", "--untracked-files=no")),
            "rustc": sh("rustc", "--version"), "node": sh("node", "--version"),
            "load": round(load(), 2), "utc": datetime.datetime.now(datetime.timezone.utc).isoformat(timespec="seconds")}


def main():
    a = argparse.ArgumentParser()
    a.add_argument("--sizes", type=int, nargs="+", default=[10000, 50000, 100000])
    a.add_argument("--reps", type=int, default=5)
    a.add_argument("--max-load", type=float, default=3.0)
    a.add_argument("--wait", type=int, default=600, help="seconds to wait for a quiet machine")
    args = a.parse_args()
    native = ROOT / "target/release/examples/protocol_timing"
    wasm = ROOT / "target/wasm32-unknown-unknown/release/arqlet_wasm.wasm"
    for f in (native, wasm):
        if not f.exists():
            sys.exit(f"Missing {f}; build it first (see the docstring).")
    graphs = {}
    for n in args.sizes:
        path = ROOT / f"benchmarks/datasets/protocol-{n}.json"
        if not path.exists():
            subprocess.run([sys.executable, str(ROOT / "benchmarks/overview/generate.py"), "--nodes", str(n), "--seed", "1",
                            "--out", str(path)], check=True, capture_output=True)
        graphs[n] = path
    deadline = time.time() + args.wait
    while load() > args.max_load and time.time() < deadline:
        time.sleep(10)
    info = machine()
    if info["load"] > args.max_load:
        print(f"warning: load {info['load']} above {args.max_load}", file=sys.stderr)
    stamp = datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    out = ROOT / f"benchmarks/results/protocol-{stamp}.jsonl"
    rows = [info]
    for n, path in graphs.items():
        for cmd in ([str(native), str(path), str(args.reps)],
                    ["node", str(ROOT / "benchmarks/protocol/measure-wasm.mjs"), str(wasm), str(path), str(args.reps)],
                    ["node", str(ROOT / "benchmarks/protocol/measure-resident.mjs"), str(wasm), str(path), str(args.reps)]):
            result = subprocess.run(cmd, capture_output=True, text=True, check=True)
            for line in result.stdout.splitlines():
                row = json.loads(line)
                row["loadAfter"] = round(load(), 2)
                rows.append(row)
                print(json.dumps(row), flush=True)
    out.write_text("".join(json.dumps(r) + "\n" for r in rows))
    print(f"wrote {out.relative_to(ROOT)}", file=sys.stderr)


if __name__ == "__main__":
    main()
