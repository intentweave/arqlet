"""Tables for benchmarks/results/bus-patch-20261127.md (#212) from the JSON lines of bus-patch.mjs.

Usage: python3 benchmarks/experiments/bus_patch_table.py phases FILE.jsonl
       python3 benchmarks/experiments/bus_patch_table.py variants FILE.jsonl
"""
import json
import sys


def rows(path):
    with open(path) as f:
        return [json.loads(line) for line in f if line.strip()]


def phases(path):
    # The probe's lines are `path ms count`; the top level of a patch is `dispatch/patch.compute`, of an import `dispatch`.
    print("| case | request | total ms | first layout | of it placement / separation / routing | second pass | of it overlaps | bus drawing | rest | conflicts | same as fresh |")
    print("|---|---|---|---|---|---|---|---|---|---|---|")
    for r in rows(path):
        p = {}
        for line in r.get("phases", []):
            name, ms, _ = line.rsplit(" ", 2)
            p[name.replace("dispatch/patch.compute/", "").replace("dispatch/", "")] = float(ms)
        total = r["timingsMs"]["total"]
        first = p.get("firstLayout", 0)
        second = p.get("secondPass.reroute", 0) + p.get("secondPass.finish", 0)
        overlaps = p.get("secondPass.reroute/R.overlaps", 0)
        bus = p.get("bus.collapse", 0) + p.get("bus.barsToLines", 0)
        t = r["timingsMs"]
        rest = total - first - second - bus
        print(
            f"| {r['case']} | {r['request']} | {total:.0f} | {first:.0f} | {t['placement']:.0f} / {t['separation']:.0f} / {t['routing']:.0f} "
            f"| {second:.0f} | {overlaps:.0f} | {bus:.0f} | {rest:.0f} | {r['routeConflicts']} | {r.get('sameAsFresh', '')} |"
        )


def variants(path):
    keys = ["siblingOverlaps", "lineNodeHits", "sharedLinePairs", "crossings", "bends", "length"]
    print("| case | variant | request | ms | conflicts | " + " | ".join(keys) + " | width x height | units kept |")
    print("|---|---|---|---|---|" + "---|" * len(keys) + "---|---|")
    for r in rows(path):
        if r["request"] == "import":
            continue
        m = r.get("metrics", {})
        print(
            f"| {r['case']} | {r['label']} | {r['request']} | {r['timingsMs']['total']:.0f} | {r['routeConflicts']} | "
            + " | ".join(f"{m.get(k, 0):.0f}" for k in keys)
            + f" | {r['width']} x {r['height']} | "
            + (f"{r['unitsKept']}/{r['unitsKept'] + r['unitsMoved']}" if r["request"] == "patch" else "")
            + " |"
        )


if __name__ == "__main__":
    {"phases": phases, "variants": variants}[sys.argv[1]](sys.argv[2])
