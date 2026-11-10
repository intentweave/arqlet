# Single pass: edges through a middle area (ADR 0017, amendment A3), 2026-11-08

Revision `ce5cc98` (side nodes beside their leaf, A1–A2) against `ce5cc98` plus A3, Apple M3, release builds. The 11 mixed datasets,
unmodified (raw task), cut to 150 nodes, five seeds, metric means over the seeds. "Before" is
[the A1–A2 report](single-pass-lateral-20261108.md); "default" the refined default of the
[gate report](single-pass-gate-mixed-20261107.md). The machine was heavily loaded by other work (load average 10–56); no time
is claimed. One earlier run of synthetic-2k timed out at load 56 and ran in 144 ms alone; the numbers below come from a
complete later run.

Raw: [single-pass-a3-20261108.json](single-pass-a3-20261108.json). Command:
`node benchmarks/eval/single-pass.mjs --corpus --raw --repeat 1 --seeds 5 --arms singlePass --no-elk DATASETS…`.

## The change

An edge from inside an area C (say `lr`) whose parent M runs across it (`tb`), and whose grandparent G runs like C (`lr`), and
that leaves M too:

- **C** leaves by a border node of its own flow, as across a border of the same axis, instead of a side node for M's flow
  (`through_edges`). Only where C stands outermost on that side of its rank in M's plan of the earlier pass, and only where M
  does not take the edge across G itself (alternating axes over four levels: the outer area goes first).
- **M**'s side node for the edge stands beside C (A1's lateral side node, now also beside a child area of the other axis that
  has a border node for the edge), and its port lies on C's border node's lane (`lateral_child_lane`): the edge crosses M as one
  straight segment.
- **Port side of the end:** the search stops at an area of the other axis than its parent that the edge leaves by a border node.
- **Structure cache:** an area's structure is keyed on what decides these edges (its place in M's rank, M's side nodes), only
  where the axes alternate.
- **Channel widths:** the parent counts the tracks of its channels at the exact place of such ports; the alignment keeps the
  side's order (see the trials).

## Result

| Five-seed means | before (A1–A2) | after (A3) | default |
|---|---:|---:|---:|
| sum of crossings | 4622.6 | **3861.6** | 5135.4 |
| sum of bends | 3741 | **3422** | 3110 |
| cloud-architecture crossings / bends | 36.8 / 141 | **30.4** / 114 | 31 / 84 |
| org-chart crossings / bends | 55 / 373 | 48 / 337 | 122 / 308 |
| synthetic-2k crossings / bends / shared lines | 3693.8 / 1841 / 6.6 | 2946.2 / 1585 / 9.6 | 4064.4 / 1520 / 84.8 |

Against the default: **10 better, 1 worse** (uml-class, 8.6 / 8 crossings; cloud-architecture is now better: 30.4 / 31). Against
the state before: 2 better (cloud-architecture, org-chart), 8 equal, 1 worse (synthetic-2k by shared line pairs, 9.6 / 6.6, all
on edges drawn as seed routes; it delegates 40–41 edges per seed against 39–40). Org-chart no longer delegates (2 before).

Default: byte-identical on all 31 datasets at threads 1 and 4. Uniform and frozen views: byte-identical to A1–A2, except
layerweave-code, whose graph a playground build regenerated in between (353 → 354 edges in its view); no area of those views has
alternating axes.

## Trials

- **No outer priority:** on org-chart (four levels of alternating axes) both a team and its department took the same edge
  through, and 7 edges fell apart (`MixedFlow`). Fixed by letting the outer area go first.
- **Lateral beside any child with a border node:** also beside a child of the area's own axis, whose border nodes are the
  ordinary case; deployment-cluster then delegated 11 edges (`PortSide`, `LongitudinalCapacity`) and uml-class looked better
  (7.2 crossings) only through that error. Restricted to children of the other axis.
- **Port side search stopping at any border node:** deployment-cluster delegated 11 edges (`PortSide`): a border node of an area
  of its parent's axis is ordinary. Restricted to areas of the other axis.
- **Exact lanes in the parent's placement** (aligning to the A3 ports): synthetic-2k delegated 97 edges on seeds 1–4
  (`NarrowChannel` in one channel). The exact places are used for counting the tracks only.
