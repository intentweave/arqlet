# Calibrating the memory estimate

The operation `estimate` (ADR 0024 step C3, `crates/runtime/src/estimate.rs`) is a calibrated estimate, not a guarantee.
Its coefficients come from measured peaks, and this folder holds the tools that measure them and fit the coefficients.
The current calibration and its error are in `benchmarks/results/memory-estimate-20261127.md`.

**Rule: recalibrate whenever layout code changes.** The estimate's layout part stands for the peak of the layout code, so
any change to layering, ordering, placement, routing or their data structures can raise the peaks. The change does not
have to change the output; a layout change that keeps the fingerprints can still allocate differently. After such a
change:

1. Measure again (natively, then in WASM):

   ```sh
   python3 benchmarks/memory-estimate/datasets.py
   CARGO_TARGET_DIR=target cargo build --release --manifest-path benchmarks/session-memory/Cargo.toml --bin request-peak
   python3 benchmarks/memory-estimate/sweep.py native.jsonl
   cargo build -p arqlet-wasm --target wasm32-unknown-unknown --release --features query --locked
   python3 benchmarks/memory-estimate/sweep.py wasm.jsonl --wasm target/wasm32-unknown-unknown/release/arqlet_wasm.wasm
   ```

2. Fit and check:

   ```sh
   python3 benchmarks/memory-estimate/fit.py report native.jsonl wasm.jsonl   # tables and the Rust coefficient tables
   python3 benchmarks/memory-estimate/fit.py shapes native.jsonl wasm.jsonl   # leave one shape of graph out
   python3 benchmarks/memory-estimate/fit.py report native.jsonl wasm.jsonl --holdout g1,g2   # named graphs left out
   ```

3. Copy the tables (classes with their floors, the layout model, the overview's terms) into the two `COEFFICIENTS`
   blocks of `estimate.rs`, check them with `check_tables.py`, and raise `MODEL_VERSION`.

4. Write a new results file with the machine, revision, build and load, and the leave-one-shape-out check. The fit
   is deterministic (seeded), so the same rows give the same tables with the same Python (3.13.2 for the current
   tables).

Without a recalibration the estimate may fall below the peaks of the changed code.

## Files

- `datasets.py`: the generated graphs of the calibration: synthetic graphs, full layouts and code indexes.
- `sweep.py`: runs every configuration through `request-peak` (native, counted heap) or `wasm-peak.mjs` (WASM, linear
  memory and `heapPeakBytes`). Each run has a time limit and is retried once. Rows are appended as they finish, each
  with the machine's load.
- `wasm-peak.mjs`: one fresh WASM instance per request.
- `rss.py`: the resident memory of `layout-command` per request, next to the estimate. Natively the estimate stands for
  the counted heap, and the resident memory can be several times that.
- `check_tables.py FIT_OUTPUT`: checks that the tables in `estimate.rs` are what `fit.py report` printed.
- `fit.py`: the layout model, the coefficients per kind of operation, place and target, the structural floor, the margin
  (`MARGIN`, as in the engine), and the reports.
- `../session-memory/src/bin/request-peak.rs`: the native measurement, with a counting allocator.
