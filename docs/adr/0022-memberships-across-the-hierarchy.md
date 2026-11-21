# ADR 0022: Memberships across the hierarchy

Date: 2026-11-21.
Status: proposed. Docs only; no code and no capability status change.

Related: [ADR 0003](0003-container-direction-constraints.md) (direction and strategy per area), [ADR 0005](0005-routing-constraints.md)
(routing per container, the outline contract), [ADR 0007](0007-recursive-strategy-orchestration.md) (containment, optimization
partition and membership groups are different concepts), [ADR 0010](0010-containment-tree-and-container-sizing.md) (the
containment tree is a view of the same ids, not a second hierarchy), [ADR 0017](0017-area-border-legs.md) (area borders as a
contract), [ADR 0019](0019-engine-interface.md) (view model, `details`, sessions).
Plan: [roadmap](../roadmap.md) §6.

## Context

A node has exactly one container (`parentId`). Everything that groups nodes today nests inside that one hierarchy:

- `LayoutOptions.groups` (provisional) takes **one path per node** — `{"billing-api": ["Billing", "Invoices"]}` — and makes
  containers of kind `group` in the view only, nested along the path.
- `communities` divides what is still large with Louvain, again as containers in the view.
- `levelOfDetail` decides which of those start collapsed.

All three are one tree. A node that belongs to a zone, a team and a security domain **at the same time** cannot be shown: the
three groupings cut across each other, and only one of them can be the container. Hosts work around it by choosing one
grouping at import and losing the rest, or by importing the same graph three times.

Two constraints from earlier decisions bound the design:

- **ADR 0007:** "Semantic containment, optimization partition and membership groups are different concepts. Containment
  provides one spatial owner and local coordinate system. … Potential overlapping groups do not automatically become
  independently solvable containers." So overlapping groups must not become a second containment tree.
- **ADR 0010:** the containment tree is a view of the same ids, not a second hierarchy.

And two hard parts, named in the roadmap:

- **Contiguity is not always possible.** Three groups that pairwise share a member cannot all occupy a contiguous block of
  one order (an Euler-diagram case; the fixture `non-contiguous.json` is the minimal instance). Any design that assumes
  contiguity will fail on real data, so non-contiguity must be a reported outcome, not a crash or a silent reordering.
- **Hulls cross container borders** and must not obstruct the routing.

## Decision

Memberships are **graph data**, separate from `parentId`, grouped by **dimension**. How a dimension is shown is a **view
option**, not data. Memberships never change the containment tree and never create a layout region.

### 1. Model: `Graph.memberships`

Memberships are authored domain data, like `buses` (ADR 0005), not something the engine derives. They live in the canonical
graph, which keeps its hierarchy unchanged.

```json
"memberships": {
  "dimensions": [
    {
      "id": "zone",
      "label": "Zone",
      "groups": [
        { "id": "dmz",        "label": "DMZ",        "members": ["gateway", "authn"] },
        { "id": "internal",   "label": "Internal",   "members": ["authz", "orders"] },
        { "id": "restricted", "label": "Restricted", "members": ["ledger"], "parent": "internal" }
      ]
    }
  ]
}
```

Rules:

- `dimensions` is an **array**, so the dimension order is declared and deterministic. Dimension ids are unique.
- `groups` is an **array** per dimension, so the group order is declared: it is the lane order, the legend order and the
  iteration order of everything derived. Group ids are unique within their dimension; a group is named
  `<dimension>/<group>` where one name is needed.
- `members` are node ids. An id the graph does not have is **ignored and counted**, not an error — a host mapping may cover
  more than the imported view, exactly as `groups` does today.
- `parent` nests a group inside another group **of the same dimension** (the general form of today's group path). Cycles are
  refused. Membership in a nested group **implies** membership in its ancestors, so a parent group does not list its
  children's members again.
- A node may be in **several groups of one dimension**. That is the point of an overlay dimension, and it is a conflict only
  for the display modes that need a partition (rule 3). For those modes the test is not "exactly one group" but "one path":
  a node's groups must all lie on one chain of `parent` links, which is what nesting means.
- `label` is carried to the viewer; the engine does not read it, except to name a group that has no label (then its id).
- Omitted when empty. A graph without memberships serializes exactly as today.

**Rejected:** node-keyed memberships (`{nodeId: {dimension: [path]}}`), the shape `groups` uses. It reads well for one node,
but it gives no place for the lane order and the group labels, so a second table would be needed anyway, and the dimension
and group order would have to be derived from sorted ids rather than declared.

**Rejected:** memberships in `LayoutOptions` beside `groups`. Memberships are facts of the domain that an export carries, not
a derived view construct like `communities`. A host that holds them only as a side mapping can still put them into the graph
it hands to `import`. `groups` stays as it is (rule 3).

### 2. View option: `membershipViews`

One display mode per dimension, in `LayoutOptions`:

```json
"membershipViews": { "zone": "lanes", "team": "overlay", "layer": "off" }
```

| Mode | Meaning |
| --- | --- |
| `off` | Default. Nothing is drawn and nothing is read. Omitted from serialization. |
| `hierarchy` | The dimension becomes containers in the **view only**, nested along `parent` — exactly what `groups` does today. |
| `lanes` | The dimension partitions the **order**: a band across the flow. A second lanes dimension gives the grid (pools and lanes). |
| `overlay` | A hull around the members, kept together softly. The hull is scene geometry, never a layout owner. |

Limits, so the layout problem stays one problem:

- **At most one `hierarchy` dimension.** A second one would be a second containment tree, which ADR 0007 forbids. Today's
  `groups` is the short form of exactly this mode; both together are refused.
- **At most two `lanes` dimensions.** The first runs across the flow, the second along it; together they are the grid of
  pools and lanes. A third has no free axis.
- **Any number of `overlay` dimensions.** They do not partition anything.
- The default is `off` for every dimension, and `membershipViews` is omitted when empty, so **default output stays
  byte-identical**. Check the stored fingerprints (`benchmarks/results/fingerprints-20261121-default-0.7.0.jsonl`) before the
  change is accepted.

### 3. What each mode does, and what it may not do

**`hierarchy`.** Containers of kind `group` in the view, derived again at every projection (`arqlet_runtime::project_view`),
so edits keep working and the source graph never holds them — the rule that already holds for `groups` and `communities`.
A node's groups in this dimension must lie on **one path** (rule 1); groups that are not nested in one another are the
conflict `membership-multiple-groups`, and the dimension is not applied. A node in no group stays a direct child of its
container.

**`lanes`.** In a layered area the dimension partitions **every rank's order**: the lanes keep their declared order on the
cross axis, and a node sits in the band of its group. In an organic area the lanes are bands of a grid and the stress is
solved within the band. Lanes are a constraint on the order, not a container: they do not introduce a coordinate system, a
size policy or a border contract. A node in several groups that are not one path is `membership-multiple-groups`; a node in no group is
`membership-unassigned` and is placed outside the lanes, where it would be without them.

**`overlay`.** A hull around the group's members, drawn **behind** the nodes:

- The hull is **rectilinear** in layered areas (the union of the member boxes grown by a padding, outlined orthogonally,
  which matches the orthogonal drawing) and a padded **convex hull** in organic ones. Both are comparison geometry only; no
  trigonometry, so no `PortableMath` call is needed. Rounding, if any, happens in the viewer.
- The hull is **not an obstacle**: the router does not see it, routes may cross it, and a hull never makes a drawing invalid.
  This is the only way to honour "hulls must not obstruct the routing" without turning every group into a region.
- Members are kept together **softly**: adjacent order in layered areas, a group force in organic ones. Soft means it loses
  to every hard constraint and to the published quality key; it never reorders a drawing to a worse key.
- Members inside a **collapsed** container attach to that container, as bus taps do (ADR 0019). A group whose members are all
  hidden is not drawn.

**Never, in any mode:** memberships do not change `parentId`, do not create an optimization region or a solvable container
(ADR 0007), and do not get a direction, strategy, routing or size policy of their own. Only the one `hierarchy` dimension
makes boxes, and those are view containers like today's groups.

### 4. No proxies in version 1

A group that cannot be kept together is **reported**, not duplicated. Proxies (drawing a node twice so two groups are both
contiguous) break the "one spatial owner" rule of ADR 0007 and need an occurrence identity that every operation, every patch
and every trace would have to carry. They are step M5 below, and only after M3 and M4 measure how often they would be
needed.

### 5. Diagnostics: nothing is silent

Reported in `layoutDiagnostics`, never applied silently (the project rule for stale and conflicting results):

| Diagnostic | When |
| --- | --- |
| `membership-multiple-groups` | A node is in several groups of a `hierarchy` or `lanes` dimension that are not one path. The dimension is not applied. |
| `membership-unassigned` | A node is in no group of a `lanes` dimension. |
| `membership-not-contiguous` | A group's members do not form one run in the order that was produced, with the number of runs. |
| `membership-hull-overlap` | Two hulls intersect, with the pair. |
| `membership-unknown-members` | The count of member ids the view does not have (information, not a conflict). |
| `membership-too-many-dimensions` | More than one `hierarchy`, or more than two `lanes` dimensions. |

Refused outright (an error, not a diagnostic): a `parent` cycle, a duplicate dimension or group id, and `groups` together
with a `hierarchy` dimension.

### 6. Measurement before any default

The roadmap asks for a measure of group contiguity and overlap before any default. It is this, reported per drawing and
recorded like every other quality number:

- per group: `contiguityBreaks` (maximal member runs minus one — per rank for lanes, in the final order for overlay),
  `hullIntruders` (non-member nodes inside the hull), `hullArea`;
- per dimension: the sums of those;
- per drawing: the number of intersecting hull pairs.

These numbers are **diagnostics, not part of the published conflict key**: they do not decide acceptance (ADR 0002), because
an overlay must not outrank geometry. A default for any mode is accepted only by the quality gate in `CLAUDE.md` — valid
geometry, every edge in logical direction, 0 line–node hits, crossings and bends against the default and elk-rs on the same
views, recorded timing — **plus** fewer contiguity breaks and hull intruders than the run without the mode.

### 7. The rest of the engine

- **`details`** (ADR 0019, request 31) reports a node's groups **per dimension** beside the containers above it. The
  containment path and the memberships stay distinguishable in the answer.
- **`trace`, `neighbourhood`, `filter`:** a membership group is not a node and carries no edges, so a trace does **not**
  cross it — unlike a bus, which is a view node with taps. A filter that keeps a node keeps its memberships; groups recount
  against what is left.
- **`patch`:** removing a node removes it from its groups; a group that loses every member is not drawn. Unknown member ids
  stay ignored, so a patch that adds a node can make an already-declared membership take effect.
- **Containment tree** (`packages/containment-tree`): unchanged, it shows containment only. A dimension selector is a later
  UI step (ADR 0009/0010), out of scope here.
- **Projection cost:** one reverse index (node id → groups per dimension) is built once per projection, as for `groups`. No
  mode may cost a pass over all nodes per group; §7 of the roadmap (20K to 1M) holds.

## Steps

Each step is behind the option and measured before the next. Nothing becomes a default without the gate in §6.

- **M0 — model and validation, nothing drawn.** `Graph.memberships`, `membershipViews`, the refusals and diagnostics of §5,
  the metrics of §6, the typed contracts. *Exit:* default output byte-identical against the 0.7.0 fingerprints; unknown
  member ids ignored and counted; every refusal and diagnostic tested; the fixtures below parse and lay out.
- **M1 — `hierarchy` as the general form of `groups`.** *Exit:* a `hierarchy` dimension gives the same scene as the
  equivalent `groups` mapping on the same graph; `details` reports the dimension; `groups` keeps working unchanged.
- **M2 — `overlay` drawn only**, with no influence on the layout. *Exit:* the geometry is identical to the run without the
  overlay (the hull is the only addition), hulls are valid, the §6 metrics are reported.
- **M3 — `overlay` keeps members together softly.** *Exit:* the §6 gate on the corpus, and the contiguity and intruder
  numbers improve against M2.
- **M4 — `lanes`**, first one dimension across the flow, then the grid. *Exit:* the §6 gate; `pools-and-lanes.json` draws as
  the 2×3 grid with no lane violation; `three-dimensions.json` reports what it cannot satisfy.
- **M5 — proxies**, only if M3 and M4 show that reporting is not enough, with an explicit occurrence identity and its rules
  for every operation.

## Test data

`examples/fixtures/memberships/` (see its README) carries one fixture per case the steps must answer:
`three-dimensions.json` (the motivating case: three dimensions that cut across each other and across the containers),
`pools-and-lanes.json` (the satisfiable grid), `non-contiguous.json` (the Euler case, provably not satisfiable),
`crosses-containers.json` (a hull across a container border) and `one-group-per-node.json` (a node in two groups of one
dimension: valid as overlay, refused as hierarchy or lanes).

Today's engine ignores the extra fields, so the fixtures already import and lay out as plain graphs.
`crates/runtime/tests/memberships.rs` checks both halves: that every fixture is a valid graph today, and that its
`memberships` block follows §1 and §2 — the validator there is the specification of what `crates/model` has to implement in
M0. One test per fixture pins the case it carries; the Euler case is proved by brute force over all orders, not asserted.

## Open

What this ADR does not decide, and what it needs before it can:

- **The dimensions real exports carry.** `tools/intentweave-export` emits no memberships today. The concrete dimensions
  (`zone`, `team`, `layer`, security domain, …) and their cardinality come from the exporter, and the budget per view
  (how many dimensions and groups a drawing can carry) is a measured question, not a guess.
- **Whether lanes and overlays should shape the layout at all, or only be drawn.** M2 draws, M3 shapes; the decision between
  them is the M3 gate's outcome, not an assumption here.
- **A rank-space interaction with stacked containers.** A lanes dimension partitions rank order, and packing already sets
  rank offsets before placement; which of the two runs first is an M4 question.

## Consequences

- One node can be shown in several groupings at once without a second containment tree, so ADR 0007's rule survives the
  feature instead of being bent by it.
- Non-contiguity becomes a reported, measured property of a drawing rather than a failure mode. Hosts can show "this group
  could not be kept together" instead of a silently scattered group.
- `groups` becomes the short form of one mode rather than its own mechanism, so there is one way to describe grouping and one
  path through the engine.
- Hulls that are drawn but invisible to the router are a deliberate asymmetry: a group can be drawn around nodes that the
  layout kept apart. That is the cost of not making groups into regions.
- Every mode is off by default and omitted from serialization, so the default drawing is unchanged until a gate is passed.
