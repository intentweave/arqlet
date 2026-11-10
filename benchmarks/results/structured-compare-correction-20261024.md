# Correction: the composition ignored authored container directions

Machine: Apple M3, 8 logical CPUs, 16 GiB, macOS 26.0. Revision: bc4d937 (main, after #62) plus the uncommitted correction. Programs: `crates/runtime/examples/structured_compare.rs`, `shape_probe.rs`. Default options, one thread, one run each.

## What was wrong

Every interior level of the hierarchical composition was laid out in the graph's default direction. The authored `containerDirections` of the inputs were ignored. All five diagrams with containers set a direction for every container (network-vlan, bpmn-pools, org-chart, workflow-dag; nested has none).

The joint layout honours these directions, so the comparison was between a joint layout that followed the author and a composition that did not.

## What is superseded

- `structured-compare-20261023.md` (PR #61): the composed numbers for network-vlan, bpmn-pools, org-chart and workflow-dag were computed with the bug. The claim that the composition wins on org-chart and workflow-dag is wrong.
- `structured-compare-diagnosis-20261024.md` (PR #62): the flat containers of network-vlan were caused by the ignored directions (aspect 7, for example vlan10 at 1632 × 220, against 448 × 648 once the authored direction is used). The conclusion that the fixed container shape is the cause is wrong; the proposed size negotiation is not supported by it.

## Corrected numbers (authored directions honoured)

| Diagram | Joint key | Authored composition key | Winner |
|---|---|---|---|
| network-vlan | [0,0,0,83,50,81,126,2696000] | [0,0,2,54,27,56,112,2416611] | joint (shared pairs 2 against 0; the composition is shorter and has fewer crossings: 24166 against 26960, 47 against 66) |
| bpmn-pools | [0,0,0,3,10,11,45,986500] | [0,0,0,3,10,11,46,957300] | joint (bends 46 against 45; the composition is shorter: 9573 against 9865) |
| nested | [0,0,0,0,2,0,11,107200] | [0,0,0,0,2,0,11,100000] | composed, after the shape search (87200; directions tb) |
| org-chart | [0,0,6,108,57,135,290,6761186] | [0,0,12,116,79,127,282,5938856] | joint (shared pairs 12 against 6) |
| workflow-dag | [0,0,33,701,739,0,500,20785480] | [0,0,33,712,774,0,500,20455150] | joint (key entry 3: 712 against 701) |

Within the report, the composition is shorter than the joint layout on network-vlan, bpmn-pools and org-chart, but loses on the key entries that rank before length.

## The shape search

It varies only containers without an authored direction. In these five diagrams, that is none except in nested. So the search is inert here (one composition for network-vlan, bpmn-pools, org-chart and workflow-dag). Its effect is shown only on nested.

## Probe: single flips of an authored direction

Each container with an authored direction was flipped alone, and the whole composition recomputed. The keys are compared with the authored composition (both composed), not with the joint layout.

- network-vlan: 2 of 5 flips improve the authored composition (vlan20, vlan99).
- bpmn-pools: 1 of 5 (lane:fulfilment).
- workflow-dag: 7 of 8 (most change the shared pairs entry, for example stage:0 from 33 to 22).

These flips override the author's direction. They show that the direction matters for the composition, not that the flipped drawings are better layouts for the user. Whether the search may override authored directions is a policy decision and is not made here.

## Decision needed

- Keep authored directions fixed (the current rule): the search has no room in four of five diagrams, and the composition does not beat the joint layout there.
- Treat authored directions as preferences the search may override, with the override reported: this changes what the user asked for.
- Add shape variants that do not change the direction (width or compactness within the same direction): this needs a new layout capability.

## Verification

`cargo fmt --check`, `cargo test --workspace --locked` (37 test groups) pass. Default fingerprints: 17 stable views identical; the structured entry is not part of the default refinement.

## Behaviour change

The structured entry now honours authored container directions, so its results change for every graph with container directions. This is the correction, not a tuning.
