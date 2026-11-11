# Fingerprints before release 0.2.0 (2026-11-11)

Question: do the cluster overview branches (PRs #4–#9) change the default output of the box pipeline?

Method: `bench` (release, `benchmark-timings`) built from `origin/main` (`e63b6a3`) and from the
branch (`de45c01` plus the uncommitted release preparation, which changes no Rust code), run on the
**same** dataset files, the 20 views of `fingerprints-20261108-single-pass-default.jsonl` at threads 1 and 4, one rep each.
`intentweave-export` was left out (no export in the checkout); `layerweave-pipeline` is `arqlet-pipeline` since the rename.
Fingerprints are deterministic, so the load of the machine (shared, not idle) does not matter; no timing is taken from this run.

Result:

- main against branch: all 36 rows identical (`refineFingerprint`, `geometryFingerprint`, `refineCoreFingerprint`,
  `inputGeometryFingerprint`).
- branch against the stored 2026-11-08 baseline: 28 rows identical; `layerweave-code` (6 rows) has a changed input (it
  is generated from the repository's own source, which grew); `arqlet-pipeline` (2 rows) differs from the stored
  `layerweave-pipeline` on main as well, so the difference predates these branches (the dataset was renamed).

The branch's rows: `fingerprints-20261111-overview-release.jsonl`.
