# Resident sessions keep the prepared source (2026-11-18)

Follow-up of [sessions-large-20261118.md](sessions-large-20261118.md) (ADR 0019 step 3). Machine: Apple M3 (8 cores,
16 GB), macOS (Darwin 25.0.0), loaded (load average 10 to 19; single runs, indications). Build:
`cargo build --release -p arqlet-runtime --bin layout-command`, branch `prepared-source` on `a836a37`. Command:
`python3 benchmarks/scaling/sessions.py --sizes 300000 1000000` (from #34). Raw rows: `sessions-prepared-20261118.jsonl`
(the last state below).

| nodes | message | `a836a37` | shared source | + kept structure | + no revalidation |
|---|---|---|---|---|---|
| 300 000 | toggle, with scene | 2.7 s | 2.6 s | 1.8 s | 1.5 s |
| 300 000 | toggle back, no scene | 2.5 s | 2.5 s | 1.6 s | 1.4 s |
| 1 000 000 | toggle, with scene | 8.5 s | 8.2 s | 4.8 s | 3.8 s |
| 1 000 000 | toggle back, no scene | 8.1 s | 7.3 s | 4.2 s | 3.2 s |

The import (5.5 s and 21 s) and the answers (2.6 MB at 1M) do not change.

What changed, with byte-identical answers (imports of every benchmark dataset and fixture; at 20K and 100K with three
sets of options, session imports, toggles, an export and a one-shot toggle on it, compared with `a836a37`; the 15
refinement fingerprint views):

1. **Shared source graph.** `Snapshot.source_graph` is an `Arc<Graph>`: the snapshots of one view and a session's
   requests hold the same graph instead of a copy each (serialized as before).
2. **Kept structure.** `Snapshot.structure` (not serialized) keeps the structured view (groups, communities, rests) with
   the source it was made from and a key of the options it depends on (`groups`, `communities`, `communityHubs`,
   `partial`, `hidden`); a toggle or reveal of the same source and key takes it instead of computing it, once per toggle
   (it was computed twice). The import seeds it. A snapshot read from JSON computes it at its first view.
3. **No revalidation.** A structured view that was projected once is not validated again (the projection still is).
4. **Projection without a copy of the whole graph**: only the visible nodes and ports are copied.

What remains at 1M (`sample`, before step 3): the projection on the whole structured graph (index maps by id) and the
layout of the view, about half each. Next: an index of the structured graph kept with it (interned ids, ADR 0019
decision 4), so a projection walks arrays instead of maps.
