#!/usr/bin/env python3
"""Fits the coefficients of the memory estimate (crates/runtime/src/estimate.rs, ADR 0024 C3) to measured peaks and
reports the estimate's error. Standard library only; deterministic (seeded search).

    python3 benchmarks/memory-estimate/fit.py report NATIVE.jsonl WASM.jsonl [--holdout g1,g2]   # tables + Rust tables
    python3 benchmarks/memory-estimate/fit.py classes ROWS.jsonl [NATIVE.jsonl]                  # one target
    python3 benchmarks/memory-estimate/fit.py check ROWS.jsonl [--md]   # the engine's own estimates in the rows

The rows come from sweep.py (request-peak natively, wasm-peak.mjs in WASM). The layout model is fitted on the native
rows' from-scratch layout peaks (`layoutPeak`); each class (kind of operation and place) then gets coefficients that make
estimate / measured as even as possible over its rows above 4 MB (`minmax`), with the fixed part raised until every row
is covered. `predict` is the model as the engine computes it, margin included.
"""
import json, sys
from collections import defaultdict


def rows(path):
    out = []
    for line in open(path):
        r = json.loads(line)
        if "failed" in r or "error" in r or "skipped" in r:
            continue
        r["measured"] = r.get("peak", r.get("linear"))
        if r["measured"] is None:
            continue
        out.append(r)
    return out


def solve(a, b):
    """Gaussian elimination for a small dense system."""
    n = len(a)
    m = [row[:] + [b[i]] for i, row in enumerate(a)]
    for c in range(n):
        p = max(range(c, n), key=lambda r: abs(m[r][c]))
        if abs(m[p][c]) < 1e-30:
            return None
        m[c], m[p] = m[p], m[c]
        for r in range(n):
            if r != c:
                f = m[r][c] / m[c][c]
                for k in range(c, n + 1):
                    m[r][k] -= f * m[c][k]
    return [m[i][n] / m[i][i] for i in range(n)]


def nnls_relative(xs, ys, names):
    active = list(range(len(names)))
    while True:
        a = [[0.0] * len(active) for _ in active]
        b = [0.0] * len(active)
        for x, y in zip(xs, ys):
            w = 1.0 / max(y, 1.0) ** 2
            for i, fi in enumerate(active):
                b[i] += w * x[fi] * y
                for j, fj in enumerate(active):
                    a[i][j] += w * x[fi] * x[fj]
        sol = solve(a, b)
        if sol is None:
            return {}
        neg = [active[i] for i, v in enumerate(sol) if v < 0]
        if not neg:
            return {names[f]: v for f, v in zip(active, sol)}
        worst = min(range(len(sol)), key=lambda i: sol[i])
        active.pop(worst)


def view(r):
    return r.get("viewAfter") or {}


def layout_features(v):
    e, n, o = v.get("edges", 0), v.get("nodes", 0), v.get("open", 0)
    return [1.0, n, e, o, e * e, e * e / max(n, 1), n * o, o * o]


LAYOUT = ["fixed", "node", "edge", "open", "square", "density", "nesting", "open2"]


def fit(path):
    data = rows(path)
    # The layout of a view from scratch (native rows only carry it).
    lay = [(layout_features(view(r)), r["layoutPeak"]) for r in data if r.get("layoutPeak")]
    if lay:
        coef = nnls_relative([x for x, _ in lay], [y for _, y in lay], LAYOUT)
        est = lambda x: sum(coef.get(n, 0) * v for n, v in zip(LAYOUT, x))
        scale = max(y / est(x) for x, y in lay)
        print("layout", {k: round(v * scale, 3) for k, v in coef.items()}, "scale", round(scale, 3))
        ratios = sorted(est(x) * scale / y for x, y in lay)
        print("  layout est/peak: min %.2f median %.2f max %.2f" % (ratios[0], ratios[len(ratios) // 2], ratios[-1]))


def check(path, md=False):
    data = [r for r in rows(path) if isinstance(r.get("estimate"), dict) and r["estimate"].get("bytes")]
    groups = defaultdict(list)
    for r in data:
        op = r["op"] if r["op"] != "refine4" else "refine (4 threads)"
        groups[(r["mode"], op)].append(r)
    below = []
    lines = []
    if md:
        lines.append("| Way | Operation | Rows | estimate / peak: min | median | max | estimate time: max |")
        lines.append("|---|---|---|---|---|---|---|")
    for (mode, op), rs in sorted(groups.items()):
        ratios = sorted(r["estimate"]["bytes"] / measured_for_fit(r, 65536) for r in rs)
        ms = max(r["estimate"].get("ms", 0) for r in rs)
        for r in rs:
            if r["estimate"]["bytes"] < measured_for_fit(r, 65536):
                below.append(r)
        if md:
            lines.append(f"| {mode} | {op} | {len(rs)} | {ratios[0]:.2f} | {ratios[len(ratios) // 2]:.2f} | "
                         f"{ratios[-1]:.2f} | {ms:.1f} ms |")
        else:
            lines.append(f"{mode:9} {op:20} n={len(rs):3} min {ratios[0]:.2f} median {ratios[len(ratios) // 2]:.2f} "
                         f"max {ratios[-1]:.2f}  estimate ms max {ms:.1f}")
    print("\n".join(lines))
    print(f"\nrows below their estimate's bound: {len(below)} of {len(data)}")
    for r in below:
        print(f"  {r['graph']} {r['limit']} {r['mode']} {r['op']}: estimate {r['estimate']['bytes'] / 1e6:.2f} MB, "
              f"measured {measured_for_fit(r, 65536) / 1e6:.2f} MB")



# --- Class coefficients (text, source, layout share, base) per kind of operation and place. ---

KINDS = {"import": "import", "overview": "overview", "toggle": "open", "reveal": "open", "filter": "open",
         "configure": "open", "restore": "open", "direction": "keep", "patch": "patch", "add": "patch",
         "refine": "refine", "refine4": "refine"}


def class_of(r):
    kind = KINDS[r["op"]]
    return "overview/dispatch" if kind == "overview" else f"{kind}/{r['mode']}"


# The source coefficients of the placeholder estimate (model 1 before calibration), to read the source graph's heap back
# from rows of the first runs, which carry only the parts.
PLACEHOLDER_SOURCE = {"import/dispatch": 4.0, "import/session": 4.0, "overview/dispatch": 4.0}


def sizes_of(r):
    e = r["estimate"]
    if "source" not in e:
        cls = class_of(r)
        structure = e["parts"]["structure"]
        if cls == "overview/dispatch":
            structure -= 1024.0 * r["nodes"] + 512.0 * r["edges"]
        e = dict(e, source={"heap": structure / PLACEHOLDER_SOURCE.get(cls, 2.0), "nodes": r["nodes"],
                            "edges": r["edges"]})
        a, b = r.get("viewAfter") or {}, r.get("viewBefore") or {}
        e["view"] = {k: max(a.get(k, 0), b.get(k, 0)) for k in ("nodes", "edges", "open")}
        e["jobs"] = 4 if r["op"] == "refine4" else 1
    v = e.get("view") or {}
    return {"text": e["parts"]["request"], "source": e["source"]["heap"], "nodes": e["source"]["nodes"],
            "edges": e["source"]["edges"], "vn": v.get("nodes", 0), "ve": v.get("edges", 0), "open": v.get("open", 0),
            "jobs": e.get("jobs", 1)}


def layout_of(s, lay):
    v = {"nodes": s["vn"], "edges": s["ve"], "open": s["open"]}
    return sum(lay[n] * x for n, x in zip(LAYOUT, layout_features(v)))


def measured_for_fit(r, floor):
    """What the estimate bounds: the counted heap peak natively; in WASM the linear memory growth of a fresh worker, or
    the counted peak where it is larger, and for a session's edits (whose growth a load before hides) the counted peak."""
    if r.get("pointer") == 32:
        if r["mode"] == "session" and r["op"] != "import":
            return max(r.get("heapPeak") or 0, floor)
        return max(r.get("linear") or 0, r.get("heapPeak") or 0, floor)
    return max(r["measured"], floor)


def fit_classes(path, lay, big=4 << 20, exclude=()):
    """Per class: coefficients from the rows above `big` bytes (`minmax`), then the fixed part raised until no row of
    the class, small ones included, is above its estimate."""
    data = [r for r in rows(path) if isinstance(r.get("estimate"), dict) and r["estimate"].get("parts")
            and r["graph"] not in exclude]
    if any(r.get("pointer") == 32 for r in data):
        data = [r for r in data if not (r["mode"] == "session" and r["op"] != "import" and r.get("heapPeak") is None)]
    out = {}
    for cls in sorted({class_of(r) for r in data}):
        rs = [r for r in data if class_of(r) == cls]
        session = cls.endswith("/session") and not cls.startswith("import")
        sz = list(map(sizes_of, rs))
        if cls == "overview/dispatch":
            names = ["text", "source", "nodes", "edges", "base"]
            feats = lambda s: [s["text"], s["source"], s["nodes"], s["edges"], 1.0]
        elif session:
            names = ["source", "layout", "base"]
            feats = lambda s: [s["source"], s["jobs"] * layout_of(s, lay), 1.0]
        else:
            names = ["text", "source", "layout", "base"]
            feats = lambda s: [s["text"], s["source"], s["jobs"] * layout_of(s, lay), 1.0]
        # The request text counts once by construction (the estimate's `request` part); the rest is fitted.
        rest = [measured_for_fit(r, 1) - s["text"] for r, s in zip(rs, sz)]
        pick = [i for i, r in enumerate(rs) if measured_for_fit(r, 1) > big] or list(range(len(rs)))
        c = minmax([feats(sz[i]) for i in pick], [max(rest[i], 1.0) for i in pick])
        coef = dict(zip(names, c))
        est = lambda s: sum(coef[n] * x for n, x in zip(names, feats(s)))
        coef["base"] += max(0.0, max(y - est(s) for y, s in zip(rest, sz)))
        coef["text"] = 1.0 + coef.get("text", 0.0)
        total = lambda s: s["text"] + est(s)
        ratios = sorted(total(s) / measured_for_fit(r, 1) for s, r in zip(sz, rs))
        bigr = sorted(total(sz[i]) / measured_for_fit(rs[i], 1) for i in pick)
        out[cls] = coef
        print(f"{cls:18} n={len(rs):3} " + " ".join(f"{k}={v:.4g}" for k, v in coef.items()) +
              f"  above {big >> 20} MB n={len(pick)}: max {bigr[-1]:.2f} median {bigr[len(bigr) // 2]:.2f}"
              f" | all: min {ratios[0]:.2f}")
    return out


def minmax(xs, ys, seed=1, starts=30, iters=3000):
    """Non-negative coefficients that make sum(c * x) / y as even as possible over the rows (a seeded random search in
    log space minimising log(max ratio / min ratio) plus half the mean log ratio), scaled so that the lowest ratio is 1:
    an upper bound on every row."""
    import math, random
    k = len(xs[0])
    def spread(c):
        ratios = [sum(ci * xi for ci, xi in zip(c, x)) / y for x, y in zip(xs, ys)]
        low = min(ratios)
        if low <= 0:
            return 1e9, 0
        # The worst overestimate, and half the typical one: a fixed part alone would even the ratios out at a price.
        typical = sum(math.log(r / low) for r in ratios) / len(ratios)
        return math.log(max(ratios) / low) + 0.5 * typical, low
    rng = random.Random(seed)
    best = None
    for _ in range(starts):
        c = [10 ** rng.uniform(-1, 6) if rng.random() < 0.6 else 0.0 for _ in range(k)]
        c[-1] = max(c[-1], 1.0)
        s, step = spread(c)[0], 1.0
        for it in range(iters):
            i = rng.randrange(k)
            d = list(c)
            if d[i] == 0 and rng.random() < 0.5:
                d[i] = 10 ** rng.uniform(-1, 6)
            elif rng.random() < 0.1:
                d[i] = 0.0
            else:
                d[i] = max(d[i], 1e-3) * math.exp(rng.gauss(0, step))
            sd = spread(d)[0]
            if sd < s:
                c, s = d, sd
            if it % (iters // 6) == iters // 6 - 1:
                step *= 0.7
        if best is None or s < best[0]:
            best = (s, c)
    c = best[1]
    low = spread(c)[1]
    return [ci / low for ci in c]


def fit_layout(path, big=8 << 20, exclude=()):
    """The layout of a view from scratch (native rows carry its counted peak, `layoutPeak`), from the views above `big`
    bytes (`minmax`)."""
    lay = [(layout_features(view(r)), r["layoutPeak"]) for r in rows(path)
           if r.get("layoutPeak", 0) > big and r["graph"] not in exclude]
    # `minmax` keeps its last coefficient positive: the fixed part goes last.
    c = minmax([x[1:] + x[:1] for x, _ in lay], [y for _, y in lay])
    coef = dict(zip(LAYOUT[1:] + LAYOUT[:1], c))
    ratios = sorted(sum(coef[n] * xi for n, xi in zip(LAYOUT, x)) / y for x, y in lay)
    print(f"layout from scratch, views above {big >> 20} MB: n={len(ratios)} est/peak min {ratios[0]:.2f} "
          f"median {ratios[len(ratios) // 2]:.2f} max {ratios[-1]:.2f}")
    return coef


def rust_table(classes, lay, overview):
    order = ["import/dispatch", "import/session", "open/dispatch", "open/session", "keep/dispatch", "keep/session",
             "patch/dispatch", "patch/session", "refine/dispatch", "refine/session", "overview/dispatch"]
    out = []
    for k in order:
        c = classes.get(k)
        if c is None:
            raise SystemExit(f"no rows for {k}")
        layout = 0.0 if k.startswith("overview") else c.get("layout", 0.0)
        out.append(f'        ("{k}", class({c.get("text", 1.0):.4f}, {c.get("source", 0.0):.4f}, {layout:.4f}, '
                   f'{c.get("base", 0.0):.1f})),')
    ov = classes["overview/dispatch"]
    return "\n".join(out), "[" + ", ".join(f"{lay[n]:.1f}" for n in LAYOUT) + "]", \
        (ov.get("nodes", 0.0), ov.get("edges", 0.0))


def conservative(classes):
    """A refinement holds a layout per job: its layout share is never below that of an edit that keeps the view (the
    calibration ran with a small work budget, where a refinement lays out little; it refuses large views anyway)."""
    for place in ("dispatch", "session"):
        keep = classes.get(f"keep/{place}", {}).get("layout", 1.0)
        refine = classes.get(f"refine/{place}")
        if refine is not None:
            refine["layout"] = max(refine.get("layout", 0.0), keep)
    return classes


MARGIN = 1.15  # crates/runtime/src/estimate.rs, MARGIN


def predict(r, classes, lay, margin=MARGIN):
    """The estimate model 1 gives for a row, as crates/runtime/src/estimate.rs computes it."""
    c = classes[class_of(r)]
    s = sizes_of(r)
    rest = (c.get("text", 1.0) - 1.0) * s["text"] + c.get("base", 0.0) + c.get("source", 0.0) * s["source"]
    if class_of(r) == "overview/dispatch":
        rest += c.get("nodes", 0.0) * s["nodes"] + c.get("edges", 0.0) * s["edges"]
    else:
        rest += c.get("layout", 0.0) * s["jobs"] * layout_of(s, lay)
    return s["text"] + margin * rest


def report(path, lay, classes, title, big=4 << 20, only=None):
    data = [r for r in rows(path) if isinstance(r.get("estimate"), dict) and r["estimate"].get("parts")
            and (only is None or r["graph"] in only)]
    if any(r.get("pointer") == 32 for r in data):
        data = [r for r in data if not (r["mode"] == "session" and r["op"] != "import" and r.get("heapPeak") is None)]
    groups = defaultdict(list)
    for r in data:
        op = "refine (4 threads)" if r["op"] == "refine4" else r["op"]
        groups[(r["mode"], op)].append(r)
    print(f"\n{title}\n")
    print("| Way | Operation | Rows | above 4 MB | estimate / measured above 4 MB: min | median | max | all rows: min |"
          " largest measured |")
    print("|---|---|---|---|---|---|---|---|---|")
    below = []
    for (mode, op), rs in sorted(groups.items()):
        ratio = [(predict(r, classes, lay) / measured_for_fit(r, 1), r) for r in rs]
        large = sorted(x for x, r in ratio if measured_for_fit(r, 1) > big)
        every = sorted(x for x, _ in ratio)
        below += [r for x, r in ratio if x < 1]
        top = max(measured_for_fit(r, 1) for r in rs) / 1e6
        if large:
            print(f"| {mode} | {op} | {len(rs)} | {len(large)} | {large[0]:.2f} | {large[len(large) // 2]:.2f} | "
                  f"{large[-1]:.2f} | {every[0]:.2f} | {top:.0f} MB |")
        else:
            print(f"| {mode} | {op} | {len(rs)} | 0 | | | | {every[0]:.2f} | {top:.0f} MB |")
    for r in below:
        print(f"  below: {r['graph']} {r['limit']} {r['mode']} {r['op']}: {predict(r, classes, lay) / 1e6:.1f} MB "
              f"< {measured_for_fit(r, 1) / 1e6:.1f} MB")


if __name__ == "__main__":
    if sys.argv[1] == "report":
        # fit.py report NATIVE.jsonl WASM.jsonl [--holdout g1,g2,...]: fits both targets and prints the error tables,
        # the Rust tables and, with --holdout, the error on graphs left out of the fit.
        native, wasm = sys.argv[2], sys.argv[3]
        hold = sys.argv[sys.argv.index("--holdout") + 1].split(",") if "--holdout" in sys.argv else []
        lay = fit_layout(native)
        lay_held = fit_layout(native, exclude=hold) if hold else None
        for name, path in (("native64", native), ("wasm32", wasm)):
            classes = conservative(fit_classes(path, lay))
            report(path, lay, classes, f"{name}: calibrated on every row")
            table, layout, _ = rust_table(classes, lay, None)
            print(f"\n// {name}\n{table}\n    layout: {layout},")
            if hold:
                held = conservative(fit_classes(path, lay_held, exclude=hold))
                report(path, lay_held, held, f"{name}: fitted without {', '.join(hold)}, measured on them", only=hold)
        sys.exit(0)
    cmd, path = sys.argv[1], sys.argv[2]
    if cmd == "fit":
        fit(path)
    elif cmd == "classes":
        # fit.py classes ROWS.jsonl [LAYOUT_ROWS.jsonl]: the layout model from native rows, then the classes.
        lay = fit_layout(sys.argv[3] if len(sys.argv) > 3 else path)
        print("layout", {k: round(v, 3) for k, v in lay.items()})
        classes = fit_classes(path, lay)
        table, layout, overview = rust_table(classes, lay, None)
        print(table)
        print("layout:", layout, "overview nodes/edges:", overview)
    else:
        check(path, "--md" in sys.argv)
