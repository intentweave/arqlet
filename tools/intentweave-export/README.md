# intentweave-graph-export

Standalone, unpublished Node 24+ package inside Arqlet. MIT OR Apache-2.0.
No installation or external dependencies are needed. Sources use `.ts` entrypoints
supported by Node's native TypeScript loading; a TypeScript typecheck/build pipeline
is not yet provided.

```sh
npm test
npm run export -- --db /path/to/.iw/index.db --out /path/to/new-export --claims
npm run export -- --db /path/to/.iw/index.db --out /path/to/new-export --memberships [--doc-confidence 0.8]
```

## Contract v1

- `manifest.json`: source metadata, explicit unknown repository revision, counts,
  coverage limitations, SHA-256 checksums and sizes.
- `graph.json`: nodes, edges and diagnostics. Directory containers are derived.
  File/symbol/edge source records and IDs are retained. IDs are namespaced to avoid
  collisions; persistence across reindexing is not guaranteed.
- `claims.json` (optional): all rows from the available allowlisted claim, subject,
  evidence, assessment, review and supporting observation tables. Missing tables
  are listed. This is a raw relational payload, not a resolved visual overlay.
  Discovery candidates and portable YAML policies are not exported.

With `--memberships`, `graph.json` also carries `memberships` (Arqlet ADR 0022): groupings
a symbol belongs to besides its file, as dimensions of groups. Only what the index states:

- `doc` (Documented in): one group per document of the index, holding the symbols its
  `annotations` mention at or above `--doc-confidence` (default 0.8; identifier matches
  score 0.8–1.0, code spans, bold text and headings often 0.5). A symbol may be in several
  documents, and a document's symbols lie in many files: the cross-cutting case.
- `kind` (Symbol kind) and `visibility` (`symbols.export`): one group per value, so each is
  a partition.

Groups and members are sorted; a dimension whose table or column the index lacks is left
out and named in `manifest.json` (`memberships.missing`), together with the threshold and
the counts per dimension. Members are always nodes of the same graph, and the validator
refuses what Arqlet refuses on import (a dimension called `parent`, a duplicate dimension
or group). Without the flag, `graph.json` is unchanged. Doc memberships are annotation
matches, not verified references; see `limitations` in the manifest.

The index has no namespace column (`symbols` carries `kind`, `container`, `export`,
`implements` and `decorators`; in schema 23 the last two are rarely set), and file
membership is the containment itself. Claim candidates mostly bind one subject each, so
they make no useful grouping; claims stay a raw payload under `--claims`.

All core table rows are read without query limits in a single read transaction.
The SQLite source is opened read-only; no migration, discovery or indexing runs.
Schema 23 was inspected; other versions produce a diagnostic and must satisfy
required column checks. This adapter intentionally reads the storage schema:
public query APIs limit results and do not provide a complete graph export.

Container symbol names resolve only when unique within their file. Ambiguous or
cyclic container hints produce diagnostics and retain file containment. Caller
names resolve only when unique in their file; otherwise calls originate at the
file with `sourceResolution: file_fallback`. Unresolved/external callees and import
targets have `target: null`; the original source record preserves their reference.
No guessed external symbol is manufactured. Cycles, loops and parallel edges stay.

Existing destinations are refused. Files are prepared in a sibling staging
folder; `manifest.json` is moved last and is the completion marker. Consumers
must require the manifest and verify checksums. Output is deterministic for an
unchanged source snapshot. The current checkout's HEAD is deliberately not used
as the index revision. Index freshness is unknown unless established separately.

## Limits

The export is in-memory and intended as a first fixture/export adapter, not yet a
streaming exporter. Output may include internal paths, code signatures, evidence
and documentation; generated data stays local and must be reviewed before sharing.
Claim links are preserved as source tables; semantic referential validation and
claim-to-scene resolution are future adapter work. Graph referential/containment
validation is implemented. No layout computation or direct Rust dependency.

## Provenance

Adapter authored for Arqlet using the Intentweave storage contract inspected
in local checkout `ced1639` (`packages/index/src/schema.ts`). The memberships read
`symbols.kind`, `symbols.export` and `annotations` as defined in that file at local checkout
`45ae94715ce0` (branch `feat/generalized-repository-claims`). No implementation code
was copied. Intentweave: https://github.com/intentweave/intentweave
