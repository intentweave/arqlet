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

/// How far each lane reaches across the flow (`lr`: along y), in the declared order, from the boxes alone: the members a
/// group owns.
fn bands(s: &Snapshot, source: &Graph, name: &str, vertical: bool) -> Vec<(String, f64, f64)> {
    let at = arqlet_layout::absolute_all(&s.graph, &s.boxes);
    // A node in several groups stands in the lane of the one the value order chooses.
    let owned = arqlet_model::grouping::owners(source, &lanes(name).grouping_priority);
    let lane_of = |m: &str| match owned.owners.get(m) {
        Some(arqlet_model::grouping::Owner::Group { group, .. }) => Some(group.clone()),
        _ => None,
    };
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
                .filter(|m| lane_of(m).as_deref() == Some(g.id.as_str()))
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

#[test]
fn the_bands_cover_their_lanes_and_meet_without_overlapping() {
    for (text, name, vertical) in [
        (POOLS, "pool", false),
        (THREE, "zone", false),
        (ORG, "project", true),
    ] {
        let g = graph(text);
        let s = import_graph_with(g.clone(), 500, &lanes(name)).unwrap();
        let scene = build_scene(&s).unwrap();
        let drawn = &scene.membership_lanes;
        let expected = bands(&s, &g, name, vertical);
        assert_eq!(drawn.len(), expected.len(), "{name}");
        for (band, (group, lo, hi)) in drawn.iter().zip(&expected) {
            assert_eq!(&band.group, group);
            assert_eq!(band.vertical, vertical);
            let (start, end) = if vertical {
                (band.rect.x, band.rect.x + band.rect.width)
            } else {
                (band.rect.y, band.rect.y + band.rect.height)
            };
            assert!(
                start < *lo && end > *hi,
                "{name} {group}: band {start}..{end}, members {lo}..{hi}"
            );
        }
        for pair in drawn.windows(2) {
            let (a, b) = (&pair[0].rect, &pair[1].rect);
            let (end, start) = if vertical {
                (a.x + a.width, b.x)
            } else {
                (a.y + a.height, b.y)
            };
            assert!((end - start).abs() < 1e-6, "{name}: neighbours meet");
        }
        // Every band reaches over the whole drawing along the flow.
        let b = &scene.bounds;
        for band in drawn {
            let r = &band.rect;
            assert!(r.x >= b.x - 1e-6 && r.y >= b.y - 1e-6);
            assert!(
                r.x + r.width <= b.x + b.width + 1e-6 && r.y + r.height <= b.y + b.height + 1e-6
            );
        }
    }
}

#[test]
fn no_bands_without_lanes_or_where_they_are_not_applied() {
    let g = graph(POOLS);
    let plain = import_graph_with(g.clone(), 500, &LayoutOptions::default()).unwrap();
    assert!(build_scene(&plain).unwrap().membership_lanes.is_empty());
    assert!(!json(&build_scene(&plain).unwrap()).contains("membershipLanes"));
    let mut value: serde_json::Value = serde_json::from_str(POOLS).unwrap();
    value["defaultStrategy"] = "organic".into();
    let organic: Graph = serde_json::from_value(value).unwrap();
    let s = import_graph_with(organic, 500, &lanes("pool")).unwrap();
    assert!(build_scene(&s).unwrap().membership_lanes.is_empty());
}

/// Lanes by `first`, phases by `second`, and the parent hierarchy last.
fn grid(first: &str, second: &str) -> LayoutOptions {
    let mut options = lanes(first);
    options.grouping_priority.insert(
        1,
        GroupingCriterion {
            by: second.into(),
            render: GroupingRendering::Lanes,
            ..Default::default()
        },
    );
    options
}

#[test]
fn phases_follow_one_another_along_the_flow_and_make_a_grid_with_the_lanes() {
    let cases = [
        (POOLS, "pool", "phase"),
        (POOLS, "phase", "pool"),
        (THREE, "zone", "layer"),
        (THREE, "team", "zone"),
        (THREE, "layer", "team"),
    ];
    // Also the pipeline before single pass, which ranks by the cycle break alone.
    for ((text, first, second), legacy) in cases.iter().flat_map(|c| [(*c, false), (*c, true)]) {
        let g = graph(text);
        let mut options = grid(first, second);
        if legacy {
            options.single_pass_auto = false;
            options.single_pass = false;
        }
        let s = import_graph_with(g.clone(), 500, &options).unwrap();
        assert_eq!(s.options.single_pass, !legacy, "{first} x {second}");
        assert!(s.geometry_issues.is_empty(), "{:?}", s.geometry_issues);
        let at = arqlet_layout::absolute_all(&s.graph, &s.boxes);
        // Every node of a phase lies after every node of the phase before, along x (the fixtures flow left to right).
        let phase = arqlet_model::grouping::top_level_phases(&s.graph, second);
        let mut extent: std::collections::BTreeMap<usize, (f64, f64)> = Default::default();
        for (id, p) in &phase {
            let r = &at[id.as_str()];
            let e = extent.entry(*p).or_insert((r.x, r.x + r.width));
            *e = (e.0.min(r.x), e.1.max(r.x + r.width));
        }
        let list: Vec<_> = extent.values().collect();
        for w in list.windows(2) {
            assert!(w[0].1 < w[1].0, "{first} x {second}: {list:?}");
        }
        // The lanes still lie behind one another across the flow.
        assert_bands(&s, &g, first, false);
        let scene = build_scene(&s).unwrap();
        let phase_bands: Vec<_> = scene.membership_lanes.iter().filter(|l| l.phase).collect();
        let lane_bands = scene.membership_lanes.len() - phase_bands.len();
        assert_eq!(phase_bands.len(), extent.len(), "{first} x {second}");
        assert!(lane_bands > 1);
        let report = &scene.membership_report[1];
        assert!(report.applied, "{}", report.summary);
        assert!(
            report
                .summary
                .contains("phases along the flow, 0 out of place"),
            "{}",
            report.summary
        );
        // Phases are bands, not hulls.
        assert!(!scene.membership_hulls.iter().any(|h| h.dimension == second));
        // Every edge keeps its logical direction, also one that runs back from a later phase.
        for e in &s.graph.edges {
            let (Some(t), Some(r)) = (&e.target, s.routes.iter().find(|r| r.id == e.id)) else {
                continue;
            };
            let near = |p: &arqlet_model::Point, id: &str| {
                let b = &at[id];
                p.x >= b.x - 1.
                    && p.x <= b.x + b.width + 1.
                    && p.y >= b.y - 1.
                    && p.y <= b.y + b.height + 1.
            };
            assert!(
                near(&r.points[0], &e.source) && near(r.points.last().unwrap(), t),
                "{}",
                e.id
            );
        }
    }
}

#[test]
fn the_grid_of_pools_and_phases_is_the_target_of_the_fixture() {
    // The fixture's own priority: pools across, phases along. Two lanes, three phases, every task in its cell.
    let g = graph(POOLS);
    let value: serde_json::Value = serde_json::from_str(POOLS).unwrap();
    let options: LayoutOptions =
        serde_json::from_value(serde_json::json!({"groupingPriority": value["groupingPriority"]}))
            .unwrap();
    let s = import_graph_with(g.clone(), 500, &options).unwrap();
    let scene = build_scene(&s).unwrap();
    let cells: Vec<(String, bool)> = scene
        .membership_lanes
        .iter()
        .map(|l| (l.group.clone(), l.phase))
        .collect();
    assert_eq!(
        cells,
        [
            ("customer", false),
            ("supplier", false),
            ("request", true),
            ("approval", true),
            ("fulfil", true)
        ]
        .map(|(g, p)| (g.to_string(), p))
    );
    let at = arqlet_layout::absolute_all(&s.graph, &s.boxes);
    let band = |group: &str| {
        &scene
            .membership_lanes
            .iter()
            .find(|l| l.group == group)
            .unwrap()
            .rect
    };
    for d in &g.memberships.dimensions {
        for group in &d.groups {
            let b = band(&group.id);
            for m in &group.members {
                let r = &at[m.as_str()];
                assert!(
                    r.x >= b.x
                        && r.x + r.width <= b.x + b.width
                        && r.y >= b.y
                        && r.y + r.height <= b.y + b.height,
                    "{m} outside {}",
                    group.id
                );
            }
        }
    }
}
