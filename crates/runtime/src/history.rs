//! The user's decisions about a view as one value (`Intent`), read from a snapshot and restored in one recomputation. An undo
//! or a redo is a restore of the value from before or after an action. The runtime keeps no stack: a host holds the values and
//! treats them as opaque. Restoring is checked like any other edit (the snapshot must be the current revision and layout
//! version), so a stale undo is refused, never applied silently. Provisional.
use super::{check_current, recompute_layout, validate_pins};
use arqlet_model::{Intent, NodeShape, Snapshot};
use std::collections::BTreeSet;

/// The decisions a snapshot carries. The source graph is the one the user edited; a collapsed view reads from it too.
pub fn intent_of(snapshot: &Snapshot) -> Intent {
    let graph = snapshot.source_graph.as_ref().unwrap_or(&snapshot.graph);
    let options = &snapshot.options;
    Intent {
        pins: options.pins.clone(),
        container_sizing: options.container_sizing.clone(),
        node_sizes: options.node_sizes.clone(),
        collapsed_ids: snapshot.collapsed_ids.clone(),
        shapes: graph
            .nodes
            .iter()
            .filter(|n| n.shape != NodeShape::default())
            .map(|n| (n.id.clone(), n.shape))
            .collect(),
        default_direction: graph.default_direction.clone(),
        container_directions: graph.container_directions.clone(),
        default_strategy: graph.default_strategy.clone(),
        container_strategies: graph.container_strategies.clone(),
        default_routing: graph.default_routing.clone(),
        container_routing: graph.container_routing.clone(),
        ports: graph.ports.clone(),
        edge_ports: graph.edge_ports.clone(),
        label_positions: options.label_positions.clone(),
        order_hints: options.order_hints.clone(),
    }
}

/// Lays the snapshot out again with `intent`. Every name in it must belong to the graph; an unknown one refuses the whole
/// restore. A value equal to the snapshot's own changes nothing and does not advance the layout version.
pub fn restore_intent(
    snapshot: &Snapshot,
    expected: u64,
    layout: u64,
    intent: &Intent,
) -> Result<Snapshot, String> {
    check_current(snapshot, expected, layout)?;
    if *intent == intent_of(snapshot) {
        return Ok(snapshot.clone());
    }
    let mut source = snapshot
        .source_graph
        .clone()
        .unwrap_or_else(|| snapshot.graph.clone());
    let known: BTreeSet<&str> = source.nodes.iter().map(|n| n.id.as_str()).collect();
    let unknown = intent
        .pins
        .keys()
        .chain(intent.container_sizing.keys())
        .chain(intent.node_sizes.keys())
        .chain(intent.collapsed_ids.iter())
        .chain(intent.shapes.keys())
        .chain(intent.container_directions.keys())
        .chain(intent.container_strategies.keys())
        .chain(intent.container_routing.keys())
        .find(|id| !known.contains(id.as_str()));
    if let Some(id) = unknown {
        return Err(format!("The remembered state names an unknown node: {id}"));
    }
    for node in &mut source.nodes {
        node.shape = intent.shapes.get(&node.id).copied().unwrap_or_default();
    }
    source.default_direction = intent.default_direction.clone();
    source.container_directions = intent.container_directions.clone();
    source.default_strategy = intent.default_strategy.clone();
    source.container_strategies = intent.container_strategies.clone();
    source.default_routing = intent.default_routing.clone();
    source.container_routing = intent.container_routing.clone();
    source.ports = intent.ports.clone();
    source.edge_ports = intent.edge_ports.clone();

    let mut options = snapshot.options.clone();
    options.pins = intent.pins.clone();
    options.container_sizing = intent.container_sizing.clone();
    options.node_sizes = intent.node_sizes.clone();
    options.label_positions = intent.label_positions.clone();
    options.order_hints = intent.order_hints.clone();
    options.normalize_container_sizing();
    options.validate_container_sizing()?;
    validate_pins(&source, &options.pins)?;
    super::validate_order_hints(&source, &options.order_hints)?;

    let mut changed = snapshot.clone();
    changed.source_graph = Some(source);
    changed.collapsed_ids = intent.collapsed_ids.clone();
    changed.gesture = None;
    recompute_layout(&changed, &options, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{configure_snapshot, import_graph_with, set_shape, toggle_container};
    use arqlet_model::{Graph, LayoutOptions, Point};

    fn nested() -> Snapshot {
        let text = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../examples/fixtures/structure/nested.json"),
        )
        .unwrap();
        let graph: Graph = serde_json::from_str(&text).unwrap();
        import_graph_with(
            graph,
            200,
            &LayoutOptions {
                layered_construction: false,
                hierarchical_ordering: false,
                ..LayoutOptions::default()
            },
        )
        .unwrap()
    }

    fn geometry(snapshot: &Snapshot) -> String {
        serde_json::to_string(&(&snapshot.boxes, &snapshot.routes)).unwrap()
    }

    #[test]
    fn restoring_the_intent_from_before_undoes_each_kind_of_decision() {
        let base = nested();
        let before = intent_of(&base);
        let leaf = base
            .graph
            .nodes
            .iter()
            .find(|n| {
                !base
                    .graph
                    .nodes
                    .iter()
                    .any(|c| c.parent_id.as_deref() == Some(n.id.as_str()))
            })
            .unwrap()
            .id
            .clone();
        let container = base
            .graph
            .nodes
            .iter()
            .find_map(|n| n.parent_id.clone())
            .unwrap();
        let at = base.boxes[&leaf].clone();
        let steps: Vec<(&str, Snapshot)> = vec![
            (
                "pin",
                configure_snapshot(
                    &base,
                    base.revision,
                    base.layout_version,
                    &serde_json::json!({"pins": {leaf.clone(): Point { x: at.x + 40., y: at.y + 8. }}}),
                )
                .unwrap(),
            ),
            (
                "container size",
                configure_snapshot(
                    &base,
                    base.revision,
                    base.layout_version,
                    &serde_json::json!({"containerSizing": {container.clone(): {"width": {"min": 900}}}}),
                )
                .unwrap(),
            ),
            (
                "shape",
                set_shape(&base, base.revision, base.layout_version, &leaf, NodeShape::Circle).unwrap(),
            ),
            (
                "collapse",
                toggle_container(&base, base.revision, base.layout_version, &container).unwrap(),
            ),
        ];
        for (name, changed) in steps {
            let after = intent_of(&changed);
            assert_ne!(after, before, "{name}: the action changed nothing");
            // Undo: restore the value from before. Redo: restore the value from after.
            let undone =
                restore_intent(&changed, changed.revision, changed.layout_version, &before)
                    .unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(intent_of(&undone), before, "{name}");
            assert_eq!(
                geometry(&undone),
                geometry(&base),
                "{name}: undo gives the original geometry"
            );
            let redone = restore_intent(&undone, undone.revision, undone.layout_version, &after)
                .unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(intent_of(&redone), after, "{name}");
            assert_eq!(
                geometry(&redone),
                geometry(&changed),
                "{name}: redo gives the changed geometry"
            );
        }
    }

    #[test]
    fn the_operations_run_through_the_dispatcher() {
        let base = nested();
        let read: serde_json::Value = serde_json::from_str(&crate::dispatch(
            &serde_json::json!({"op": "intent", "snapshot": base}).to_string(),
        ))
        .unwrap();
        assert_eq!(read["ok"], true, "{read}");
        let moved = configure_snapshot(
            &base,
            base.revision,
            base.layout_version,
            &serde_json::json!({"pins": {base.graph.nodes[0].id.clone(): Point { x: 50., y: 60. }}}),
        )
        .unwrap();
        let back: serde_json::Value = serde_json::from_str(&crate::dispatch(
            &serde_json::json!({"op": "restore", "snapshot": moved, "intent": read["intent"],
                "expectedRevision": moved.revision, "expectedLayoutVersion": moved.layout_version})
            .to_string(),
        ))
        .unwrap();
        assert_eq!(back["ok"], true, "{back}");
        let restored: Snapshot = serde_json::from_value(back["snapshot"].clone()).unwrap();
        assert_eq!(intent_of(&restored), intent_of(&base));
    }

    #[test]
    fn a_restore_of_the_same_value_changes_nothing() {
        let base = nested();
        let same =
            restore_intent(&base, base.revision, base.layout_version, &intent_of(&base)).unwrap();
        assert_eq!(same.layout_version, base.layout_version);
    }

    #[test]
    fn a_stale_restore_and_an_unknown_name_are_refused() {
        let base = nested();
        let mut intent = intent_of(&base);
        assert!(
            restore_intent(&base, base.revision, base.layout_version + 1, &intent)
                .unwrap_err()
                .contains("Stale")
        );
        intent.pins.insert("nowhere".into(), Point { x: 0., y: 0. });
        assert!(
            restore_intent(&base, base.revision, base.layout_version, &intent)
                .unwrap_err()
                .contains("unknown node")
        );
    }
}
