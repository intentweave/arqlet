//! ADR 0022, first slice: a membership dimension shown as an `overlay` is drawn as a hull behind the nodes and changes
//! nothing else. The two claims that make this slice an MVP are tested here:
//!
//! 1. **Nothing moves.** With the option on, every node box and every route is identical to the drawing without it. The
//!    hulls are the only addition, so no quality gate is needed to accept the step.
//! 2. **A default view is untouched.** Without `groupingPriority`, a graph carrying `memberships` serializes and draws
//!    exactly as the same graph without them.
use arqlet_model::{
    Graph, GroupingCriterion, GroupingRendering, HullShape, LayoutOptions, Snapshot,
};
use arqlet_runtime::import_graph_with;
use arqlet_scene::{build_scene, Scene};
use std::collections::BTreeMap;

const THREE_DIMENSIONS: &str =
    include_str!("../../../examples/fixtures/memberships/three-dimensions.json");
const CROSSES_CONTAINERS: &str =
    include_str!("../../../examples/fixtures/memberships/crosses-containers.json");
const ONE_GROUP_PER_NODE: &str =
    include_str!("../../../examples/fixtures/memberships/one-group-per-node.json");

fn graph(text: &str) -> Graph {
    serde_json::from_str(text).expect("the fixture parses")
}

/// A criterion of the grouping priority.
fn by(name: &str) -> GroupingCriterion {
    GroupingCriterion {
        by: name.into(),
        ..GroupingCriterion::default()
    }
}

/// The options that keep the parent hierarchy first and hold every dimension the graph declares softly, as hulls.
fn overlay_all(graph: &Graph) -> LayoutOptions {
    let mut options = LayoutOptions::default();
    options.grouping_priority.push(by("parent"));
    for dimension in &graph.memberships.dimensions {
        options.grouping_priority.push(by(&dimension.id));
    }
    options
}

/// The report entry of one criterion.
fn report<'a>(scene: &'a Scene, criterion: &str) -> &'a arqlet_scene::SceneMembershipReport {
    scene
        .membership_report
        .iter()
        .find(|r| r.dimension == criterion)
        .unwrap_or_else(|| panic!("no report for {criterion}"))
}

fn laid_out(text: &str, options: &LayoutOptions) -> (Snapshot, Scene) {
    let snapshot = import_graph_with(graph(text), 500, options).expect("imports");
    let scene = build_scene(&snapshot).expect("a scene");
    (snapshot, scene)
}

/// Every node's box and every route's points, as the comparison of two drawings.
fn geometry(scene: &Scene) -> (BTreeMap<String, String>, Vec<String>) {
    let boxes = scene
        .nodes
        .iter()
        .map(|n| {
            (
                n.node.id.clone(),
                format!(
                    "{} {} {} {}",
                    n.bounds.x, n.bounds.y, n.bounds.width, n.bounds.height
                ),
            )
        })
        .collect();
    let routes = scene
        .routes
        .iter()
        .map(|r| {
            let points: Vec<String> = r
                .points
                .iter()
                .map(|p| format!("{},{}", p.x, p.y))
                .collect();
            format!("{} {}", r.id, points.join(" "))
        })
        .collect();
    (boxes, routes)
}

#[test]
fn the_overlay_adds_hulls_and_moves_nothing() {
    for text in [THREE_DIMENSIONS, CROSSES_CONTAINERS, ONE_GROUP_PER_NODE] {
        let g = graph(text);
        let (_, plain) = laid_out(text, &LayoutOptions::default());
        let (_, with) = laid_out(text, &overlay_all(&g));

        assert!(
            plain.membership_hulls.is_empty(),
            "without the option there is no hull, so every earlier drawing is unchanged"
        );
        assert!(
            !with.membership_hulls.is_empty(),
            "the fixture declares groups the view shows, so hulls must be drawn"
        );

        let (boxes_before, routes_before) = geometry(&plain);
        let (boxes_after, routes_after) = geometry(&with);
        assert_eq!(
            boxes_before, boxes_after,
            "a hull may not move a node: it is derived from the boxes the layout already produced"
        );
        assert_eq!(
            routes_before, routes_after,
            "a hull is invisible to the router, so no route may change"
        );
    }
}

#[test]
fn both_hull_shapes_draw_the_same_groups_and_move_nothing() {
    for text in [THREE_DIMENSIONS, CROSSES_CONTAINERS, ONE_GROUP_PER_NODE] {
        let g = graph(text);
        let rectangle = overlay_all(&g);
        let mut polygon = overlay_all(&g);
        polygon.membership_hull = HullShape::Polygon;
        let (_, plain) = laid_out(text, &LayoutOptions::default());
        let (_, boxes) = laid_out(text, &rectangle);
        let (_, hulls) = laid_out(text, &polygon);

        // The shape is drawing only: the same nodes, the same routes, the same groups and the same boxes per group.
        assert_eq!(geometry(&plain), geometry(&boxes));
        assert_eq!(geometry(&plain), geometry(&hulls));
        let groups = |s: &Scene| -> Vec<(String, String, Vec<String>)> {
            s.membership_hulls
                .iter()
                .map(|h| (h.dimension.clone(), h.group.clone(), h.boxes.clone()))
                .collect()
        };
        assert_eq!(groups(&boxes), groups(&hulls));

        for (r, p) in boxes.membership_hulls.iter().zip(&hulls.membership_hulls) {
            // The default is a rectangle: four corners, every edge axis-parallel.
            assert_eq!(r.points.len(), 4, "{}: a rectangle", r.group);
            for i in 0..4 {
                let (a, b) = (&r.points[i], &r.points[(i + 1) % 4]);
                assert!(a.x == b.x || a.y == b.y, "{}: a slanted edge", r.group);
            }
            // The rectangle encloses the polygon, so it can only find as many intruders or more, never fewer.
            assert!(
                p.points
                    .iter()
                    .all(|q| arqlet_scene::hull::contains(&r.points, q)),
                "{}: the polygon reaches outside the rectangle",
                r.group
            );
            assert!(r.intruders.len() >= p.intruders.len(), "{}", r.group);
        }
    }
    // The default is omitted: options that only set the polygon mention it, the default does not.
    let text = serde_json::to_string(&LayoutOptions::default()).unwrap();
    assert!(!text.contains("membershipHull"));
    let polygon = LayoutOptions {
        membership_hull: HullShape::Polygon,
        ..LayoutOptions::default()
    };
    assert!(serde_json::to_string(&polygon)
        .unwrap()
        .contains(r#""membershipHull":"polygon""#));
}

#[test]
fn a_hull_encloses_the_boxes_of_its_members() {
    let g = graph(THREE_DIMENSIONS);
    let (_, scene) = laid_out(THREE_DIMENSIONS, &overlay_all(&g));
    let boxes: BTreeMap<&str, &arqlet_model::Rect> = scene
        .nodes
        .iter()
        .map(|n| (n.node.id.as_str(), &n.bounds))
        .collect();
    for hull in &scene.membership_hulls {
        assert!(
            hull.points.len() >= 3,
            "{}: a hull is a polygon",
            hull.group
        );
        for id in &hull.boxes {
            let r = boxes[id.as_str()];
            // The hull was built from this box's padded corners, so the box's centre lies inside it.
            assert!(
                arqlet_scene::hull::contains(&hull.points, &arqlet_scene::hull::centre(r)),
                "{} is a member of {}/{} but lies outside its hull",
                id,
                hull.dimension,
                hull.group
            );
        }
    }
}

#[test]
fn the_three_dimensions_are_all_drawn_and_reported() {
    let g = graph(THREE_DIMENSIONS);
    let (_, scene) = laid_out(THREE_DIMENSIONS, &overlay_all(&g));
    let dimensions: Vec<&str> = scene
        .membership_report
        .iter()
        .map(|r| r.dimension.as_str())
        .collect();
    assert_eq!(
        dimensions,
        ["parent", "zone", "team", "layer"],
        "the report follows the ranks of the priority"
    );
    let owner = &scene.membership_report[0];
    assert_eq!(owner.mode, "owner");
    assert_eq!(
        owner.owned,
        g.nodes.len(),
        "the parent hierarchy owns every node"
    );
    assert_eq!(
        owner.drawn, 0,
        "the owning criterion draws boxes, not hulls"
    );
    for report in scene.membership_report.iter().filter(|r| r.mode == "soft") {
        assert!(
            report.applied,
            "{}: a soft criterion is applied",
            report.dimension
        );
        assert_eq!(
            report.drawn, report.groups,
            "{}: every group of the fixture has members the view shows",
            report.dimension
        );
        assert_eq!(report.unknown_members, 0, "{}", report.dimension);
        assert_eq!(report.hidden_members, 0, "{}", report.dimension);
        assert!(
            report.summary.contains(&report.dimension),
            "the one line a host prints names the dimension: {}",
            report.summary
        );
    }
    // Membership in a nested group implies the ancestors (ADR 0022 §1): `restricted` sits inside `internal`, so the
    // `internal` hull must also enclose ledger and audit, which only `restricted` lists.
    let internal = scene
        .membership_hulls
        .iter()
        .find(|h| h.group == "internal")
        .expect("internal is drawn");
    for id in ["ledger", "audit"] {
        assert!(
            internal.boxes.iter().any(|b| b == id),
            "{id} is a member of restricted, which is nested in internal: {:?}",
            internal.boxes
        );
    }
    let restricted = scene
        .membership_hulls
        .iter()
        .find(|h| h.group == "restricted")
        .expect("restricted is drawn");
    assert_eq!(
        restricted.boxes.len(),
        2,
        "the nested group itself holds only its own members"
    );

    // Hulls of one dimension carry the group labels, not the ids, where the fixture gives labels.
    let zone: Vec<&str> = scene
        .membership_hulls
        .iter()
        .filter(|h| h.dimension == "zone")
        .map(|h| h.label.as_str())
        .collect();
    assert_eq!(zone, ["DMZ", "Internal", "Restricted"]);
}

#[test]
fn an_unknown_member_is_counted_and_not_refused() {
    let g = graph(ONE_GROUP_PER_NODE);
    let (_, scene) = laid_out(ONE_GROUP_PER_NODE, &overlay_all(&g));
    let report = report(&scene, "zone");
    assert_eq!(
        report.unknown_members, 1,
        "the fixture names one id the graph does not have, which is ignored and counted"
    );
    assert_eq!(report.drawn, 2, "both groups are still drawn");
}

#[test]
fn two_groups_sharing_a_node_overlap_and_say_so() {
    let g = graph(ONE_GROUP_PER_NODE);
    let (_, scene) = laid_out(ONE_GROUP_PER_NODE, &overlay_all(&g));
    let report = report(&scene, "zone");
    assert_eq!(
        report.overlapping_pairs, 1,
        "north and south share the node `shared`, so their hulls must meet"
    );
}

#[test]
fn a_group_across_a_container_border_is_drawn_as_one_hull() {
    let g = graph(CROSSES_CONTAINERS);
    let (_, scene) = laid_out(CROSSES_CONTAINERS, &overlay_all(&g));
    let hot = scene
        .membership_hulls
        .iter()
        .find(|h| h.group == "hot-path")
        .expect("the crossing group is drawn");
    assert_eq!(
        hot.boxes.len(),
        3,
        "one hull covers members of two containers (ADR 0022 §3): {:?}",
        hot.boxes
    );
    // `local` is l1 and l3 and skips l2, which lies between them, so the hull reports an intruder.
    let local = scene
        .membership_hulls
        .iter()
        .find(|h| h.group == "local")
        .expect("the local group is drawn");
    assert!(
        local.intruders.contains(&"l2".to_string()),
        "l2 sits between l1 and l3 but is not a member, so it is an intruder: {:?}",
        local.intruders
    );
}

#[test]
fn lanes_are_accepted_and_reported_as_not_applied_yet() {
    let mut options = LayoutOptions::default();
    options.grouping_priority = vec![
        by("parent"),
        GroupingCriterion {
            render: GroupingRendering::Lanes,
            ..by("zone")
        },
    ];
    let (_, scene) = laid_out(THREE_DIMENSIONS, &options);
    let zone = report(&scene, "zone");
    assert!(!zone.applied, "lanes arrive with M4");
    assert!(
        zone.summary.contains("not applied yet"),
        "a host must learn that it asked for something the engine does not do yet: {}",
        zone.summary
    );
    assert!(
        zone.drawn > 0,
        "until then the zone is held softly, as hulls"
    );

    // With the parent hierarchy first nothing is projected, so the drawing is the plain one.
    let (_, plain) = laid_out(THREE_DIMENSIONS, &LayoutOptions::default());
    assert_eq!(geometry(&plain), geometry(&scene));
}

#[test]
fn a_graph_without_memberships_never_mentions_them() {
    // The guarantee the stored fingerprints rest on: both new fields are omitted when empty, so every dataset that
    // carries no memberships serializes exactly as it did before this change.
    let mut plain = graph(THREE_DIMENSIONS);
    plain.memberships = Default::default();
    let snapshot = import_graph_with(plain, 500, &LayoutOptions::default()).expect("imports");
    let text = serde_json::to_string(&snapshot).expect("serializes");
    assert!(
        !text.contains("membership"),
        "neither the graph's memberships nor the view option may appear in a default snapshot"
    );
}

#[test]
fn carrying_memberships_without_a_view_option_draws_the_same_picture() {
    // The data travels with the snapshot (it is canonical, so the source graph keeps it too), but at default options no
    // mode reads it, so the drawing must be the one the graph had without it.
    let with = graph(THREE_DIMENSIONS);
    let mut without = with.clone();
    without.memberships = Default::default();

    let a = import_graph_with(with, 500, &LayoutOptions::default()).expect("imports");
    let b = import_graph_with(without, 500, &LayoutOptions::default()).expect("imports");
    let (left, right) = (
        build_scene(&a).expect("a scene"),
        build_scene(&b).expect("a scene"),
    );
    assert_eq!(
        geometry(&left),
        geometry(&right),
        "carrying the data may not change a single box or route"
    );
    assert!(
        left.membership_hulls.is_empty() && left.membership_report.is_empty(),
        "and it may not add anything to the scene until a mode asks for it"
    );
    assert!(
        serde_json::to_value(&a).expect("serializes")["graph"]["memberships"].is_object(),
        "while the canonical graph does keep what it was given"
    );
}

/// The refusals of ADR 0022 §5: data the engine would otherwise accept now and have to refuse later.
#[test]
fn malformed_memberships_and_conflicting_options_are_refused() {
    let base = |memberships: serde_json::Value| {
        serde_json::json!({
            "nodes": [{"id": "a", "kind": "k", "label": "a"}, {"id": "b", "kind": "k", "label": "b"}],
            "edges": [],
            "memberships": memberships,
        })
    };
    let import = |g: serde_json::Value| {
        let graph: Graph = serde_json::from_value(g).expect("parses");
        import_graph_with(graph, 100, &LayoutOptions::default())
    };
    let cases = [
        (
            "a dimension twice",
            serde_json::json!({"dimensions": [{"id": "z", "groups": [{"id": "g", "members": ["a"]}]},
            {"id": "z", "groups": [{"id": "h", "members": ["b"]}]}]}),
            "duplicate membership dimension",
        ),
        (
            "a group twice",
            serde_json::json!({"dimensions": [{"id": "z", "groups": [{"id": "g", "members": ["a"]},
            {"id": "g", "members": ["b"]}]}]}),
            "duplicate membership group",
        ),
        (
            "an unknown parent",
            serde_json::json!({"dimensions": [{"id": "z", "groups": [{"id": "g", "members": ["a"],
            "parent": "nowhere"}]}]}),
            "does not declare",
        ),
        (
            "a parent cycle",
            serde_json::json!({"dimensions": [{"id": "z", "groups": [{"id": "g", "members": ["a"],
            "parent": "h"}, {"id": "h", "members": ["b"], "parent": "g"}]}]}),
            "parent cycle",
        ),
        (
            "a dimension called parent",
            serde_json::json!({"dimensions": [{"id": "parent", "groups": [{"id": "g", "members": ["a"]}]}]}),
            "may not be called `parent`",
        ),
    ];
    for (what, memberships, message) in cases {
        let error = import(base(memberships)).expect_err(what);
        assert!(error.contains(message), "{what}: {error}");
    }
    // An unknown member id is not refused (ignored and counted).
    assert!(import(base(
        serde_json::json!({"dimensions": [{"id": "z", "groups": [{"id": "g",
        "members": ["a", "nobody"]}]}]})
    ))
    .is_ok());
    // The refusals of the options alone (ADR 0022 §5).
    let refused = |patch: serde_json::Value, message: &str| {
        let error = arqlet_runtime::options_from_patch(&patch).expect_err(message);
        assert!(error.contains(message), "{patch}: {error}");
    };
    refused(
        serde_json::json!({"groups": {"a": ["G"]}, "groupingPriority": [{"by": "z"}, {"by": "parent"}]}),
        "cannot be used together",
    );
    refused(
        serde_json::json!({"groupingPriority": [{"by": "z"}, {"by": "z"}]}),
        "twice",
    );
    refused(
        serde_json::json!({"groupingPriority": [{"by": "parent", "values": ["x"]}]}),
        "not to parent",
    );
    refused(
        serde_json::json!({"groupingPriority": [{"by": "a", "as": "lanes"}, {"by": "b", "as": "lanes"},
        {"by": "c", "as": "lanes"}]}),
        "two at most",
    );
    // `groups` with the default priority is today's view, and a priority without `groups` is fine.
    assert!(arqlet_runtime::options_from_patch(
        &serde_json::json!({"groups": {"a": ["G"]}, "groupingPriority": [{"by": "parent"}]})
    )
    .is_ok());

    // The refusals that need the graph: an undeclared dimension or value, at import.
    let graph: Graph = serde_json::from_value(base(serde_json::json!({"dimensions": [{"id": "z",
        "groups": [{"id": "g", "members": ["a"]}]}]})))
    .unwrap();
    let with = |priority: serde_json::Value| {
        let options =
            arqlet_runtime::options_from_patch(&serde_json::json!({"groupingPriority": priority}))
                .expect("the options alone are valid");
        import_graph_with(graph.clone(), 100, &options)
    };
    assert!(with(serde_json::json!([{"by": "team"}]))
        .unwrap_err()
        .contains("neither parent nor a declared dimension"));
    assert!(with(serde_json::json!([{"by": "z", "values": ["nope"]}]))
        .unwrap_err()
        .contains("declares no group"));
    assert!(with(serde_json::json!([{"by": "z", "values": ["g"]}, {"by": "parent"}])).is_ok());
}
