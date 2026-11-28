#!/usr/bin/env python3
"""Tables of `throughput.py` and `throughput-wasm.mjs` results: median (min–max) per mode, size and message, with the
load before each row, the layout phases (`timings`, medians), memory, and the profile categories.

    python3 benchmarks/scaling/throughput_summary.py benchmarks/results/throughput-1m-DATE.jsonl [...-wasm.jsonl]
"""
import json, sys
from statistics import median

ORDER = ["import", "toggle-open", "toggle-close", "reveal", "columns", "patch-small", "patch-large", "memory",
         "overview-first", "overview-byid", "import-timed", "toggle"]
PHASES = ["layout", "placement", "organicStress", "separation", "repair", "portResolution", "routing", "plan",
          "channelGrowth", "construction", "fallbackRouting", "finalization", "buses"]


def fmt_s(xs):
    xs = sorted(xs)
    if not xs:
        return "–"
    m = median(xs)
    return f"{m:.2f} s" + (f" ({xs[0]:.2f}–{xs[-1]:.2f})" if len(xs) > 1 else "")


def mib(b):
    return f"{b / 2**20:,.0f} MiB" if b else "–"


def main():
    # Only the rows of attempts that completed (`repStart` … `repDone` of the same mode, size and repetition): a failed
    # or interrupted attempt is left out (its rows stay in the file).
    rows, pending = [], {}
    for path in sys.argv[1:]:
        for line in open(path):
            if not line.strip():
                continue
            r = json.loads(line)
            key = (r.get("mode"), r.get("size"), r.get("rep"))
            if r.get("message") == "repStart":
                pending[key] = []
            elif r.get("message") == "repDone":
                rows += pending.pop(key, [])
            elif key in pending:
                pending[key].append(r)
    groups = {}
    for r in rows:
        if "seconds" in r and r.get("message") in ORDER and r.get("ok", r.get("status") == 0):
            groups.setdefault((r["mode"], r["size"], r["message"]), []).append(r)
    print("| mode | nodes | message | wall, median (min–max) | n | load (1 min) before | layout (timings) | resident after / peak | answer |")
    print("|---|---|---|---|---|---|---|---|---|")
    for key in sorted(groups, key=lambda k: (k[0], k[1], ORDER.index(k[2]))):
        g = groups[key]
        loads = [r["load"][0] for r in g if r.get("load")]
        lay = [r["timings"]["layout"] / 1000 for r in g if r.get("timings")]
        tot = [r["timings"]["total"] / 1000 for r in g if r.get("timings")]
        res = median([r.get("residentBytes") or r.get("linearMemoryBytes") or 0 for r in g])
        peak = max([r.get("peakResidentPolled") or r.get("peakResident") or r.get("linearMemoryBytes") or 0 for r in g])
        lay_s = f"{median(lay):.2f} s of {median(tot):.2f} s" if lay else "–"
        print(f"| {key[0]} | {key[1]:,} | {key[2]} | {fmt_s([r['seconds'] for r in g])} | {len(g)} | "
              f"{min(loads):.1f}–{max(loads):.1f} | {lay_s} | {mib(res)} / {mib(peak)} | "
              f"{median([r.get('answerBytes', 0) for r in g]) / 1e6:.1f} MB |")
    print()
    print("Layout phases (ms, medians of the timed rows):")
    print()
    print("| mode | nodes | message | total | " + " | ".join(PHASES) + " |")
    print("|---|---|---|---|" + "---|" * len(PHASES))
    for key in sorted(groups, key=lambda k: (k[0], k[1], ORDER.index(k[2]))):
        t = [r["timings"] for r in groups[key] if r.get("timings")]
        if not t or median([x["layout"] for x in t]) == 0:
            continue
        print(f"| {key[0]} | {key[1]:,} | {key[2]} | {median([x['total'] for x in t]):,.0f} | "
              + " | ".join(f"{median([x[p] for x in t]):,.0f}" for p in PHASES) + " |")
    print()
    print("Profiles (`sample`, 1 ms; share of busy samples by category, outermost-priority rule of throughput.py):")
    print()
    for r in rows:
        if r.get("mode") == "profile" and r.get("profile") and r["profile"]["samples"] > 0:
            p = r["profile"]
            cats = ", ".join(f"{k} {100 * v / p['samples']:.0f} %" for k, v in p["categories"].items()
                             if 100 * v / p["samples"] >= 1)
            print(f"- {r['size']:,} {r['message']} ({r['seconds']:.2f} s, load {r['load'][0]:.1f}, {p['samples']} samples;"
                  f" memcmp {100 * p['memcmp'] / p['samples']:.0f} %, allocator {100 * p['allocator'] / p['samples']:.0f} %):"
                  f" {cats}")
            top = ", ".join(f"`{s.split('::')[-1] if '::' in s else s}` {100 * c / p['samples']:.0f} %"
                            for s, c in p["engineTop"][:6])
            print(f"  - top engine frames (own samples incl. memcmp/allocator they call): {top}")


if __name__ == "__main__":
    main()
