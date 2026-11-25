//! ADR 0022 M4a: the groups of the first criterion as lanes (`as: lanes`), bands across the flow of the top level.
use arqlet_model::{Graph, GroupingCriterion, GroupingRendering, LayoutOptions, Snapshot};
use arqlet_runtime::{dispatch, import_graph_with, refine_snapshot};
use arqlet_scene::build_scene;

const POOLS: &str = include_str!("../../../examples/fixtures/memberships/pools-and-lanes.json");
const THREE: &str = include_str!("../../../examples/fixtures/memberships/three-dimensions.json");
const ORG: &str = include_str!("../../../examples/fixtures/memberships/org-chart-projects.json");

fn graph(text: &str) -> Graph {
    serde_json::from_str(text).expect("the fixture parses")
}

fn json<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap()
}

fn lanes(name: &str) -> LayoutOptions {
    let mut options = LayoutOptions::default();
    options.grouping_priority.push(GroupingCriterion {
        by: name.into(),
        render: GroupingRendering::Lanes,
        ..Default::default()
    });
    options.grouping_priority.push(GroupingCriterion {
        by: "parent".into(),
        ..Default::default()
    });
    options
}

/// How far each lane reaches across the flow (`lr`: along y), in the declared order, from the boxes alone.
fn bands(s: &Snapshot, source: &Graph, name: &str, vertical: bool) -> Vec<(String, f64, f64)> {
    let at = arqlet_layout::absolute_all(&s.graph, &s.boxes);
    source
        .memberships
        .dimension(name)
        .unwrap()
        .groups
        .iter()
        .filter_map(|g| {
            let of: Vec<(f64, f64)> = g
                .members
                .iter()
                .filter_map(|m| at.get(m.as_str()))
                .map(|r| {
                    if vertical {
                        (r.x, r.x + r.width)
                    } else {
                        (r.y, r.y + r.height)
                    }
                })
                .collect();
            (!of.is_empty()).then(|| {
                (
                    g.id.clone(),
                    of.iter().map(|e| e.0).fold(f64::MAX, f64::min),
                    of.iter().map(|e| e.1).fold(f64::MIN, f64::max),
                )
            })
        })
        .collect()
}

fn assert_bands(s: &Snapshot, source: &Graph, name: &str, vertical: bool) {
    let b = bands(s, source, name, vertical);
    assert!(b.len() > 1);
    for pair in b.windows(2) {
        assert!(pair[0].2 < pair[1].1, "{name}: {pair:?}");
    }
    assert!(s.geometry_issues.is_empty(), "{:?}", s.geometry_issues);
    let report = &build_scene(s).unwrap().membership_report[0];
    assert!(report.applied, "{}", report.summary);
    assert!(
        report.summary.contains("as lanes, 0 out of place"),
        "{}",
        report.summary
    );
}

#[test]
fn every_lane_lies_behind_the_one_before_in_the_declared_order() {
    for (text, name) in [
        (POOLS, "pool"),
        (POOLS, "phase"),
        (THREE, "zone"),
        (THREE, "team"),
        (THREE, "layer"),
    ] {
        let g = graph(text);
        let s = import_graph_with(g.clone(), 500, &lanes(name)).unwrap();
        assert_bands(&s, &g, name, false);
        assert!(
            !s.graph
                .nodes
                .iter()
                .any(|n| n.id.starts_with("membership:")),
            "a lane is no container"
        );
        // Turned: the lanes run along the flow from top to bottom.
        let mut tb = g.clone();
        tb.default_direction = serde_json::from_str("\"tb\"").unwrap();
        let s = import_graph_with(tb.clone(), 500, &lanes(name)).unwrap();
        assert_bands(&s, &tb, name, true);
    }
}

#[test]
fn lanes_stay_through_edits_refinement_and_configuration() {
    let g = graph(POOLS);
    let s = import_graph_with(g.clone(), 500, &lanes("pool")).unwrap();
    let refined = refine_snapshot(&s, s.revision, s.layout_version).unwrap();
    assert_bands(&refined, &g, "pool", false);
    for n in &g.nodes {
        let added = arqlet_runtime::add_node(&s, s.revision, &n.id, "new").unwrap();
        assert_bands(&added, &g, "pool", false);
    }
    let off = serde_json::json!({
        "op": "configure", "snapshot": s, "expectedRevision": s.revision,
        "expectedLayoutVersion": s.layout_version, "options": {"groupingPriority": []}
    });
    let answer: serde_json::Value = serde_json::from_str(&dispatch(&json(&off))).unwrap();
    assert_eq!(answer["ok"], true, "{answer}");
    let plain: Snapshot = serde_json::from_value(answer["snapshot"].clone()).unwrap();
    assert!(plain.options.grouping_priority.is_empty());
    let on = serde_json::json!({
        "op": "configure", "snapshot": plain, "expectedRevision": plain.revision,
        "expectedLayoutVersion": plain.layout_version,
        "options": {"groupingPriority": lanes("pool").grouping_priority}
    });
    let answer: serde_json::Value = serde_json::from_str(&dispatch(&json(&on))).unwrap();
    assert_eq!(answer["ok"], true, "{answer}");
    let again: Snapshot = serde_json::from_value(answer["snapshot"].clone()).unwrap();
    assert_bands(&again, &g, "pool", false);
}

#[test]
fn nodes_of_no_lane_and_their_containers_come_after_the_lanes() {
    // The chart flows from top to bottom, so the lanes lie along x. Gus and the containers that keep a node are on no project; every project is a lane before them.
    let g = graph(ORG);
    let s = import_graph_with(g.clone(), 500, &lanes("project")).unwrap();
    let at = arqlet_layout::absolute_all(&s.graph, &s.boxes);
    let owned = arqlet_model::grouping::owners(&g, &lanes("project").grouping_priority);
    let in_lane = |id: &str| {
        matches!(
            owned.owners.get(id),
            Some(arqlet_model::grouping::Owner::Group { .. })
        )
    };
    let end = g
        .nodes
        .iter()
        .filter(|n| in_lane(&n.id))
        .map(|n| at[n.id.as_str()].x + at[n.id.as_str()].width)
        .fold(f64::MIN, f64::max);
    let others: Vec<&str> = s
        .graph
        .nodes
        .iter()
        .map(|n| n.id.as_str())
        .filter(|id| !in_lane(id))
        .collect();
    assert!(others.contains(&"gus"));
    for id in others {
        assert!(at[id].x >= end, "{id}");
    }
    assert!(s.geometry_issues.is_empty());
}

#[test]
fn an_area_without_a_flow_says_that_the_lanes_are_not_applied() {
    let mut value: serde_json::Value = serde_json::from_str(POOLS).unwrap();
    value["defaultStrategy"] = "organic".into();
    let g: Graph = serde_json::from_value(value).expect("an organic graph");
    let s = import_graph_with(g, 500, &lanes("pool")).unwrap();
    let report = &build_scene(&s).unwrap().membership_report[0];
    assert!(!report.applied, "{}", report.summary);
}

#[test]
fn lanes_hold_in_a_view_with_many_edges_between_them() {
    // Enough crossings that the hierarchical sweep finds better orders: they must keep the lanes as blocks too.
    let n = 40;
    let nodes: Vec<_> = (0..n)
        .map(|i| serde_json::json!({"id": format!("n{i}"), "kind": "task", "label": "n"}))
        .collect();
    let edges: Vec<_> = (0..n)
        .flat_map(|i| [(i, i * 7 + 5), (i, i * 3 + 11)])
        .map(|(i, j)| (i, j % n))
        .filter(|(i, j)| i < j)
        .enumerate()
        .map(|(k, (i, j))| {
            serde_json::json!({"id": format!("e{k}"), "kind": "flow", "source": format!("n{i}"), "target": format!("n{j}")})
        })
        .collect();
    let groups: Vec<_> = (0..4)
        .map(|l| {
            let members: Vec<_> = (0..n)
                .filter(|i| i * 5 % 4 == l)
                .map(|i| format!("n{i}"))
                .collect();
            serde_json::json!({"id": format!("l{l}"), "members": members})
        })
        .collect();
    let g: Graph = serde_json::from_value(serde_json::json!({
        "defaultDirection": "lr", "nodes": nodes, "edges": edges,
        "memberships": {"dimensions": [{"id": "lane", "groups": groups}]}
    }))
    .unwrap();
    let s = import_graph_with(g.clone(), 500, &lanes("lane")).unwrap();
    assert_bands(&s, &g, "lane", false);
    // Bands that were only moved apart would be several times as high as their fullest ranks.
    let b = bands(&s, &g, "lane", false);
    let height = b.last().unwrap().2 - b[0].1;
    assert!(height < 54. * n as f64, "{height}");
    // Blocks, dummies included, in every rank of the plan.
    let lane = |id: &str| {
        id.strip_prefix('n')
            .and_then(|i| i.parse::<usize>().ok())
            .map(|i| i * 5 % 4)
    };
    for rank in &s.layered_plans["<root>"].ranks {
        let order: Vec<usize> = rank.iter().filter_map(|id| lane(id)).collect();
        assert!(order.windows(2).all(|w| w[0] <= w[1]), "{order:?}");
    }
}
