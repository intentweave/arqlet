# Curated reference corpus

Curated references (see `docs/curated-references.md`, section 9) stored as evaluation inputs.

- `manifest.json`: the index. Each case has its file, sha256, input and payload hashes, expected validity, tags and,
  where a repair is evaluated, the recorded edits.
- `<case>/reference.lwref.json`: the reference document in canonical text, so its bytes are reproducible.
- `build.py`: creates the cases from the fixtures in `examples/fixtures/reference/` through `layout-command`.
- `evaluate.py`: runs each case in the modes validation, constrained, unconstrained and repair; writes
  `results/evaluation-<stamp>.json` with the engine binary hash, the git revision and the metric deltas. Modes are
  reported separately. The results hold quality metrics only; they contain no timings.

Checks: `cargo test -p arqlet-runtime --test reference_corpus` (part of `cargo test --workspace`) verifies the
hashes, the expected validity, the constrained pins and the repairs without a browser.

Regenerate after an intended engine change: `cargo build -p arqlet-runtime --bins`, then
`python3 benchmarks/references/build.py` and `python3 benchmarks/references/evaluate.py`.

Cases: `ports-fanout`, `cycle-selfloop` and `container-boundary` are unedited references (valid). The pinned and repair
cases use `container-boundary`. `state-tcp-moved` is the user's draft, kept as a negative example: it is invalid (five
hard violations, five pending routes), and the corpus expects it to stay invalid.

The corpus has six cases. It is not a statistical sample, and it is not evidence about performance or scaling.
Status is `draft` for every case: no reference has been accepted by a human review yet.
