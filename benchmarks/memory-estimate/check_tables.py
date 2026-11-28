#!/usr/bin/env python3
"""Checks that the coefficient tables in crates/runtime/src/estimate.rs are those `fit.py report` prints (its output
file as the argument): the class rows' numbers, in order, for both targets."""
import re, sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
number_rows = lambda text: [re.findall(r"-?\d+\.\d+", m) for m in re.findall(r'\(\s*"[a-z]+/[a-z]+",\s*class\(([^)]*)\)', text)]
printed = number_rows(Path(sys.argv[1]).read_text())
engine = number_rows((ROOT / "crates/runtime/src/estimate.rs").read_text())
others = lambda text: (re.findall(r"layout: \[([-\d., ]*)\]", text), re.findall(r"overview_(?:nodes|edges): ([\d.]+)", text))
norm = lambda pair: ([re.sub(r"\s+", " ", x).strip() for x in pair[0]], pair[1])
if printed != engine or norm(others(Path(sys.argv[1]).read_text())) != norm(others((ROOT / "crates/runtime/src/estimate.rs").read_text())):
    sys.exit(f"the tables differ: {len(printed)} printed rows, {len(engine)} in the engine (or the layout and overview terms)")
print(f"the engine's {len(engine)} class rows, layout model and overview terms are those fit.py prints")
