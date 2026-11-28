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

/// Two modules of 20 leaves each, one with a nested package, and calls between them.
fn modules() -> Graph {
    let mut nodes = vec![
        json!({"id": "m0", "kind": "module", "label": "m0"}),
        json!({"id": "m1", "kind": "module", "label": "m1"}),
        json!({"id": "p", "kind": "package", "label": "p", "parentId": "m1"}),
    ];
    let mut edges = Vec::new();
    for m in 0..2 {
        for i in 0..20 {
            let parent = if m == 1 && i % 2 == 0 {
                "p".to_string()
            } else {
                format!("m{m}")
            };
            nodes.push(json!({"id": format!("f{m}-{i}"), "kind": if i % 5 == 0 { "test" } else { "fn" }, "label": "f", "parentId": parent}));
            edges.push(json!({"id": format!("e{m}-{i}"), "kind": "call", "source": format!("f{m}-{i}"), "target": format!("f{m}-{}", (i + 3) % 20)}));
        }
    }
    edges.push(json!({"id": "x", "kind": "call", "source": "f0-0", "target": "f1-1"}));
    serde_json::from_value(json!({"nodes": nodes, "edges": edges})).unwrap()
}

/// The same snapshot as read back from JSON: no kept structure, so the next view structures its source anew.
fn stripped(s: &arqlet_model::Snapshot) -> arqlet_model::Snapshot {
    let mut s: arqlet_model::Snapshot =
        serde_json::from_str(&serde_json::to_string(s).unwrap()).unwrap();
    s.structure = None;
    s
}

fn text(s: &arqlet_model::Snapshot) -> String {
    serde_json::to_string(s).unwrap()
}

/// Reveals keep the structure: a sequence of reveal, reveal more, toggle and whole reveals on a session (the structure
/// kept across them) gives, at every step, the snapshot that the same step on the snapshot read back (structured anew)
/// gives, and the structure's view is what `structured` makes of the source with the options.
#[test]
fn a_sequence_of_reveals_equals_the_same_steps_structured_anew() {
    use arqlet_runtime::{reveal_more, reveal_nodes, structured};
    for options in [
        json!({"levelOfDetail": {"depth": 0}}),
        json!({"communities": 6, "levelOfDetail": {"depth": 0}}),
        json!({"levelOfDetail": {"depth": 0}, "hidden": {"nodeKinds": ["test"], "compact": true}}),
        json!({"levelOfDetail": {"depth": 0}, "hidden": {"nodeKinds": ["test"]}}),
    ] {
        let options: LayoutOptions = serde_json::from_value(options).unwrap();
        let mut live = import_graph_with(modules(), 100, &options).unwrap();
        let structure_at_import = live.structure.clone().unwrap();
        let steps: Vec<Box<dyn Fn(&arqlet_model::Snapshot) -> arqlet_model::Snapshot>> = vec![
            Box::new(|s| {
                reveal_nodes(s, s.revision, s.layout_version, &["f0-7".into()], true).unwrap()
            }),
            Box::new(|s| {
                reveal_nodes(
                    s,
                    s.revision,
                    s.layout_version,
                    &["f1-3".into(), "f0-9".into()],
                    true,
                )
                .unwrap()
            }),
            Box::new(|s| reveal_more(s, s.revision, s.layout_version, "m0", 4).unwrap()),
            Box::new(|s| {
                reveal_nodes(s, s.revision, s.layout_version, &["f1-4".into()], true).unwrap()
            }),
            Box::new(|s| {
                let id = s
                    .collapsed_ids
                    .iter()
                    .find(|c| !c.starts_with("rest:"))
                    .cloned()
                    .unwrap_or_else(|| "m0".into());
                toggle_container(s, s.revision, s.layout_version, &id).unwrap()
            }),
            Box::new(|s| {
                reveal_nodes(s, s.revision, s.layout_version, &["f0-1".into()], false).unwrap()
            }),
            Box::new(|s| {
                reveal_more(s, s.revision, s.layout_version, "m0", 100)
                    .unwrap_or_else(|_| s.clone())
            }),
        ];
        for (n, step) in steps.iter().enumerate() {
            let fresh = step(&stripped(&live));
            let next = step(&live);
            assert_eq!(text(&next), text(&fresh), "step {n}, options {options:?}");
            let kept = next.structure.as_ref().expect("a structure is kept");
            let source = next.source_graph.as_ref().unwrap();
            let made = structured(source, &next.options).unwrap();
            assert_eq!(
                serde_json::to_string(&*kept.view).unwrap(),
                serde_json::to_string(&*made).unwrap(),
                "step {n}: the kept view is the structured one"
            );
            // The structure itself is reused while the options that make it stay (`partial` is not among them),
            // except under a compacting filter.
            if !options.hidden.compacts() {
                let under = kept.base.as_ref().unwrap_or(kept);
                assert_eq!(under.key, structure_at_import.key, "step {n}");
            }
            live = next;
        }
    }
}

/// What is shown in part is not part of the structure's key: after a reveal the structure under it is the import's.
#[test]
fn a_reveal_keeps_the_structure_under_what_is_shown_in_part() {
    use arqlet_runtime::reveal_nodes;
    let options: LayoutOptions =
        serde_json::from_value(json!({"communities": 6, "levelOfDetail": {"depth": 0}})).unwrap();
    let first = import_graph_with(modules(), 100, &options).unwrap();
    let at_import = first.structure.clone().unwrap();
    let one = reveal_nodes(
        &first,
        first.revision,
        first.layout_version,
        &["f0-7".into()],
        true,
    )
    .unwrap();
    let two = reveal_nodes(
        &one,
        one.revision,
        one.layout_version,
        &["f1-3".into()],
        true,
    )
    .unwrap();
    for s in [&one, &two] {
        let kept = s.structure.as_ref().unwrap();
        assert!(!kept.partial.is_empty());
        let base = kept.base.as_ref().expect("the structure under the rests");
        assert!(Arc::ptr_eq(&base.view, &at_import.view));
        assert!(Arc::ptr_eq(&base.index, &at_import.index));
    }
}
