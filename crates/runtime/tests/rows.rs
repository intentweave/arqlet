//! Rows of a leaf node (ADR 0011): a node is as large as its rows, the rows are stacked inside its padding in the scene,
//! and rows that cannot be placed are refused by name.

use arqlet_model::{Graph, LayoutOptions, Row, ROW_GAP, ROW_PADDING};
use std::collections::BTreeSet;

fn graph() -> Graph {
    serde_json::from_str(
        r#"{"nodes":[{"id":"order","kind":"entity","label":"Order","parentId":null},
                    {"id":"other","kind":"node","label":"Other","parentId":null}],
            "edges":[{"id":"e1","kind":"places (1:N)","source":"order","target":"other"}]}"#,
    )
    .unwrap()
}

fn rows(options: &mut LayoutOptions) {
    options.node_rows.insert(
        "order".into(),
        vec![
            Row {
                id: "id".into(),
                label: "id".into(),
                width: 80.,
                height: 20.,
            },
            Row {
                id: "placed_at".into(),
                label: "placed_at".into(),
                width: 120.,
                height: 20.,
            },
        ],
    );
}

#[test]
fn a_node_is_as_large_as_its_rows() {
    let mut options = LayoutOptions::default();
    rows(&mut options);
    let laid = arqlet_layout::compute(&graph(), None, &BTreeSet::new(), &options).unwrap();
    let node = &laid.boxes["order"];
    assert!(
        (node.width - (120. + 2. * ROW_PADDING)).abs() < 1e-6,
        "{node:?}"
    );
    assert!(
        (node.height - (40. + ROW_GAP + 2. * ROW_PADDING)).abs() < 1e-6,
        "{node:?}"
    );
}

#[test]
fn the_scene_stacks_the_rows_inside_the_node_and_keeps_them_inside() {
    let mut options = LayoutOptions::default();
    rows(&mut options);
    let laid = arqlet_layout::compute(&graph(), None, &BTreeSet::new(), &options).unwrap();
    let scene = arqlet_scene::build_scene(&laid).unwrap();
    let node = scene.nodes.iter().find(|n| n.node.id == "order").unwrap();
    assert_eq!(node.rows.len(), 2);
    let (first, second) = (&node.rows[0].bounds, &node.rows[1].bounds);
    assert!((first.y - (node.bounds.y + ROW_PADDING)).abs() < 1e-6);
    assert!((second.y - (first.y + first.height + ROW_GAP)).abs() < 1e-6);
    for row in &node.rows {
        assert!(
            row.bounds.x >= node.bounds.x
                && row.bounds.x + row.bounds.width <= node.bounds.x + node.bounds.width
        );
        assert!(row.bounds.y + row.bounds.height <= node.bounds.y + node.bounds.height);
    }
    let other = scene.nodes.iter().find(|n| n.node.id == "other").unwrap();
    assert!(other.rows.is_empty());
}

#[test]
fn rows_are_refused_by_name_when_they_are_not_placeable() {
    let mut options = LayoutOptions::default();
    options.node_rows.insert(
        "missing".into(),
        vec![Row {
            id: "x".into(),
            label: "x".into(),
            width: 10.,
            height: 10.,
        }],
    );
    let refused = arqlet_layout::compute(&graph(), None, &BTreeSet::new(), &options).unwrap_err();
    assert!(refused.contains("unknown node: missing"), "{refused}");

    let mut duplicate = LayoutOptions::default();
    rows(&mut duplicate);
    duplicate.node_rows.get_mut("order").unwrap()[1].id = "id".into();
    let refused = arqlet_layout::compute(&graph(), None, &BTreeSet::new(), &duplicate).unwrap_err();
    assert!(refused.contains("unique"), "{refused}");

    let mut negative = LayoutOptions::default();
    rows(&mut negative);
    negative.node_rows.get_mut("order").unwrap()[0].height = -1.;
    let refused = arqlet_layout::compute(&graph(), None, &BTreeSet::new(), &negative).unwrap_err();
    assert!(refused.contains("finite positive"), "{refused}");
}

fn bound_graph(row: &str, side: &str) -> Graph {
    serde_json::from_str(&format!(
        r#"{{"nodes":[{{"id":"order","kind":"entity","label":"Order","parentId":null}},
                    {{"id":"other","kind":"node","label":"Other","parentId":null}}],
            "edges":[{{"id":"e1","kind":"places","source":"order","target":"other"}}],
            "ports":[{{"id":"placed","nodeId":"order","side":"{side}","offset":0.5,"row":"{row}"}}],
            "edgePorts":[{{"edgeId":"e1","sourcePort":"placed","targetPort":null}}]}}"#
    ))
    .unwrap()
}

#[test]
fn a_row_port_sits_on_its_side_at_the_middle_of_its_row() {
    let mut options = LayoutOptions::default();
    rows(&mut options);
    let laid = arqlet_layout::compute(
        &bound_graph("placed_at", "right"),
        None,
        &BTreeSet::new(),
        &options,
    )
    .unwrap();
    let port = laid
        .resolved_ports
        .iter()
        .find(|p| p.id == "placed")
        .expect("the row port is resolved");
    let scene = arqlet_scene::build_scene(&laid).unwrap();
    let node = scene.nodes.iter().find(|n| n.node.id == "order").unwrap();
    let row = node.rows.iter().find(|r| r.id == "placed_at").unwrap();
    assert!(
        (port.position.y - (row.bounds.y + row.bounds.height / 2.)).abs() < 1e-6,
        "{port:?} {row:?}"
    );
    assert!(
        (port.position.x - (node.bounds.x + node.bounds.width)).abs() < 1e-6,
        "{port:?}"
    );
}

#[test]
fn a_row_port_naming_an_unknown_row_or_another_side_is_refused() {
    let mut options = LayoutOptions::default();
    rows(&mut options);
    let unknown = arqlet_layout::compute(
        &bound_graph("missing", "right"),
        None,
        &BTreeSet::new(),
        &options,
    )
    .unwrap_err();
    assert!(unknown.contains("does not have"), "{unknown}");
    let top = arqlet_layout::compute(
        &bound_graph("placed_at", "top"),
        None,
        &BTreeSet::new(),
        &options,
    )
    .unwrap_err();
    assert!(top.contains("left or right side"), "{top}");
}
