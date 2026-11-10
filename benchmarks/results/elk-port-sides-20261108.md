# Ports on every side, 2026-11-08

Branch `feature/elk-adapter` after `1601804` (channel reservation of the leads), Apple M3, release build of `layout-command`; elk-rs `@openedges/elk-layout`
0.12.1 (pinned in `benchmarks/eval/package-lock.json`). Raw: [elk-port-sides-20261108.json](elk-port-sides-20261108.json).
Command: `node benchmarks/eval/elk-side-ports.mjs --elk --options '{"portSides":false}' target/release/layout-command` (from
`benchmarks/eval`). Deterministic; times are not reported (the machine was heavily loaded).

## Task

40 random flat graphs in ELK's JSON form (seed 7, 6–19 nodes of 80 × 40, 1.4 edges per node, no self-loops), every edge
between two ports whose sides are drawn at random from all four (`FIXED_SIDE`), half `RIGHT`, half `DOWN`: 687 edges, of which
only 1 in 16 has both ports on its flow sides. Every arm is measured by the same `metrics` operation on the returned ELK graph.

## Result

| arm | constructed | crossings | line–node hits | shared line pairs | hugging pairs | bends |
|---|---:|---:|---:|---:|---:|---:|
| fallback (`portSides: false`, before) | 38 of 687 | 241 | 0 | 25 | 161 | **2296** |
| `portSides` (the adapter's default now) | **687 of 687** | **88** | 0 | **0** | **34** | 2522 |
| elk-rs | – | 140 | 0 | **0** | 272 | **1610** |

By the gate's order (overlaps, line–node hits, shared lines, crossings, hugging, bends) `portSides` now wins against both:
no overlaps, hits or shared lines in any arm but the fallback, then 88 crossings against elk-rs's 140 and 34 hugging pairs
against 272. It draws 57 % more bends than elk-rs (10 % more than the fallback); bends are the remaining gap.

Every edge is constructed. The last fallback edges (66 at `1601804`) were track conflicts of leads around a node: two leads
around the same end of a node stood on one line across the flow, and a lead stood on the channel track the channel planning
gave another edge. Now the leads around one end of a node stand a track apart along the flow, nested (the port nearest the side
they go to takes the inner lane and turns first), and the joint channel planning keeps every bend a track from the leads in its
channel and reserves the room (without the reservation: 675 of 687 constructed, 89 crossings).

## What `portSides` does (option, off by default; `crates/layout`)

- **Construction** (`constructive::port_lead`, `lead_lanes`): an end whose port lies across the flow leads straight out to a
  lane beside its side; an end on the side against the flow leads out by the margin (a track further for each lead around the
  same end inside it) and around its node, by the side with fewer leads across the flow. The constructed route runs from lane to lane along the flow as before. The leads of one node
  side take lanes a track apart in an order that keeps them from crossing (`plan_leads`: leaving ends first, the one nearest
  the far end nearest the node). An edge whose target box lies before its source box is built reversed.
- **Placement** (`explicit_lanes`, `align_lanes`): the alignment meets an explicit port at its configured place, and a lead at
  its lane, so the other end is aligned with it; the gap between two neighbours of a rank widens by a track for every lead
  into it beyond the first. Block placement asks the space between neighbours in their original order in its mirrored
  variants too (needed once the space differs by side; unchanged for the symmetric default).
- **Channels** (`joint::reserve_once`, `lead_segments`): the bends of the joint channel planning keep a track from any lead
  across their range in their channel, and the channel's demand includes that room, so growth widens it.
- **Default:** off and omitted; the default drawings are byte-identical (62/62 views against `939674f`), and of the corpus
  views only `fixed-ports` (the one with explicit ports) changes when the option is on.

## Tried and not kept (each measured on the same 40 graphs, gate order)

| variant | crossings | shared | hugging | bends | why not |
|---|---:|---:|---:|---:|---|
| a stub out of the port only when its side faces the other end, no placement change | 248 | 28 | – | 2334 | more shared lines than the fallback |
| leads at the margin, without lanes of their own or room in the placement | 189 | 49 | – | 2608 | leads of two neighbours shared a lane |
| port places in the hierarchical sweep (0 before, 1 after a node) | 111 | 9 | 28 | 2412 | more shared lines |
| the same in the placement's own order | 140 | 12 | 39 | 2422 | more crossings and shared lines |
| the sweep's order kept for every rank | 168 | 9 | 34 | 2434 | more crossings |
| a lead reaches the other end's lane when the way is free | 180 | 12 | 67 | 2234 | more crossings |
| only the outermost lead across the flow does | 118 | 7 | 39 | 2348 | more crossings |
| a lead around the node by the side toward the other end | 99 | 10 | 27 | 2662 | more shared lines and bends |

## Open

- Bends against elk-rs: elk-rs places the other end so that a lead runs straight to it (its order and placement know the
  port sides); the measured ways of feeding port places into our order did not pass the gate.
- `fixed-ports` (hub with ports on four sides, `lr`): crossings 1 → 4, bends 12 → 16 with the option, so it stays off by
  default; the ELK adapter turns it on for graphs with explicit ports.
