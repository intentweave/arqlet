//! Provisional (ADR 0019): a view's structure (groups, communities) is kept with the snapshot and reused by the next view
//! of the same source graph and the same structure options; the source graph is shared, not copied.
use arqlet_model::{Graph, LayoutOptions};
use arqlet_runtime::{add_node, import_graph_with, toggle_container};
use serde_json::json;
use std::sync::Arc;

/// Three rings of twelve leaves at the top level: `communities` divides them.
fn flat() -> Graph {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    for g in 0..3 {
        for i in 0..12 {
            nodes.push(json!({"id": format!("v{g}-{i}"), "kind": "node", "label": "v"}));
            edges.push(json!({"id": format!("e{g}-{i}"), "kind": "flow", "source": format!("v{g}-{i}"), "target": format!("v{g}-{}", (i + 1) % 12)}));
        }
    }
    serde_json::from_value(json!({"defaultDirection": "lr", "nodes": nodes, "edges": edges}))
        .unwrap()
}

#[test]
fn a_toggle_reuses_the_structure_and_shares_the_source() {
    let options: LayoutOptions = serde_json::from_value(json!({"communities": 10})).unwrap();
    let first = import_graph_with(flat(), 100, &options).unwrap();
    let kept = first
        .structure
        .clone()
        .expect("the import keeps its structure");
    assert!(Arc::ptr_eq(
        &kept.source,
        first.source_graph.as_ref().unwrap()
    ));
    let community = first.collapsed_ids[0].clone();
    let opened =
        toggle_container(&first, first.revision, first.layout_version, &community).unwrap();
    // Same source, same options: the same structure and the same shared graph.
    assert!(Arc::ptr_eq(
        opened.source_graph.as_ref().unwrap(),
        first.source_graph.as_ref().unwrap()
    ));
    assert!(Arc::ptr_eq(
        &opened.structure.as_ref().unwrap().view,
        &kept.view
    ));
    // A snapshot read back (no structure) gives the same view.
    let text = serde_json::to_string(&first).unwrap();
    let loaded: arqlet_model::Snapshot = serde_json::from_str(&text).unwrap();
    assert!(loaded.structure.is_none());
    let again =
        toggle_container(&loaded, loaded.revision, loaded.layout_version, &community).unwrap();
    assert_eq!(
        serde_json::to_string(&again).unwrap(),
        serde_json::to_string(&opened).unwrap()
    );
}

#[test]
fn a_new_source_or_new_structure_options_do_not_reuse_it() {
    let options: LayoutOptions = serde_json::from_value(json!({"communities": 10})).unwrap();
    let first = import_graph_with(flat(), 100, &options).unwrap();
    let kept = first.structure.clone().unwrap();
    // An edit makes a new source graph: the next view structures it anew.
    let visible = first.graph.nodes[0].id.clone();
    let edited = add_node(&first, first.revision, &visible, "new").unwrap();
    assert!(!Arc::ptr_eq(
        edited.source_graph.as_ref().unwrap(),
        &kept.source
    ));
    if let Some(structure) = &edited.structure {
        assert!(!Arc::ptr_eq(&structure.view, &kept.view));
        assert!(Arc::ptr_eq(
            &structure.source,
            edited.source_graph.as_ref().unwrap()
        ));
    }
    // Other structure options under the same source give another key.
    let mut other = first.clone();
    other.options.communities = 20;
    let community = first.collapsed_ids[0].clone();
    if let Ok(result) = toggle_container(&other, other.revision, other.layout_version, &community) {
        assert!(!Arc::ptr_eq(&result.structure.unwrap().view, &kept.view));
    }
}
