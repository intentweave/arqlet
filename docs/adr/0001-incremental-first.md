# ADR 0001: Incremental execution from the first prototype

Status: accepted initial direction.

Backend precomputation and browser edits must operate on a resumable state.
Build a shared Rust core with host adapters and versioned snapshots/patches.
Start with strict stale-result rejection, explicit change events and dependency
invalidation. Avoid full-scene diffing as the primary change mechanism.

Reuse layout algorithms where suitable, but own the graph/view/runtime contracts.
Do not commit to a full ELK port or a particular Rust algorithm yet.

Consequence: the first milestone must demonstrate safe handoff and a concurrent
edit, even with a deliberately simple initial placement algorithm.

Detailed consumer feedback, delta invalidation and iterative continuation are specified in [ADR 0006](0006-reactive-layout-session.md).
