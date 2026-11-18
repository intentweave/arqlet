//! Provisional (`isolatedGridFrom`): only the areas with many children without an edge to a sibling get the grid of
//! `isolatedGrid`; every other area is laid out as by default.
use arqlet_model::{Graph, LayoutOptions};
use arqlet_runtime::import_graph_with;
use serde_json::json;

/// A container with `loose` children that no edge joins to each other (each has an edge to `hub` outside), and a
/// small ring at the top level.
fn graph(loose: usize) -> Graph {
    let mut nodes = vec![
        json!({"id": "box", "kind": "ecu", "label": "box"}),
        json!({"id": "hub", "kind": "ecu", "label": "hub"}),
    ];
    let mut edges = Vec::new();
    for i in 0..loose {
        nodes.push(json!({"id": format!("f{i}"), "kind": "frame", "label": format!("f{i}"), "parentId": "box"}));
        edges.push(json!({"id": format!("e{i}"), "kind": "sends", "source": format!("f{i}"), "target": "hub"}));
    }
    for i in 0..3 {
        nodes.push(json!({"id": format!("r{i}"), "kind": "fn", "label": "r"}));
        edges.push(json!({"id": format!("r{i}"), "kind": "calls", "source": format!("r{i}"), "target": format!("r{}", (i + 1) % 3)}));
    }
    serde_json::from_value(json!({"defaultDirection": "tb", "nodes": nodes, "edges": edges}))
        .unwrap()
}

fn options(v: serde_json::Value) -> LayoutOptions {
    serde_json::from_value(v).unwrap()
}

#[test]
fn an_area_with_many_unlinked_children_gets_the_grid() {
    let plain = import_graph_with(graph(60), 1000, &LayoutOptions::default()).unwrap();
    let gridded =
        import_graph_with(graph(60), 1000, &options(json!({"isolatedGridFrom": 50}))).unwrap();
    let aspect = |s: &arqlet_model::Snapshot| {
        let b = &s.boxes["box"];
        b.width.max(b.height) / b.width.min(b.height)
    };
    assert!(
        aspect(&plain) > 50.,
        "one long row by default: {}",
        aspect(&plain)
    );
    assert!(
        aspect(&gridded) < aspect(&plain) / 4.,
        "a grid: {}",
        aspect(&gridded)
    );
    // The same as `isolatedGrid` there.
    let everywhere =
        import_graph_with(graph(60), 1000, &options(json!({"isolatedGrid": true}))).unwrap();
    assert_eq!(gridded.boxes["box"].width, everywhere.boxes["box"].width);
    assert_eq!(gridded.boxes["box"].height, everywhere.boxes["box"].height);
    // The option travels with the snapshot; the computed areas do not.
    let text = serde_json::to_string(&gridded.options).unwrap();
    assert!(text.contains("isolatedGridFrom") && !text.contains("isolatedGridAreas"));
}

#[test]
fn below_the_threshold_the_drawing_is_the_default_one() {
    let plain = import_graph_with(graph(30), 1000, &LayoutOptions::default()).unwrap();
    let mut below =
        import_graph_with(graph(30), 1000, &options(json!({"isolatedGridFrom": 50}))).unwrap();
    below.options = plain.options.clone();
    assert_eq!(
        serde_json::to_string(&below).unwrap(),
        serde_json::to_string(&plain).unwrap()
    );
}
