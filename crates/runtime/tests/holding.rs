//! ADR 0022 M3b (`holdMemberships`): a soft group whose members share a container is held together there, where the budget
//! allows it, and nothing of the container that holds it during the layout is left in the result.
use arqlet_model::{Graph, GroupingCriterion, LayoutOptions, Point, Snapshot};
use arqlet_runtime::{dispatch, import_graph_with, refine_snapshot};
use arqlet_scene::{build_scene, metrics::Metrics};

const CROSSES: &str =
    include_str!("../../../examples/fixtures/memberships/crosses-containers.json");
const THREE: &str = include_str!("../../../examples/fixtures/memberships/three-dimensions.json");
const NON_CONTIGUOUS: &str =
    include_str!("../../../examples/fixtures/memberships/non-contiguous.json");

const FIXTURES: [&str; 7] = [
    CROSSES,
    THREE,
    NON_CONTIGUOUS,
    include_str!("../../../examples/fixtures/memberships/one-group-per-node.json"),
    include_str!("../../../examples/fixtures/memberships/org-chart-projects.json"),
    include_str!("../../../examples/fixtures/memberships/overlapping-boxes.json"),
    include_str!("../../../examples/fixtures/memberships/pools-and-lanes.json"),
];

fn graph(text: &str) -> Graph {
    serde_json::from_str(text).expect("the fixture parses")
}

/// The parent hierarchy first, every dimension held softly.
fn options(graph: &Graph, hold: bool) -> LayoutOptions {
    let by = |name: &str| GroupingCriterion {
        by: name.into(),
        ..Default::default()
    };
    let mut options = LayoutOptions::default();
    options.grouping_priority.push(by("parent"));
    for d in &graph.memberships.dimensions {
        options.grouping_priority.push(by(&d.id));
    }
    options.hold_memberships = hold;
    options
}

fn laid(text: &str, hold: bool) -> Snapshot {
    let g = graph(text);
    let o = options(&g, hold);
    import_graph_with(g, 500, &o).expect("imports")
}

fn measured(s: &Snapshot) -> Metrics {
    arqlet_layout::published_metrics(s).expect("measured")
}

fn json<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap()
}

#[test]
fn a_group_that_shares_a_container_is_held_whole_and_its_members_keep_their_owner() {
    let (free, held) = (laid(CROSSES, false), laid(CROSSES, true));
    let drawn = |s: &Snapshot| {
        build_scene(s)
            .unwrap()
            .membership_hulls
            .iter()
            .filter(|h| h.group == "local")
            .count()
    };
    assert_eq!(drawn(&free), 2, "in pieces without holding");
    assert_eq!(drawn(&held), 1, "whole with it");
    assert_eq!(held.held_groups.len(), 1);
    assert_eq!(held.held_groups[0].group, "local");
    assert_eq!(
        json(&held.graph),
        json(&free.graph),
        "the view is the same graph"
    );
    let scene = build_scene(&held).unwrap();
    let report = scene
        .membership_report
        .iter()
        .find(|r| r.dimension == "focus")
        .unwrap();
    assert_eq!(report.held, 1);
    assert!(report.summary.contains("1 held together"));
}

#[test]
fn nothing_of_the_holding_container_is_left() {
    let mut held = 0;
    for text in FIXTURES {
        let s = laid(text, true);
        held += s.held_groups.len();
        assert!(!json(&s).contains("held:"), "{}", json(&s));
        let boxes: Vec<&str> = s.boxes.keys().map(String::as_str).collect();
        let mut nodes: Vec<&str> = s.graph.nodes.iter().map(|n| n.id.as_str()).collect();
        nodes.sort_unstable();
        assert_eq!(boxes, nodes);
        assert!(s.geometry_issues.is_empty(), "{:?}", s.geometry_issues);
        // The boxes are where a validator that never heard of holding finds them valid.
        let issues = arqlet_scene::validate_geometry(
            &s.graph,
            &s.boxes,
            arqlet_scene::GeometryRules::default(),
        );
        assert!(issues.is_empty(), "{issues:?}");
        // Every edge keeps its logical direction: its route runs from its source's box to its target's.
        let at = arqlet_layout::absolute_all(&s.graph, &s.boxes);
        for e in &s.graph.edges {
            let (Some(target), Some(r)) = (&e.target, s.routes.iter().find(|r| r.id == e.id))
            else {
                continue;
            };
            let near = |p: &Point, id: &str| {
                let b = &at[id];
                p.x >= b.x - 1.
                    && p.x <= b.x + b.width + 1.
                    && p.y >= b.y - 1.
                    && p.y <= b.y + b.height + 1.
            };
            assert!(near(&r.points[0], &e.source), "{}", e.id);
            assert!(near(r.points.last().unwrap(), target), "{}", e.id);
        }
    }
    assert!(held > 0, "the test is about something");
}

#[test]
fn holding_stays_in_its_budget_and_never_breaks_a_group_that_was_whole() {
    for text in FIXTURES {
        let (free, held) = (laid(text, false), laid(text, true));
        let (a, b) = (measured(&free), measured(&held));
        assert!(b.line_node_hits <= a.line_node_hits);
        let crossings = |m: &Metrics| m.crossings + m.shared_endpoint_crossings;
        let n = held.held_groups.len();
        assert!(crossings(&b) <= crossings(&a) + n * 2.max(crossings(&a) / 20));
        assert!(b.bends <= a.bends + n * 4.max(a.bends / 20));
        // Groups inside one container drawn as one hull without holding still are.
        let whole = |s: &Snapshot| {
            let scene = build_scene(s).unwrap();
            scene
                .membership_hulls
                .iter()
                .filter(|h| h.pieces == 1)
                .filter(|h| {
                    let of = |id: &String| {
                        s.graph
                            .nodes
                            .iter()
                            .find(|n| n.id == *id)
                            .unwrap()
                            .parent_id
                            .clone()
                    };
                    h.boxes.iter().all(|b| of(b) == of(&h.boxes[0]))
                })
                .map(|h| (h.dimension.clone(), h.group.clone()))
                .collect::<std::collections::BTreeSet<_>>()
        };
        assert!(whole(&held).is_superset(&whole(&free)));
    }
    // Holding g3 of the Euler case would break another group, so it is released.
    assert!(laid(NON_CONTIGUOUS, true).held_groups.is_empty());
    // Two more crossings on rank 3, whose budget is one: released.
    assert!(laid(THREE, true).held_groups.is_empty());
}

#[test]
fn off_is_the_default_and_omitted_and_holding_is_deterministic() {
    let free = laid(CROSSES, false);
    assert!(!json(&free).contains("holdMemberships") && !json(&free).contains("heldGroups"));
    assert_eq!(json(&laid(CROSSES, true)), json(&laid(CROSSES, true)));
    // Without a soft dimension there is nothing to hold, and the drawing is the default one.
    let o = LayoutOptions {
        hold_memberships: true,
        ..Default::default()
    };
    let alone = import_graph_with(graph(CROSSES), 500, &o).unwrap();
    let default = import_graph_with(graph(CROSSES), 500, &LayoutOptions::default()).unwrap();
    assert_eq!(json(&alone.boxes), json(&default.boxes));
    assert_eq!(json(&alone.routes), json(&default.routes));
}

#[test]
fn refinement_and_the_structured_route_keep_what_is_held() {
    let held = laid(CROSSES, true);
    let refined = refine_snapshot(&held, held.revision, held.layout_version).unwrap();
    assert_eq!(json(&refined.boxes), json(&held.boxes));
    assert_eq!(refined.held_groups, held.held_groups);
    // The same with the pipeline that plans candidates.
    let g = graph(CROSSES);
    let mut o = options(&g, true);
    o.single_pass_auto = false;
    let legacy = import_graph_with(g.clone(), 500, &o).unwrap();
    let refined = refine_snapshot(&legacy, legacy.revision, legacy.layout_version).unwrap();
    assert_eq!(refined.held_groups, legacy.held_groups);
    if !legacy.held_groups.is_empty() {
        assert_eq!(json(&refined.boxes), json(&legacy.boxes));
    }
    let view = laid(CROSSES, true).graph;
    let (structured, _) =
        arqlet_runtime::coordinator::refine_structured(&view, &options(&view, true)).unwrap();
    assert_eq!(structured.held_groups, held.held_groups);
}

#[test]
fn the_option_takes_effect_when_it_is_configured_and_goes_again() {
    let configure = |s: &Snapshot, hold: bool| -> Snapshot {
        let answer: serde_json::Value =
            serde_json::from_str(&dispatch(&json(&serde_json::json!({
                "op": "configure", "snapshot": s, "expectedRevision": s.revision,
                "expectedLayoutVersion": s.layout_version, "options": {"holdMemberships": hold}
            }))))
            .unwrap();
        assert_eq!(answer["ok"], true, "{answer}");
        serde_json::from_value(answer["snapshot"].clone()).unwrap()
    };
    let on = configure(&laid(CROSSES, false), true);
    assert_eq!(on.held_groups.len(), 1);
    let off = configure(&on, false);
    assert!(off.held_groups.is_empty() && !off.options.hold_memberships);
}

#[test]
fn a_member_with_a_fixed_place_is_not_moved_into_a_held_group() {
    let g = graph(CROSSES);
    let mut o = options(&g, true);
    let member = g.memberships.dimension("focus").unwrap();
    let member = &member
        .groups
        .iter()
        .find(|x| x.id == "local")
        .unwrap()
        .members[0];
    let free = laid(CROSSES, false);
    let at = &free.boxes[member];
    o.pins.insert(member.clone(), Point { x: at.x, y: at.y });
    let s = import_graph_with(g, 500, &o).unwrap();
    assert!(s.held_groups.is_empty());
}

#[test]
fn an_edit_reports_only_what_is_still_whole() {
    let held = laid(CROSSES, true);
    let source = held.graph.edges[0].source.clone();
    let after = arqlet_runtime::add_node(&held, held.revision, &source, "new").unwrap();
    let scene = build_scene(&after).unwrap();
    for h in &after.held_groups {
        let pieces = scene
            .membership_hulls
            .iter()
            .filter(|x| x.dimension == h.dimension && x.group == h.group)
            .count();
        assert_eq!(pieces, 1, "{h:?} is said to be held");
    }
    assert!(after.options.hold_memberships, "the option goes on");
}

#[test]
fn every_kind_of_edit_works_on_a_drawing_that_holds_a_group() {
    let held = laid(CROSSES, true);
    let call = |s: &Snapshot, mut request: serde_json::Value| -> Snapshot {
        request["snapshot"] = serde_json::to_value(s).unwrap();
        request["expectedRevision"] = s.revision.into();
        request["expectedLayoutVersion"] = s.layout_version.into();
        let answer: serde_json::Value = serde_json::from_str(&dispatch(&json(&request))).unwrap();
        assert_eq!(answer["ok"], true, "{request:?}: {}", answer["error"]);
        serde_json::from_value(answer["snapshot"].clone()).unwrap()
    };
    let containers: Vec<String> = held
        .graph
        .nodes
        .iter()
        .filter(|n| {
            held.graph
                .nodes
                .iter()
                .any(|m| m.parent_id.as_deref() == Some(&n.id))
        })
        .map(|n| n.id.clone())
        .collect();
    for c in &containers {
        let closed = call(
            &held,
            serde_json::json!({"op": "toggle", "nodeId": c, "containerId": c}),
        );
        call(
            &closed,
            serde_json::json!({"op": "toggle", "nodeId": c, "containerId": c}),
        );
    }
    for e in &held.graph.edges {
        let added = arqlet_runtime::add_node(&held, held.revision, &e.source, "new").unwrap();
        let refined = refine_snapshot(&added, added.revision, added.layout_version).unwrap();
        assert!(refined.geometry_issues.is_empty());
        arqlet_runtime::add_node(&added, added.revision, &e.source, "again").unwrap();
    }
    let structured = call(&held, serde_json::json!({"op": "structuredRefine"}));
    assert!(structured.geometry_issues.is_empty());
}
