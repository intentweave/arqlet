#!/usr/bin/env python3
"""Throughput baseline at up to 1M nodes (#249, step 1): what a resident session and a one-shot request cost per
operation, with the layout's phase split (`timings: true`) and, with `--profile`, a `sample` profile per message.

Graphs: `benchmarks/overview/generate.py` (code-index shaped, as `series.py` and `sessions.py`), written once to
`benchmarks/datasets/throughput-<n>-s<seed>.json` (git-ignored) and recorded with their SHA-256. First view as in
`series.py` (`levelOfDetail: {nodes: 1000, edges: 3000}`, `liftEdges`, limit 10 000).

Modes (`--mode`, several allowed):

  session   one `layout-command --sessions` process per repetition, the messages of one session in this order:
              import          the first view, with its scene (no timings: the plain path a backend takes)
              toggle-open     `toggle` of the first collapsed container the scene shows, with the scene
              toggle-close    the same container again, `scene: false`
              reveal          `reveal` of a symbol inside a collapsed container (opens the containers above it)
              columns         the scene as columns (ADR 0019 step 4)
              patch-small     a `patch` adding 5 symbols and 8 edges into one file (a container)
              patch-large     a `patch` adding 2 000 symbols and 4 000 edges into 20 files
              memory          the session's own estimate (`memory`)
              overview-first  `overview` with the graph and a work budget (`--overview-budget`), `threads`
              overview-byid   the same continued from the first answer's places by id (`start.nodes`), same budget
              import-timed    the import again with `timings: true` (the phase split of the import; the timed path
                              clones the request's JSON value, so its wall time is not the import's)
            Every message but `import` asks for `timings`. Per message: wall time (line written to line read), bytes
            sent and answered, the process's resident memory after it and the highest seen while it ran (`ps`, polled
            every 0.1 s), and the 1-minute load before it.
  oneshot   one `layout-command` process per request (`/usr/bin/time -l`: peak resident memory): import, toggle of
            the same container on the import's snapshot, and the small patch on it.

`--profile` (with `session`, one extra repetition): `sample` runs against the process during each message and the call
tree is split into categories (outermost match, see CATEGORIES); the top functions by own samples are kept too.

Rows go to `--out` (JSONL, appended); a repetition that is complete in the file is skipped, so a run resumes. Every
subprocess has a time limit; a repetition that times out is retried once. Before every repetition the driver waits
(up to `--max-wait` s) for the 1-minute load to fall below `--max-load` and records it either way.

    cargo build --release -p arqlet-runtime --bin layout-command --locked
    python3 benchmarks/scaling/throughput.py --sizes 100000 300000 1000000 --reps 3 --out benchmarks/results/throughput-1m-DATE.jsonl
"""
import argparse, hashlib, json, os, platform, queue, re, subprocess, sys, threading, time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(Path(__file__).resolve().parent))
from series import FIRST_VIEW, machine, generate  # noqa: E402

DATASETS = ROOT / "benchmarks/datasets"
WORK = ROOT / "target/throughput"


def dataset(n, seed):
    path = DATASETS / f"throughput-{n}-s{seed}.json"
    if not path.exists():
        DATASETS.mkdir(parents=True, exist_ok=True)
        tmp = path.with_suffix(".tmp")
        tmp.write_text(json.dumps(generate(n, seed), separators=(",", ":")))
        tmp.rename(path)
    text = path.read_text()
    return path, text, hashlib.sha256(text.encode()).hexdigest()


def plan(graph):
    """The deterministic edit targets of a graph: a file for the small patch, 20 files for the large one."""
    files = [n["id"] for n in graph["nodes"] if n.get("kind") == "file"]
    children = {}
    for node in graph["nodes"]:
        if node.get("parentId"):
            children.setdefault(node["parentId"], []).append(node["id"])
    sized = [f for f in files if len(children.get(f, [])) >= 2]

    def patch(targets, per_file, edges_per_node, tag):
        nodes, edges = [], []
        for f in targets:
            new = [f"{f}/{tag}{k}" for k in range(per_file)]
            nodes += [{"id": i, "kind": "symbol", "label": i.rsplit("/", 1)[1], "parentId": f} for i in new]
            old = children[f]
            for k, i in enumerate(new):
                for j in range(edges_per_node):
                    target = new[(k + 1) % len(new)] if j == 0 else old[(k * 7 + j) % len(old)]
                    edges.append({"id": f"{tag}-e{len(edges)}", "kind": "call", "source": i, "target": target})
        return {"nodes": nodes, "edges": edges}

    small = patch(sized[:1], 5, 2, "ps")
    small["edges"] = small["edges"][:8]
    large = patch(sized[1:21], 100, 2, "pl")
    return small, large


class Session:
    """A `layout-command --sessions` process with a reader thread, so every answer is waited for with a time limit."""

    def __init__(self, binary):
        self.proc = subprocess.Popen([str(binary), "--sessions"], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                     stderr=subprocess.DEVNULL)
        self.lines = queue.Queue()
        threading.Thread(target=self._read, daemon=True).start()

    def _read(self):
        for line in self.proc.stdout:
            self.lines.put(line)
        self.lines.put(None)

    def rss(self):
        out = subprocess.run(["ps", "-o", "rss=", "-p", str(self.proc.pid)], capture_output=True, text=True).stdout
        return int(out.strip()) * 1024 if out.strip() else None

    def send(self, line, timeout):
        """Writes one line; answers (seconds, answer bytes or None on timeout, highest resident bytes seen)."""
        peak = [self.rss() or 0]
        done = threading.Event()

        def poll():
            while not done.wait(0.1):
                r = self.rss()
                if r:
                    peak[0] = max(peak[0], r)

        threading.Thread(target=poll, daemon=True).start()
        started = time.perf_counter()
        try:
            self.proc.stdin.write(line.encode() + b"\n")
            self.proc.stdin.flush()
            answer = self.lines.get(timeout=timeout)
        except (queue.Empty, BrokenPipeError):
            answer = None
        seconds = time.perf_counter() - started
        done.set()
        return seconds, answer, peak[0]

    def close(self):
        try:
            self.proc.stdin.close()
            self.proc.wait(timeout=30)
        except Exception:
            pass
        if self.proc.poll() is None:
            self.proc.kill()
            self.proc.wait()


# Categories of a profile's call tree. A sample belongs to the first category in this list that matches any frame on its
# path from the thread's root (so JSON reading inside the overview counts as JSON reading, a graph clone inside a toggle
# as clone/drop, an index built inside the structure as index); samples under no match are `other`.
CATEGORIES = [
    ("json parse", [r"serde_json::de::", r"serde_json::value::de", r"from_value", r"Deserialize>::deserialize",
                    r"serde::de::"]),
    ("json write", [r"serde_json::ser", r"serde_json::value::ser", r"to_value", r"Serialize>::serialize"]),
    ("validate", [r"arqlet_model::validate"]),
    ("index", [r"GraphIndex"]),
    ("structure", [r"arqlet_runtime::structured", r"arqlet_layout::communities", r"select_visible",
                   r"initially_collapsed", r"level_budget_collapse", r"select_connected", r"structure_of"]),
    ("projection", [r"arqlet_view::project", r"merge_view", r"project_view"]),
    ("layout", [r"arqlet_layout::compute", r"arqlet_layout::layout_"]),
    ("scene", [r"arqlet_scene", r"scene_of", r"columns::"]),
    ("overview", [r"arqlet_layout::overview", r"arqlet_runtime::overview::"]),
    ("clone/drop", [r"drop_in_place", r"core::clone::Clone", r"::clone\b"]),
]
COMPILED = [(name, [re.compile(p) for p in pats]) for name, pats in CATEGORIES]
# A thread waiting (reading stdin, parked workers, the kernel): not work of the message.
IDLE = re.compile(r"__read_nocancel|__psynch_cvwait|__semwait|__ulock_wait|mach_msg|kevent|__workq_kernreturn|__wait4|"
                  r"__select|^read\b|__psynch_mutexwait|swtch_pri|thread_suspend")
# The nearest frame of the engine (or serde) a sample's own frame is charged to, for `engineTop`: string comparisons and
# allocations land on the function that asked for them.
ENGINE = re.compile(r"(arqlet_|serde_json::|serde::|<?alloc::collections)")
LINE = re.compile(r"^([\s+!:|]*)(\d+)\s+(.*?)(?:\s+\(in [^)]*\).*)?$")


SKIP = re.compile(r"[sB][0-9A-Za-z]*_")


def demangle(symbol):
    """Rust v0 symbols (`_RNv…`) to their path of identifiers (`arqlet_model::validate`), roughly: enough to match a
    category, not a full demangler. Other symbols are returned as they are."""
    if not symbol.startswith("_R"):
        return symbol
    out, i, n = [], 2, len(symbol)
    while i < n:
        m = SKIP.match(symbol, i)
        if m and m.end() > i + 1:
            i = m.end()
            continue
        if symbol[i].isdigit():
            j = i
            while j < n and symbol[j].isdigit():
                j += 1
            length = int(symbol[i:j])
            if j < n and symbol[j] == "_":
                j += 1
            out.append(symbol[j:j + length])
            i = j + length
            continue
        i += 1
    return "::".join(out)


def categorize(symbol):
    for k, (name, pats) in enumerate(COMPILED):
        if any(p.search(symbol) for p in pats):
            return k
    return None


def parse_sample(text):
    """The call graph of a `sample` report: samples per category (outermost match) and the top functions by own samples."""
    call = text.split("Call graph:", 1)[-1].split("Total number in stack", 1)[0]
    nodes = []  # (depth, count, symbol)
    for raw in call.splitlines():
        m = LINE.match(raw)
        if not m or not raw.strip():
            continue
        nodes.append((len(m.group(1)), int(m.group(2)), demangle(m.group(3).strip())))
    # stack: (depth, category, count, children count, symbol, nearest engine frame)
    shares, own, engine, stack = {}, {}, {}, []
    idle = memcmp = alloc = 0

    def pop():
        nonlocal idle, memcmp, alloc
        depth, cat, count, kids, symbol, frame = stack.pop()
        self_count = count - kids
        if self_count > 0:
            if IDLE.search(symbol):
                idle += self_count
                return
            key = "other" if cat is None else COMPILED[cat][0]
            shares[key] = shares.get(key, 0) + self_count
            own[symbol] = own.get(symbol, 0) + self_count
            engine[frame] = engine.get(frame, 0) + self_count
            if "memcmp" in symbol:
                memcmp += self_count
            if re.search(r"malloc|free|realloc|memmove|madvise|_xzm_", symbol):
                alloc += self_count

    for depth, count, symbol in nodes:
        while stack and stack[-1][0] >= depth:
            pop()
        if symbol.startswith("Thread_") or symbol.startswith("DispatchQueue"):
            stack.append((depth, None, count, count, symbol, symbol))  # a thread's root: no own samples
            continue
        parent_cat = stack[-1][1] if stack else None
        own_cat = categorize(symbol)
        cat = min((c for c in (parent_cat, own_cat) if c is not None), default=None)
        frame = symbol if ENGINE.match(symbol) else (stack[-1][5] if stack else symbol)
        if stack:
            d, c, k, kids, sy, fr = stack[-1]
            stack[-1] = (d, c, k, kids + count, sy, fr)
        stack.append((depth, cat, count, 0, symbol, frame))
    while stack:
        pop()
    busy = sum(own.values())
    top = sorted(own.items(), key=lambda kv: -kv[1])[:25]
    engine_top = sorted(engine.items(), key=lambda kv: -kv[1])[:25]
    return {"samples": busy, "idle": idle, "memcmp": memcmp, "allocator": alloc,
            "engineTop": [[s[:160], c] for s, c in engine_top],
            "categories": {k: v for k, v in sorted(shares.items(), key=lambda kv: -kv[1]) if v > 0},
            "top": [[s[:160], c] for s, c in top]}


def medians(path):
    """Median seconds per (size, message) of the session repetitions in the results file so far."""
    times = {}
    if path.exists():
        for line in path.read_text().splitlines():
            r = json.loads(line)
            if r.get("mode") == "session" and "seconds" in r and r.get("ok"):
                times.setdefault((r["size"], r["message"]), []).append(r["seconds"])
    return {k: sorted(v)[len(v) // 2] for k, v in times.items()}


def wait_for_load(max_load, max_wait):
    waited = 0
    while os.getloadavg()[0] >= max_load and waited < max_wait:
        time.sleep(15)
        waited += 15
    return {"loadBefore": [round(x, 2) for x in os.getloadavg()], "waitedSeconds": waited}


def session_rep(binary, n, graph_text, graph, sha, rep, args, write, profile_dir=None, estimates=None):
    small, large = plan(graph)
    s = Session(binary)
    rows = []
    try:
        _, opened, _ = s.send(json.dumps({"session": "open"}), 60)
        if opened is None:
            return False
        sid = json.loads(opened)["session"]
        state = {}

        def message(name, request, raw=None):
            body = raw if raw is not None else json.dumps(request)
            line = f'{{"session":{sid},"request":{body}}}'
            load = [round(x, 2) for x in os.getloadavg()]
            sampler = None
            if profile_dir is not None:
                out = profile_dir / f"{n}-{name}.sample.txt"
                # `sample` takes a fixed duration (it writes no report when interrupted): the median of the unprofiled
                # repetitions with a margin; the samples after the answer are the main thread waiting on stdin (idle).
                duration = max(2, int(estimates.get((n, name), 30) * 1.3 + 2))
                sampler = subprocess.Popen(["sample", str(s.proc.pid), str(duration), "1", "-mayDie", "-file",
                                            str(out)], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                time.sleep(0.5)
            seconds, answer, peak = s.send(line, args.timeout)
            if sampler is not None:
                try:
                    sampler.wait(timeout=duration + 300)
                except subprocess.TimeoutExpired:
                    sampler.kill()
                    sampler.wait()
            row = {"mode": "profile" if profile_dir else "session", "size": n, "rep": rep, "message": name,
                   "seconds": round(seconds, 3), "messageBytes": len(line), "load": load}
            if answer is None:
                row["status"] = "timeout"
                write(row)
                raise TimeoutError(name)
            value = json.loads(answer)
            row.update({"answerBytes": len(answer), "ok": value.get("ok"), "residentBytes": s.rss(),
                        "peakResidentPolled": peak})
            if not value.get("ok"):
                row["error"] = str(value.get("error"))[:300]
            if "timings" in value:
                row["timings"] = value["timings"]
            if name == "memory":
                row["estimate"] = {"total": value.get("total"), "parts": value.get("parts")}
            if name.startswith("overview") and value.get("ok"):
                o = value["overview"]
                work = {k: (len(v) if isinstance(v, list) else v) for k, v in (o.get("work") or {}).items()}
                row["overview"] = {"stats": o.get("stats"), "work": work, "ids": len(o.get("ids", []))}
            if profile_dir is not None:
                out = profile_dir / f"{n}-{name}.sample.txt"
                if out.exists():
                    row["profile"] = parse_sample(out.read_text(errors="replace"))
            write(row)
            rows.append(row)
            return value

        first = message("import", None, raw=json.dumps(
            {"op": "import", "limit": 10000, "options": FIRST_VIEW})[:-1] + ',"graph":' + graph_text + "}")
        shown = {node["id"] for node in first.get("scene", {}).get("nodes", [])}
        collapsed = first.get("collapsedIds", [])
        target = next((c for c in collapsed if c in shown), None)
        if target:
            message("toggle-open", {"op": "toggle", "nodeId": target, "timings": True})
            message("toggle-close", {"op": "toggle", "nodeId": target, "scene": False, "timings": True})
        # A symbol whose file is inside a collapsed container, as deep as the generator goes.
        hidden = set(collapsed)
        parent = {node["id"]: node.get("parentId") for node in graph["nodes"]}

        def under_collapsed(i):
            p = parent.get(i)
            while p:
                if p in hidden:
                    return True
                p = parent.get(p)
            return False

        symbol = next((x["id"] for x in graph["nodes"] if x.get("kind") == "symbol" and under_collapsed(x["id"])), None)
        if symbol:
            message("reveal", {"op": "reveal", "nodeIds": [symbol], "timings": True})
        message("columns", {"op": "columns", "timings": True})
        message("patch-small", {"op": "patch", "patch": small, "timings": True})
        message("patch-large", {"op": "patch", "patch": large, "timings": True})
        message("memory", {"op": "memory"})
        if not args.no_overview:
            o = message("overview-first", None, raw=json.dumps(
                {"op": "overview", "threads": args.threads, "workBudget": args.overview_budget,
                 "timings": True})[:-1] + ',"graph":' + graph_text + "}")
            if o.get("ok"):
                ov = o["overview"]
                places = {i: {"x": ov["x"][k], "y": ov["y"][k], "r": ov["r"][k]} for k, i in enumerate(ov["ids"])}
                del o, ov
                message("overview-byid", {"op": "overview", "threads": args.threads,
                                          "workBudget": args.overview_budget, "start": {"nodes": places},
                                          "timings": True})
        message("import-timed", None, raw=json.dumps(
            {"op": "import", "limit": 10000, "options": FIRST_VIEW, "timings": True})[:-1] + ',"graph":' + graph_text
            + "}")
        write({"mode": "profile" if profile_dir else "session", "size": n, "rep": rep, "message": "repDone",
               "graphSha256": sha, "target": target, "revealed": symbol,
               "smallPatch": [len(small["nodes"]), len(small["edges"])],
               "largePatch": [len(large["nodes"]), len(large["edges"])]})
        return True
    except TimeoutError:
        return False
    finally:
        s.close()


def oneshot_rep(binary, n, graph_text, graph, rep, args, write):
    WORK.mkdir(parents=True, exist_ok=True)
    small, _ = plan(graph)
    linux = platform.system() == "Linux"

    def run(name, request_text):
        req, resp = WORK / f"{n}-{name}.req.json", WORK / f"{n}-{name}.resp.json"
        req.write_text(request_text)
        load = [round(x, 2) for x in os.getloadavg()]
        started = time.perf_counter()
        status = None
        with open(req, "rb") as stdin, open(resp, "wb") as stdout:
            try:
                done = subprocess.run(["/usr/bin/time", "-v" if linux else "-l", str(binary)], stdin=stdin,
                                      stdout=stdout, stderr=subprocess.PIPE, timeout=args.timeout)
                status = done.returncode
                err = done.stderr.decode(errors="replace")
            except subprocess.TimeoutExpired:
                status, err = "timeout", ""
        seconds = time.perf_counter() - started
        m = re.search(r"Maximum resident set size \(kbytes\): (\d+)" if linux else r"(\d+)\s+maximum resident set size",
                      err)
        peak = (int(m.group(1)) * (1024 if linux else 1)) if m else None
        row = {"mode": "oneshot", "size": n, "rep": rep, "message": name, "seconds": round(seconds, 3),
               "status": status, "requestBytes": req.stat().st_size, "answerBytes": resp.stat().st_size,
               "peakResident": peak, "load": load}
        req.unlink()
        answer = None
        if status == 0:
            answer = json.loads(resp.read_text())
            row["ok"] = answer.get("ok")
            if not answer.get("ok"):
                row["error"] = str(answer.get("error"))[:300]
        resp.unlink(missing_ok=True)
        write(row)
        if status == "timeout":
            raise TimeoutError(name)
        return answer

    try:
        first = run("import", json.dumps({"op": "import", "limit": 10000, "options": FIRST_VIEW})[:-1]
                    + ',"graph":' + graph_text + "}")
        if not first or not first.get("ok"):
            return False
        snap = first["snapshot"]
        del first
        visible = {x["id"] for x in snap["graph"]["nodes"]}
        target = next((c for c in snap.get("collapsedIds", []) if c in visible), None)
        edit = {"expectedRevision": snap["revision"], "expectedLayoutVersion": snap["layoutVersion"]}
        snap_text = json.dumps(snap)
        del snap
        if target:
            run("toggle", json.dumps({"op": "toggle", "nodeId": target, **edit})[:-1] + ',"snapshot":' + snap_text + "}")
        run("patch-small", json.dumps({"op": "patch", "patch": small, **edit})[:-1] + ',"snapshot":' + snap_text + "}")
        write({"mode": "oneshot", "size": n, "rep": rep, "message": "repDone"})
        return True
    except TimeoutError:
        return False


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--sizes", type=int, nargs="+", default=[100000, 300000, 1000000])
    p.add_argument("--seed", type=int, default=1)
    p.add_argument("--reps", type=int, default=3)
    p.add_argument("--mode", nargs="+", default=["session", "oneshot"], choices=["session", "oneshot"])
    p.add_argument("--profile", action="store_true", help="one more session repetition under `sample`")
    p.add_argument("--profile-only", action="store_true", help="only the profiled repetition")
    p.add_argument("--threads", type=int, default=4)
    p.add_argument("--overview-budget", type=int, default=1_000_000_000)
    p.add_argument("--no-overview", action="store_true")
    p.add_argument("--timeout", type=int, default=900, help="seconds per message or request")
    p.add_argument("--max-load", type=float, default=4.0)
    p.add_argument("--max-wait", type=int, default=1200, help="seconds to wait for the load before a repetition")
    p.add_argument("--binary", type=Path, default=ROOT / "target/release/layout-command")
    p.add_argument("--out", type=Path, required=True)
    a = p.parse_args()
    if not a.binary.exists():
        sys.exit(f"Build first: cargo build --release -p arqlet-runtime --bin layout-command --locked ({a.binary})")
    done = set()
    if a.out.exists():
        for line in a.out.read_text().splitlines():
            r = json.loads(line)
            if r.get("message") == "repDone":
                done.add((r["mode"], r["size"], r["rep"]))
    lock = threading.Lock()

    def write(row):
        with lock, open(a.out, "a") as f:
            f.write(json.dumps(row) + "\n")
        print(json.dumps({k: v for k, v in row.items() if k not in ("profile", "estimate")})[:400], flush=True)

    write({"machine": machine(), "binary": str(a.binary.relative_to(ROOT)) if a.binary.is_relative_to(ROOT)
           else str(a.binary), "firstView": FIRST_VIEW, "seed": a.seed, "threads": a.threads,
           "overviewBudget": a.overview_budget, "argv": sys.argv[1:]})
    for n in a.sizes:
        path, text, sha = dataset(n, a.seed)
        graph = json.loads(text)
        write({"dataset": str(path.relative_to(ROOT)), "size": n, "sha256": sha, "bytes": len(text),
               "nodes": len(graph["nodes"]), "edges": len(graph["edges"]),
               "unresolved": sum(1 for e in graph["edges"] if e.get("target") is None),
               "containers": len({x.get("parentId") for x in graph["nodes"] if x.get("parentId")})})
        jobs = []
        if not a.profile_only:
            for rep in range(1, a.reps + 1):
                jobs += [(m, rep) for m in a.mode]
        if a.profile or a.profile_only:
            jobs.append(("profile", 0))
        for mode, rep in jobs:
            if (mode, n, rep) in done:
                continue
            for attempt in (1, 2):
                write({"mode": mode, "size": n, "rep": rep, "message": "repStart", "attempt": attempt,
                       **wait_for_load(a.max_load, a.max_wait)})
                if mode == "oneshot":
                    ok = oneshot_rep(a.binary, n, text, graph, rep, a, write)
                else:
                    pdir = None
                    if mode == "profile":
                        pdir = WORK / "profiles"
                        pdir.mkdir(parents=True, exist_ok=True)
                    ok = session_rep(a.binary, n, text, graph, sha, rep, a, write, pdir, medians(a.out))
                if ok:
                    break
    print(f"Wrote {a.out}")


if __name__ == "__main__":
    main()
