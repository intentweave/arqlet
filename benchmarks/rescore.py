#!/usr/bin/env python3
"""Re-evaluates stored snapshots with the current metric version. Writes a new file; never overwrites old numbers.

  python3 benchmarks/rescore.py [SNAPSHOT_DIR ...]   (default: benchmarks/results/profiles-*-snapshots)

Each output row names the snapshot, the metric version, and the metrics. Compare rows only with the same
metric version; a version change is a new measurement, not a regression.
"""
import datetime
import glob
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import profile_runner as pr  # noqa: E402

RESULTS = pr.RESULTS


def main():
    dirs = sys.argv[1:] or sorted(glob.glob(str(RESULTS / "profiles-*-snapshots")))
    stamp = datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    out = RESULTS / f"rescore-{stamp}.jsonl"
    rows = 0
    with out.open("w") as f:
        for directory in dirs:
            for path in sorted(Path(directory).glob("*.snapshot.json")):
                snapshot = json.loads(path.read_text())
                metrics, _, _ = pr.measure(snapshot, timeout=600)
                f.write(json.dumps({"snapshot": str(path.relative_to(pr.ROOT)), "metricsVersion": metrics["version"],
                                    "metrics": metrics}) + "\n")
                rows += 1
    print(f"{rows} snapshots rescored -> {out.relative_to(pr.ROOT)}")


if __name__ == "__main__":
    main()
