# ADR 0022: Memberships across the hierarchy

Date: 2026-11-21; revised 2026-11-22 (grouping priority, §2–§4).
Status: accepted and implemented through M2b (2026-11-23), off by default. Implemented: the model and its refusals (M0),
the hull drawn from boxes already placed, as a rectangle or a convex polygon (M2, `membershipHull`), memberships through
`patch` (§5b, with the change log of ADR 0023 R2), and **the grouping priority as a projection** (M1): `groupingPriority`
ranks the parent hierarchy and the dimensions, the first criterion that assigns a node owns it (its container in the view),
the value order decides among several groups, every later criterion is drawn as hulls, `details` names the groups per
criterion, and the playground offers *Structure first*. `membershipViews` is gone. And **the display stages** (M2b): a soft
group is drawn whole where its hull encloses no foreign node, else in pieces — one per owner, then by distance — and a
group in more than three pieces is marked member by member. Proposed still: holding with a measured switch (M3), lanes
(M4) and the later display options (M5).

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

### Why the revision

The first version gave every dimension a display mode (`hierarchy`, `lanes`, `overlay`) and kept the parent hierarchy as the
one structure that is always the container. Two cases show that this is the wrong axis:

- **An organisation chart viewed by project.** The data's containment is the reporting line (head of department, staff).
  A reader who asks "who works on which project" does not care about that line; projects cut across departments, and one
  person works on two of them. The project assignment should be the structure of the view, and the reporting line a
  secondary one.
- **A process viewed by responsibility.** Swimlanes of responsibility need not match the containment in the source data.
  In that view the lanes are the first structure and span the drawing; the original containers come second.

Both are the same thing: **which structure comes first is a property of the view, not of the data**, and the user chooses
it. Treating the parent hierarchy as one structure among several — the default first one — removes the special case.

## Decision

Memberships are **graph data**, separate from `parentId`, grouped by **dimension** (§1). A view orders the structures it
shows — the parent hierarchy and any dimensions — by a **priority** the user sets (§2). The first structure that assigns a
node owns it; every later one holds it **softly**, as a containment that may be broken (§3). What cannot be held is drawn
as split hulls, and what falls apart too much is marked instead (§3). The canonical graph never changes.

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

- `dimensions` is an **array**, so the dimension order is declared and deterministic. Dimension ids are unique, and `parent`
  is reserved (§2): a dimension of that name is refused.
- `groups` is an **array** per dimension, so the group order is declared: it is the default value priority (§2), the lane
  order, the legend order and the iteration order of everything derived. Group ids are unique within their dimension; a
  group is named `<dimension>/<group>` where one name is needed.
- `members` are node ids. An id the graph does not have is **ignored and counted**, not an error — a host mapping may cover
  more than the imported view, exactly as `groups` does today.
- `parent` nests a group inside another group **of the same dimension** (the general form of today's group path). Cycles are
  refused. Membership in a nested group **implies** membership in its ancestors, so a parent group does not list its
  children's members again.
- A node may be in **several groups of one dimension** — a person on two projects. Which of them owns the node, when that
  dimension owns at all, is the value priority of §2; the others hold it softly.
- `label` is carried to the viewer; the engine does not read it, except to name a group that has no label (then its id).
- Omitted when empty. A graph without memberships serializes exactly as today.

**Rejected:** node-keyed memberships (`{nodeId: {dimension: [path]}}`), the shape `groups` uses. It reads well for one node,
but it gives no place for the lane order and the group labels, so a second table would be needed anyway, and the dimension
and group order would have to be derived from sorted ids rather than declared.

**Rejected:** memberships in `LayoutOptions` beside `groups`. Memberships are facts of the domain that an export carries, not
a derived view construct like `communities`. A host that holds them only as a side mapping can still put them into the graph
it hands to `import`.

### 2. View option: `groupingPriority`

A ranked list of **criteria** in `LayoutOptions`. A criterion is the parent hierarchy (`parent`) or a dimension; the index
in the list is its rank, and there may be as many ranks as the graph has structures:

```json
"groupingPriority": [
  { "by": "project", "values": ["zeus", "apollo"] },
  { "by": "parent" },
  { "by": "site", "as": "lanes" }
]
```

| Field | Meaning |
| --- | --- |
| `by` | `parent` (containment from `parentId`) or a declared dimension id. Each criterion appears at most once. |
| `values` | The value priority of a dimension: which groups come first when a node is in several. Unlisted groups follow in their declared order. Only for dimensions. |
| `as` | `containers` (the default: nested boxes) or `lanes` (bands; §3). |
| `hull` | `rectangle` or `polygon`: the shape of this criterion's hulls (§3); without it, `membershipHull` applies. |

Three rules make the list a projection:

1. **Ownership.** A node's owner in the view is its group in the **first criterion, by rank, that assigns it one** — its
   deepest group on that criterion's path. `parent` assigns every node (a node without `parentId` is owned by the root), so
   listing it anywhere guarantees an owner. Every criterion after the owning one holds the node **softly** (§3).
2. **Value priority.** When the owning criterion is a dimension and the node is in several of its groups that are not one
   path, the `values` order picks the owner (`grouping-multiple-owners`, information); the node's other groups of that
   dimension hold it softly, like a later criterion.
3. **No owner.** A node that no listed criterion assigns — possible only when `parent` is not listed — is placed at the
   root and reported (`grouping-unassigned`). Explicit groups for such nodes are a later display option (§4).

Defaults and limits:

- The default is `[{ "by": "parent" }]` — exactly today's view — and is **omitted from serialization**, so default output
  stays byte-identical. Check the stored fingerprints (`benchmarks/results/fingerprints-20261121-default-0.7.0.jsonl`)
  before the change is accepted.
- A dimension that is not listed is neither drawn nor read.
- At most **two criteria `as: lanes`**: the first runs across the flow, the second along it, which is the grid of pools and
  lanes. A third has no free axis.

**Relation to ADR 0007.** "One spatial owner" holds **in the view**: rule 1 gives every node exactly one. The re-parenting
happens in the projection only, as `groups` and communities already do it (`with_given_groups` sets `parent_id` on the
view's copy); the canonical graph and its `parentId` are untouched, so ADR 0010 holds as well. Overlapping groups still never
become independently solvable containers: two soft groups that share members cannot both be held as containers; the
higher rank holds as a container, and the lower is held as a region across it or split (§3).

**Replaced `membershipViews`.** The first version's `{ "team": "overlay" }` is `[{ "by": "parent" }, { "by": "team" }]` with
holding switched off — only the last stages of §3 drawn. M1 removed `membershipViews` (both were provisional; no
compatibility promise). `groups` stays its own mechanism — sub-containers nested inside each node's container — and is
refused together with a non-default `groupingPriority`; whether it becomes a criterion held as nested containers is a
question for M3, where holding exists.

### 3. Soft criteria are containments that may be broken

For the abstract layout graph, a soft criterion is **a containment that may be broken**. Each of its groups starts as a
container the construction tries to respect — contiguous in each rank's order within its owner in layered areas, one region
in organic ones — and falls back step by step where that is not possible or not worth it. Per group, the stages are:

1. **Held.** The layout keeps the group's members together and draws one hull around them. Within one owner the group is
   a nested, breakable container in the abstract plan. Across owners it cannot be a container — that would move a node out
   of its owner, which rule 1 forbids — but it can still be held as a **region**: the members inside each owner are placed
   at the owner's border that faces the rest of the group, and the owners are placed next to each other, so one hull
   crosses the owner's border and encloses nothing else. That is the target picture below. The same holds for containers
   of a higher rank: a lower rank is held across them, not inside them.
2. **Split.** Where one hull would enclose foreign nodes — because holding is released by cost (below), or because no
   placement keeps the region clean — the group falls into **pieces**, first one per owner, then by distance within an
   owner, each drawn as its own hull.
3. **Marked.** A group that falls into more pieces than a threshold is drawn as **marks** on its members (a colour or a
   badge, no polygon). A group of thirty scattered members as thirty small hulls is noise; as marks it is not, and a mark
   never claims a shape the layout did not produce.

**The shape of a hull is configurable**: `rectangle` (the default) is the padded bounding box of the group's boxes — a box
that looks like a container and may overlap one, as in the target picture; `polygon` is the padded convex hull, tighter
around scattered members, with diagonal edges. `LayoutOptions.membershipHull` sets it for every group, and a criterion's
`hull` overrides it (M1). The rectangle always encloses the polygon, so it finds at least as many foreign nodes; where
either shape would enclose one, the group is split (stage 2) rather than drawn in a third, more complicated outline.
This is deliberate: the split does not depend on the shape chosen, so switching the shape changes outlines only and
never how a group is divided; the price is that a view drawn with `polygon` also gets the splits that only the rectangle
needs. A
hull is drawn behind the nodes and is **not an obstacle** — the router does not see it, routes may cross it, and it never
makes a drawing invalid. Both shapes are comparison geometry, no trigonometry.

**Holding is a switch per piece, not a quality criterion.** The general quality criteria (ADR 0002, the published key) stay
as they are and judge the drawing. Whether a piece is held is decided like a dynamic option: by the **marginal cost** of
holding it in crossings and bends, against a **budget that shrinks with rank** — rank 2 is held longer than rank 3. A held
piece shapes the candidates (as `orderHints` and `directionsFirst` already do), and the quality criteria optimise within
that space, so acceptance cannot optimise the holding away. Holding may therefore cost crossings and bends; that is the
price of the chosen view, accepted by the user who chose it. Where exactly the switches lie — the budgets and the marking
threshold — is not decided here but measured (§6), and both become options the user can set.

**Lanes.** `as: lanes` renders a criterion as bands instead of boxes: in a layered area it partitions every rank's order,
in an organic area it is a band of a grid. Lanes of the **owning** criterion span its owner — at rank 1 the whole drawing,
which is the swimlane view of a process. Lanes of a **later** criterion lie inside the owner of each node: a container of a
higher rank is never cut by a band, it sits in one lane, and a container whose members belong to different lanes splits the
criterion there (stage 2). This is the first version's rule "lanes do not span container borders", now the case of lanes
below a higher rank rather than a rule for all lanes.

Members inside a **collapsed** container attach to that container's box, as bus taps do (ADR 0019). A group whose members
are all hidden is not drawn.

**The target picture** (`overlapping-boxes.json`). Five nodes: `a`, `b` and `s` in a container `A`; `c` and `d` outside
it; a group `B` of `s`, `c` and `d` held softly after the containment. The drawing that counts as success: two boxes that
overlap — `A` as a container, `B` as its hull — with `s` in the overlap, at the side of `A` that faces `c` and `d`; no node
of `A` other than `s` inside `B`, no node of `B` other than `s` inside `A`; edges run across both borders as they need. It
is the smallest case that shows a node in two groupings at once, drawn as what it is.

### 4. Later display options

What a view does with nodes that do not fit one owner is presentation, and several presentations are projections in their
own right. Version 1 reports and does not duplicate:

- **No owner:** at the root, reported (§2, rule 3). Later: an explicit group "unassigned".
- **Several owners in the owning dimension:** the value priority decides, the rest holds softly (§2, rule 2). Later: an
  explicit group "several", for a view that wants them side by side.
- **Proxies** — drawing a node once per group so that each group is whole. A proxy is a projection too, but it needs an
  occurrence identity that every operation, every patch and every trace would carry, and it breaks "one spatial owner"
  unless the occurrences are marked as copies. Only after M3 shows how often holding fails.

### 5. Diagnostics: nothing is silent, and typed rather than printed

| Diagnostic | When |
| --- | --- |
| `grouping-unassigned` | No listed criterion assigns the node; it is placed at the root. |
| `grouping-multiple-owners` | The node is in several groups of its owning dimension that are not one path; the value priority chose (information). |
| `grouping-split` | A soft group falls into pieces, with their number and the reason: no clean region across its owners, or released by cost. |
| `grouping-marked` | A soft group is drawn as marks because it fell into too many pieces. |
| `membership-not-contiguous` | A held group's or a lane's members do not form one run in the order that was produced, with the number of runs. |
| `membership-hull-overlap` | Two hulls of one criterion intersect, with the pair. |
| `membership-unknown-members` | The count of member ids the view does not have (information, not a conflict). |

**The per-group numbers are a typed field, not lines of text.** One string per group in `layoutDiagnostics` would be
thousands of lines on a real graph, and a host would have to parse them back. So the counts live in a structured report
(per criterion, and per group within it: the measures of §6, the stage it reached and which diagnostics it raised), and
`layoutDiagnostics` carries **one summary line per criterion** — "project: 3 of 14 groups split, 1 marked" — so a host that
only prints diagnostics still learns that something is wrong, and a host that acts on them reads the typed field. As built
in M1, the report is the scene's `membershipReport`, one entry per criterion with its `mode` (`owner` or `soft`) and
`summary`: `owned`, `multipleOwners` (rule 2) and `unassigned` (rule 3, on the first entry) carry `grouping-multiple-owners`
and `grouping-unassigned`; since M2b, `split`, `marked` and `intrudersWhole` carry `grouping-split` and `grouping-marked`.

Refused outright (an error, not a diagnostic): a `parent` cycle, a duplicate dimension or group id, a dimension named
`parent`; in `groupingPriority`, a criterion listed twice, a dimension or value that is not declared, `values` on `parent`,
more than two criteria `as: lanes`; and `groups` together with a non-default `groupingPriority`.

### 5b. Changing memberships: part of `patch`

Memberships are graph data, so they change the way the rest of the graph changes — through `patch` (ADR 0019), not through
a new import and not through an operation of their own, which would be a second path for graph data. `GraphPatch` gains
`memberships` (adds or replaces a dimension's groups, as `nodes` and `edges` add or replace) and `removeMemberships` (by
dimension, or by group within one), applied under the same revision checks, with removals first. A membership change
advances the revision and the layout version like any other graph change.

One consequence worth stating: a change to a dimension that **cannot own nodes** changes no geometry — a dimension that is
not listed, or one listed after `parent`, which assigns every node first. The view is then answered from the layout it
already has, with the new revision; a re-layout would move the drawing for no reason. A dimension listed before `parent`
(or with `parent` absent) owns nodes and shapes the view's tree, so a change of it is projected again
(`grouping::can_own`; test `m1_a_membership_patch_projects_again_where_the_dimension_owns_…`).

### 6. Measurement, and where the switches lie

Measures, reported per drawing and recorded like every other quality number:

- `contiguityBreaks` (maximal member runs minus one) for **held groups and lanes**: it measures a linear order, so it
  applies per rank in a layered area and to the order of an area's children;
- `hullIntruders` (leaf boxes of non-members whose centre lies inside the hull) and `hullArea` for **every hull**, held or
  split;
- per criterion: the sums, the pieces, and how many groups reached each stage of §3;
- per drawing: the number of intersecting hull pairs **within one criterion**. Two criteria that cut across each other are
  expected to overlap, so counting those pairs would measure the premise instead of the drawing.

Which measure belongs to which stage matters, because the one-dimensional limit does not bind in two. Three groups that
pairwise share a member cannot all be contiguous in one *order* — the Euler case, which `non-contiguous.json` proves by
exhausting all 720 orders of six nodes. That bounds held groups and lanes, and it is why splitting is reported rather than
avoided. It says nothing about hulls: in the plane, three pairwise-overlapping regions without a common intersection are
drawable. An intruder is a *leaf* whose *centre* lies inside: a container that encloses members is the drawing's own
structure, and a box that grazes the border is not inside.

**Where the switches lie is measured, not chosen.** Three quantities decide the stages of §3, and M3 finds them:

- the **cost of holding** a piece: the difference in crossings and bends between the drawing with the piece held and
  released (the method — a local recomputation or the construction's own estimate — is part of M3);
- the **budget per rank**, which shrinks with the rank;
- the **marking threshold**: the number of pieces (or their smallest size) from which a group is marked instead of hulled.

The procedure: hold everything on the membership fixtures, the corpus views and the exports that carry memberships once the
exporter emits them; record the cost of every piece; choose budgets and the threshold from that distribution; record the
run in `benchmarks/results/` with its machine and revision line. Budgets and threshold then become options with measured
defaults, which the user can change.

These numbers are **diagnostics, not part of the published conflict key**: they do not decide acceptance (ADR 0002). Holding
for listed soft criteria becomes a default only through the gate in `CLAUDE.md` — valid geometry, every edge in logical
direction, 0 line–node hits, crossings and bends against the default and elk-rs on the same views, recorded timing — with
the cost of holding stated beside it. The default view (`parent` only) is unchanged by all of this.

### 7. The rest of the engine

- **`details`** (request 31, #77) reports a node's owner and its soft groups **per criterion** beside the containers above it,
  so the containment path and the memberships stay distinguishable in the answer. `NodeDetails` is serialized with
  `deny_unknown_fields`, so the new field is **optional and omitted when empty**: a typed host built before this ADR keeps
  deserializing the answer unchanged.
- **`trace`, `neighbourhood`, `filter`:** a membership group is not a node and carries no edges, so a trace does **not**
  cross it — unlike a bus, which is a view node with taps. A filter that keeps a node keeps its memberships; groups recount
  against what is left.
- **`patch`:** memberships are changed through `patch` itself (§5b). Removing a node also removes it from its groups, and a
  group that loses every member is not drawn. Unknown member ids stay ignored, so a patch that adds a node can make an
  already-declared membership take effect.
- **`communities`** divide large owner containers as before, whatever criterion the owners come from.
- **Containment tree** (`packages/containment-tree`): unchanged, it shows the source containment. A view of the owners of a
  non-default priority, and a selector for the priority, are later UI steps (ADR 0009/0010), out of scope here.
- **Projection cost:** one reverse index (node id → groups per dimension) is built once per projection, as for `groups`. No
  criterion may cost a pass over all nodes per group; §7 of the roadmap (20K to 1M) holds.

## Steps

Each step is behind an option and measured before the next. Nothing becomes a default without the gate in §6.

- **M0 — model and validation, nothing drawn.** *Exit:* default output byte-identical against the 0.7.0 fingerprints;
  unknown member ids ignored and counted; every refusal and diagnostic tested; the fixtures parse and lay out.
  **Done** (2026-11-21): `Memberships`, `MembershipDimension`, `MembershipGroup` and `MembershipView` in `arqlet-model`,
  both fields omitted when empty (tested: a snapshot of a graph without memberships does not contain the string
  `membership` at all, so the stored fingerprints cannot move), the generated TypeScript contracts, the validator of §1 in
  `crates/runtime/tests/memberships.rs`, and the refusals of the data (duplicate ids, an unknown or cyclic `parent`,
  `groups` beside a `hierarchy` view; test `malformed_memberships_and_conflicting_options_are_refused`).
- **M2 — the last stage drawn, nothing held.** *Exit:* the geometry is identical to the run without it (the hull is the only
  addition), hulls are valid, the measures are reported.
  **Done** (2026-11-21), narrowed: `arqlet_scene::hull` (padded convex hull, point-in-polygon, separating-axis overlap —
  comparisons only), assembled in `build_scene` from boxes the layout has already produced, as scene `membershipHulls` and
  `membershipReport`, drawn by the SVG viewer behind everything else, offered in the playground as *Membership hulls*. On
  three fixtures every node box and every route is byte-identical with the hulls on and off (`membership_hulls.rs`), and the
  same holds through the page (`ui-regression.mjs`). It draws **one hull per group**, which is not yet a stage of §3: a
  group across owners becomes one large polygon.
- **Patch (§5b).** **Done** (2026-11-22), with the change log of ADR 0023 R2.
- **M2b — the display stages of §3, still nothing held.** For each group the first form that encloses no foreign node:
  one hull, else one per owner, else by distance; marks above a piece count (a provisional threshold until M3 measures
  it), in the configured shape. *Exit:* geometry identical with and without;
  no hull on the fixtures encloses a foreign node; the first slice's intruder count reported beside the new one for one
  version, because splitting lowers it by construction and that is not a layout gain.
  **Done** (2026-11-23). `arqlet_scene::hull::pieces` decides the pieces from boxes already placed: the whole group if
  its hull is clean, else one piece per owner (the box's container in the view), else, within an owner, the closest
  pieces joined as long as their joint hull stays clean (squared gaps, ties by index; owners larger than 64 members are
  not merged but fall apart, and so as a rule into marks). A group in more than `MARKS_ABOVE_PIECES` (3, a placeholder for
  M3) pieces becomes `Scene.membershipMarks` — one per member box, a dot in its dimension's hue in the SVG viewer. Hulls
  carry `piece` and `pieces`; the report adds `split`, `marked` and `intrudersWhole` (what the first slice would have
  enclosed). Overlapping pairs are counted between different groups only, not between the pieces of one. Tests: the
  piece rules in `arqlet_scene::hull`; no hull on any of the seven fixtures, under its own priority, encloses a foreign
  box, and no box is in two pieces of one group; a group over four containers is marked, a pair inside one is not; both
  shapes cover each group with the same boxes. On the three-dimension example the nine groups are drawn in more than nine
  clean pieces: they cut across the deployments, which is what the stages are for.
- **M1 — `groupingPriority` as a projection.** Ranked criteria, ownership, value priority, `parent` as a criterion that can
  be moved down, the refusals and diagnostics of §5, `details` per criterion; `membershipViews` removed. No change to the
  layout algorithm: a non-default priority hands the engine a different container tree. *Exit:* the default view
  byte-identical; on `org-chart-projects.json` the person on two projects is owned by the project the value priority
  names first, and the person on none by the next criterion that assigns them — their department, or their site when
  `parent` is not listed — and is reported as unassigned only when no listed criterion does.
  **Done** (2026-11-22). `arqlet_model::grouping` computes the owners (one reverse index per dimension; unit tests), and
  `arqlet_layout::communities::with_grouping_priority` projects them as the first step of `structured`: a node owned by a
  dimension moves into a container `membership:<dimension>/<group>` (kind `membership`, nested along the dimension's
  `parent`), and a source container the projection empties is left out of the view and drawn as a hull instead. Every
  later criterion is drawn as hulls, `parent` too (one hull per source container); the report names per criterion what it
  owns, how many nodes the value order decided and how many no criterion assigns. `details` answers `groupings` per
  criterion (optional, omitted under the default). A criterion's `hull` overrides `membershipHull`; `as: lanes` is
  accepted and reported as not applied yet. The playground offers *Structure first*. Tests: the projection agrees with
  the oracle of `memberships.rs` node by node, the canonical graph and every edge's direction stay as they were, a change
  of priority projects again and back, `[]` and `[parent]` give today's view, the refusals of §5.
  Two faults the tests found on the way, both silent otherwise: the kept view structure was keyed without the priority
  (a change of priority would have reused the old tree), and an import that neither abstracts nor has a node budget never
  projected at all. And one decision taken: a membership patch moves nothing only when no changed dimension can own nodes
  (§5b). The first draft's exit "`[parent, <dimension> held without cost check]` gives the same scene as `groups`" was
  dropped: it needs a holding mode, which is M3.
- **M3 — holding.** Soft criteria as breakable containers within an owner and as regions across owners (members at the
  facing border), the switch per piece, and the measurement of §6 that places it. *Exit:* the gate of §6, the budgets and
  the threshold recorded with their run, and `overlapping-boxes.json` drawn as the target picture of §3.
- **M4 — lanes** (`as: lanes`): at rank 1 across the drawing, below a higher rank inside each owner, two of them as a grid.
  *Exit:* the gate; `pools-and-lanes.json` with its `groupingPriority` draws as the 2×3 grid with no lane violation;
  `three-dimensions.json` reports what it cannot satisfy.
- **M5 — the later display options of §4:** explicit groups "unassigned" and "several", proxies.

M2b and M1 change no layout and need no quality gate; M3 and M4 change the layout and need it. That is the order.

### The first slice: M0 plus M2, narrowed

M2 was the MVP, and deliberately not lanes, because **a group that is only drawn touches no layout code at all**: the hulls
are computed from boxes the layout has already produced, so the exit check is mechanical and `crates/layout` was not opened.
It was narrowed further — one hull per group with the nesting of §1 honoured, a padded convex hull for every area instead
of the rectilinear outline of §3 (a monotone chain is about twenty lines of comparisons; a rectangle union has to be traced
into an outline), and the intruder and overlap counts in `membershipReport`. The rectilinear hull stays the target for
layered areas and is measured against the convex one on the same views.

## Test data

`examples/fixtures/memberships/` (see its README) carries one fixture per case the steps must answer:
`three-dimensions.json` (three dimensions that cut across each other and across the containers), `pools-and-lanes.json`
(the satisfiable grid), `non-contiguous.json` (the Euler case, provably not satisfiable in one order),
`crosses-containers.json` (a group across two owners, and one with an intruder), `one-group-per-node.json` (a node in two
groups of one dimension, and an unknown member id), `org-chart-projects.json` (the case of the revision: a reporting line
as containment, projects across departments, one person on two projects and one on none, and a second dimension) and
`overlapping-boxes.json` (the target picture of §3: a container and a soft group that share one node).

Today's engine ignores the extra fields, so the fixtures import and lay out as plain graphs. `crates/runtime/tests/memberships.rs`
checks that, validates every `memberships` block against §1 and every `groupingPriority` against §2, and implements the
ownership rule of §2 as an oracle that M1 must agree with. One test per fixture pins the case it carries; the Euler case is
proved by brute force over all orders.

## Open

- **Where the switches lie** (§6): the cost of holding, the budget per rank and the marking threshold are measured in M3.
- **The dimensions real exports carry.** `tools/intentweave-export` emits no memberships today; the dimensions and their
  cardinality, and how many criteria a view can carry, come from the exporter and from measurement.
- **A dimension at several ranks.** `values` lets a dimension order its groups; whether one dimension may also appear at two
  ranks (`[project: apollo, parent, project: the rest]`, one project first and the others after the reporting line) is
  open. The list's shape allows it later without a change of format.
- **The parent hierarchy moved down.** A deep reporting line becomes many nested soft groups. Whether every level is held,
  or only the nearest few, is part of M3's measurement.
- **A rank-space interaction with stacked containers.** Lanes partition the order within an owner, and packing already sets
  rank offsets before placement; which runs first is an M4 question.

## Consequences

- A view chooses its structure: the same graph can be read by reporting line, by project or by responsibility, without
  changing the data and without a second containment tree. ADR 0007's rule holds in every view.
- The parent hierarchy stops being a special case. It is the default first criterion, and it can be moved down like any
  other.
- What cannot be held becomes a reported, measured stage — split, then marked — rather than a silently scattered group or a
  hull that swallows half the drawing.
- Holding costs crossings and bends, deliberately and within a budget the user can set. The quality criteria still judge
  the drawing; only the space they optimise in changes.
- Hulls that are drawn but invisible to the router are a deliberate asymmetry: a released group can be drawn around nodes
  that the layout kept apart.
- The default view is unchanged and omitted from serialization until a gate is passed.
