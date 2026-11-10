# Config sets, local exceptions and mixed diagrams

Status: binding small architecture draft; implementation status below.
Supplements the [product specification](specification.md). The goal is extensibility,
not a universal diagram editor in the first step.

## Three independent areas

- **Layout:** position, order, direction, spacing and inner container arrangement.
- **Routing:** connection conditions, port groups, edge routes and bundling.
- **Appearance:** visible markers, colors, shapes, label presentation and LOD.

A config set combines presets for these areas. It changes neither domain identities nor the
meaning of relationships. A profile is not a rule that every element of a diagram must follow.
Hiding port markers does not change connections. "Automatic edge connections instead of individual
ports", by contrast, is a later routing option. Measured shapes and texts act as geometry inputs to
layout and routing; this feedback remains necessary despite the separation of responsibilities.

## Local exceptions

Priority: engine default → global config set → view defaults → next container override → explicit
object condition. Hard domain connection conditions are not overridden by a profile. Missing values
are inherited; explicitly removing an override restores inheritance. Arrays are replaced, not
concatenated. Profile inheritance between config sets is not introduced for now.

Containers form layout boundaries. Inside, a different method may run later; outside, the container
supplies extent and connections. Between inner layout, container size and outer routing, bounded
coordination passes are needed. Coordinates remain local to the container. Different methods must not
be silently simulated under the name "hierarchical". Existing direction overrides are preserved when a
config set is applied.

## Comments and comment boxes

Planned role `annotation`, independent of the domain node type:

- Free position or anchoring to a node, edge or container.
- An anchor contains a target ID and an offset; for edges, later additionally a relative position along
  the route. No binding to an arbitrary segment index.
- Comments do not take part in rank assignment or flow order by default.
- Their rectangles still need space and are routing obstacles by default.
- Association lines are visual connections with no influence on rank assignment.
- When collapsing, comments on hidden targets are hidden by default; a later explicit aggregate
  presentation remains possible.
- If the target is deleted, the comment is kept and the orphaned anchor is diagnosed; it is not silently
  rewritten to another target.

This role is implemented only together with placement, projection and tests. An existing
`kind: comment` string alone does not guarantee this semantics.

## Later diagram methods

- Swimlanes: membership, lane boundaries and connections across lanes.
- Sequence: participant order and event order as additional conditions.
- Timing: time values and a defined time scale.
- Free graph presentation: non-hierarchical placement.
- Radial arrangement: its own layout mode, independent of straight edges.

Straight edges determine their border connection from the node shape and the direction to the
neighbor. No radial layout mode is required for this. Rounded orthogonal lines and free curves are
likewise different routing modes. Until implemented, these methods are not offered as working
options. Unknown methods must be explicitly rejected.

## Small implemented contract (version 1)

`configure` accepts `options: {"configSet": <object>}` as an atomic application. The config set must be
passed separately from further option overrides; subsequent normal `configure` calls may override
individual values.

```json
{
  "schemaVersion": 1,
  "id": "my-code-view",
  "layout": {
    "algorithm": "hierarchical",
    "candidates": [1, 2, 3, 4],
    "hybridBy": "compact"
  },
  "routing": {
    "style": "orthogonal",
    "bundling": "none",
    "routingCandidate": true
  },
  "appearance": {
    "showPorts": true
  }
}
```

All fields except `appearance` are required. Unknown fields, versions, algorithms and routing styles are
rejected. Candidates must be unambiguously in the range 1 to 4; an empty list allows a routing-only pass.
Config sets are ordinary JSON data and can be managed by the calling product. The Rust helper
`ConfigSet::from_options` exports the currently supported profile values. No registry service, no built-in
profile tree.

Snapshots store the resolved values in `options`. Measurements, label texts, graph directions, container
direction overrides and collapse states are preserved when applying. A profile ID is currently an
identifier of the config file, not a stored provenance record in the snapshot. An export with
`from_options` is a config set, not a complete snapshot.

Presets: `presets/*.json` contains four versioned presets (Preview, Grid, Readable, Quality). A layout
preset sets no appearance option: the user's port display is preserved (`appearance` is optional in the
config set). A direction change outside a preset clears the identifier, as does any other option change.
Applying a preset consists of two operations (direction, then config set); this is an intermediate state.
An atomic engine operation follows once the flow is stable. Each carries a strictly validated `configSet`
together with the runtime policy (`graph.defaultDirection`, `refine`, `budgetMs`), which is not part of
the config set. The demo, the React Flow reference and the benchmark runner load the same files. An
applied preset sets `options.configId` to `<id>@<version>`; any change outside a preset clears the
identifier, and the selection then shows "Custom". A test checks that all presets are valid and named.

The demo has a switch for port markers. It changes the stored appearance option, not port labels or port
connections. Importing and exporting config files in the UI and diagram-type presets follow only after this
contract. No generic container overrides, comment semantics, free layout methods or new routing styles are
implemented yet.

## Next small steps

1. Implement the comment role with anchoring and exclusion from ranking.
2. Add container overrides for settings that already work.
3. Respect occupied edge tracks even during obstacle repair. Only then add port groups and true shared
   edge trunks.
4. Two traceable example profiles and config file import/export in the UI.
5. Flexible local coordinate corrections; other diagram methods separately.

Acceptance criteria: applying a profile preserves measurements and explicit connections; a change of
appearance does not alter routes; native and WASM interpret config sets identically; unsupported modes fail
in a traceable way. Future comments must not force ranks, but must be considered in routing and collapse.
Future container exceptions must be able to differ inside without changing outer identities.

## Regional routing requirements

Root and container routing use the same fixed/auto/allowed/inherit semantics as the other discrete axes. A preset or UI action changes authored requirements explicitly; an execution option cannot silently override them. Preserve explicit default-valued declarations and distinguish removing an override from selecting auto. The current defaultRouting/containerRouting fields are a serialization representation, not different semantic scopes. See [ADR 0005](adr/0005-routing-constraints.md).
