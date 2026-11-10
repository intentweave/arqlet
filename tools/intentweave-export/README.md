# intentweave-graph-export

Standalone, unpublished Node 24+ package inside Arqlet. MIT OR Apache-2.0.
No installation or external dependencies are needed. Sources use `.ts` entrypoints
supported by Node's native TypeScript loading; a TypeScript typecheck/build pipeline
is not yet provided.

```sh
npm test
npm run export -- --db /path/to/.iw/index.db --out /path/to/new-export --claims
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
in local checkout `ced1639` (`packages/index/src/schema.ts`). No implementation code
was copied. Intentweave: https://github.com/intentweave/intentweave
