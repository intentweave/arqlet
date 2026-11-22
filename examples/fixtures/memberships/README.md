# Membership fixtures

Test data for [ADR 0022](../../../docs/adr/0022-memberships-across-the-hierarchy.md): a node belongs to several groupings at
once (a zone, a team, a layer), which the single `parentId` hierarchy cannot show.

- `memberships` is **graph data** (ADR 0022 §1): dimensions in a declared order, each with groups in a declared order.
  The engine reads it (M0).
- `groupingPriority` is a **view option** (§2) and belongs to `LayoutOptions`, not to the graph: a ranked list of
  criteria — `parent` or a dimension, with a value order, a rendering and a hull shape. Every fixture carries the view it
  is meant for; a host passes it with the import request. Inside a graph file the engine ignores the key (`GraphWire`
  ignores keys it does not know), so each fixture also imports as a plain graph — the tests do both.

## The fixtures

| File | The case it carries |
| --- | --- |
| `three-dimensions.json` | The motivating case. Nine services in three containers, plus `zone`, `team` and `layer`, each cutting across the containers **and** across each other. No dimension can be the containment tree. `zone` nests (`restricted` inside `internal`), so it also carries the nested-group case. Parallel edges, a self-loop and a two-cycle are included, because those are not edge cases (`AGENTS.md`). |
| `pools-and-lanes.json` | The satisfiable grid: two lanes dimensions (`pool` × `phase`) whose six cells are all occupied, every node in exactly one cell. This is what M4 must draw as pools and lanes. |
| `non-contiguous.json` | The Euler case: three groups of one dimension that pairwise share a member, cyclically. **No** order of the six nodes keeps all three contiguous — the test proves it by brute force over all 720 orders, and pins the best result (one break). The proof bounds a *linear* order, so it bounds `lanes` and the rank order of layered areas, and that is why non-contiguity is reported rather than avoided (§4, §5). It does **not** bound `overlay`: in the plane these three groups are drawable, which is why `contiguityBreaks` is a lanes measure and a hull is judged by its intruders and overlaps instead (§6). The fixture is therefore the case that separates the two modes. |
| `crosses-containers.json` | A hull that crosses a container border (`hot-path` spans `left` and `right`), next to a group that stays inside one container but skips a sibling (`local` is `l1` and `l3`, not `l2`), so the drawing has an intruder inside a hull to count (§6). |
| `one-group-per-node.json` | A node in two sibling groups of one dimension: valid as `overlay`, and the conflict `membership-multiple-groups` as `hierarchy` or `lanes`. It also names one member id the graph does not have, which must be ignored and counted, not refused (§1). |
| `overlapping-boxes.json` | The target picture (ADR 0022 §3): a container `A` holding `a`, `b`, `s`, and a soft group `B` of `s`, `c`, `d`. Success is two overlapping boxes with only `s` in the overlap, at the side of `A` facing `c` and `d`, and no other node of either group inside the other. M3's exit; until then it carries the shape. |
| `org-chart-projects.json` | The case of the revision: departments as the reporting line's containment, projects that cut across them, Dora on two projects, Gus on none, and a second dimension (site). Its `groupingPriority` puts projects first with `zeus` before `apollo`: Zeus owns Dora and Apollo holds her softly; Gus falls to the next criterion that assigns him (his department, or his site without `parent`), and is unassigned only when projects are the only criterion. |

## What is checked, and where

`crates/runtime/tests/memberships.rs` (a module of the runtime's one test binary) does both halves:

1. **The shape**: a validator over the raw JSON implements ADR 0022 §1 and §2 — unique dimension and group ids, `parent`
   inside the same dimension and free of cycles, no empty or duplicated member list, known display modes, at most one
   `hierarchy` and two `lanes`, and for those modes a node's groups must form one path. When `crates/model` learns the
   field, this validator is the specification of what it has to implement.
2. **The graph**: every fixture parses as a `Graph`, imports, places every node, resolves every edge and lays out without a
   geometry issue — with the extra fields ignored.

Plus one test per fixture that pins the case it carries, so an edit cannot quietly turn the Euler case into a satisfiable
one or make a dimension agree with the containment tree.

Run them with:

```sh
cargo test --profile fast -p arqlet-runtime --test all memberships::
```

Not part of the benchmark manifest, and not a dataset for `benchmarks/`: these are correctness cases, and none of them is
large enough to measure anything.
