# Constructive layered routes, track-aware search (plan section 4a), 2026-10-30

Import only, `bench --constructive 1`, Apple M3, 8 logical CPUs, release, rustc 1.99.0, revision 0bcfd56 plus uncommitted
changes, load about 5, one run. Per dataset: `constructive-track-aware-20261030.jsonl`.

Changes against the step 2 record (`constructive-step2-20261030.md`):
- Segments are stored in absolute coordinates, so routes of left-to-right and top-to-bottom areas share one record.
- The path search rejects a step that comes within the track spacing of an assigned segment, instead of finding the
  cheapest path first and checking afterwards. The final check stays.
- The strict test of the first and last segments is kept (they are checked against every obstacle).

| variant | constructed | NoFreeTrack | PortSide | NotForward | NoFixedFlow | CrossesArea | ContainerEndpoint |
|---|---:|---:|---:|---:|---:|---:|---:|
| baseline | 198 (was 191) | 12 (was 19) | 144 | 0 | 510 | 164 | 67 |
| flowPorts | 229 | 104 | 0 | 21 | 510 | 164 | 67 |

No dataset gets more hard violations than its import. Construction stays under 7 ms in total across the 17 datasets.
Per dataset, the baseline gains are layerweave-code 23 → 27, layerweave-pipeline 10 → 12, state-order 7 → 8. The earlier
three-edge loss on layerweave-code came from the post-check that ran after the search, so it is corrected here.

Not measured: refinement time, and the repair pops of the delegated edges. These edges still go through the existing
router, so this is not a cost saving yet.
