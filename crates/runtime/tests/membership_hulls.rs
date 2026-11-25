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

/// The boxes a group is drawn with, over all its pieces and marks (ADR 0022 §3, M2b).
fn drawn_boxes(scene: &Scene, criterion: &str, group: &str) -> std::collections::BTreeSet<String> {
    scene
        .membership_hulls
        .iter()
        .filter(|h| h.dimension == criterion && h.group == group)
        .flat_map(|h| h.boxes.iter().cloned())
        .chain(
            scene
                .membership_marks
                .iter()
                .filter(|m| m.dimension == criterion && m.group == group)
                .map(|m| m.node.clone()),
        )
        .collect()
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

        // The shape is drawing only: the same nodes and routes, and every group drawn with the same boxes. The pieces may
        // differ: a rectangle encloses more than the polygon, so it can need more pieces to stay clean (M2b).
        assert_eq!(geometry(&plain), geometry(&boxes));
        assert_eq!(geometry(&plain), geometry(&hulls));
        let covered =
            |s: &Scene| -> BTreeMap<(String, String), std::collections::BTreeSet<String>> {
                let mut out: BTreeMap<_, std::collections::BTreeSet<String>> = BTreeMap::new();
                for h in &s.membership_hulls {
                    out.entry((h.dimension.clone(), h.group.clone()))
                        .or_default()
                        .extend(h.boxes.iter().cloned());
                }
                for m in &s.membership_marks {
                    out.entry((m.dimension.clone(), m.group.clone()))
                        .or_default()
                        .insert(m.node.clone());
                }
                out
            };
        assert_eq!(covered(&boxes), covered(&hulls));

        for r in &boxes.membership_hulls {
            // The default is a rectangle: four corners, every edge axis-parallel.
            assert_eq!(r.points.len(), 4, "{}: a rectangle", r.group);
            for i in 0..4 {
                let (a, b) = (&r.points[i], &r.points[(i + 1) % 4]);
                assert!(a.x == b.x || a.y == b.y, "{}: a slanted edge", r.group);
            }
        }
        // In either shape no piece encloses a foreign box.
        for h in boxes.membership_hulls.iter().chain(&hulls.membership_hulls) {
            assert!(
                h.intruders.is_empty(),
                "{}/{}: {:?}",
                h.dimension,
                h.group,
                h.intruders
            );
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
    // `internal` hulls must also enclose ledger and audit, which only `restricted` lists.
    let internal = drawn_boxes(&scene, "zone", "internal");
    for id in ["ledger", "audit"] {
        assert!(
            internal.contains(id),
            "{id} is a member of restricted, which is nested in internal: {internal:?}"
        );
    }
    assert_eq!(
        drawn_boxes(&scene, "zone", "restricted").len(),
        2,
        "the nested group itself holds only its own members"
    );

    // Hulls of one dimension carry the group labels, not the ids, where the fixture gives labels; every piece of a group
    // carries its group's label.
    let mut zone: Vec<&str> = scene
        .membership_hulls
        .iter()
        .filter(|h| h.dimension == "zone")
        .map(|h| h.label.as_str())
        .collect();
    zone.dedup();
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
fn a_group_whose_hull_would_swallow_a_foreign_node_is_split_and_every_piece_is_clean() {
    // ADR 0022 §3, M2b: one hull where it encloses nothing foreign, else one per owner, else pieces by distance.
    let g = graph(CROSSES_CONTAINERS);
    let (_, scene) = laid_out(CROSSES_CONTAINERS, &overlay_all(&g));
    let pieces = |group: &str| -> Vec<Vec<String>> {
        scene
            .membership_hulls
            .iter()
            .filter(|h| h.group == group)
            .map(|h| h.boxes.clone())
            .collect()
    };
    // `hot-path` spans `left` and `right`; its whole hull would enclose foreign nodes, so it is drawn per owner.
    assert_eq!(
        pieces("hot-path"),
        [
            vec!["l2".to_string()],
            vec!["r1".to_string(), "r2".to_string()]
        ],
        "one piece per container"
    );
    // `local` is l1 and l3 and skips l2, which lies between them: drawn whole, it would swallow l2, so it is two pieces.
    assert_eq!(
        pieces("local"),
        [vec!["l1".to_string()], vec!["l3".to_string()]]
    );
    for hull in &scene.membership_hulls {
        assert!(
            hull.intruders.is_empty(),
            "{}/{} piece {} of {} encloses {:?}",
            hull.dimension,
            hull.group,
            hull.piece,
            hull.pieces,
            hull.intruders
        );
        assert!(hull.piece < hull.pieces);
    }
    // The report says so, and keeps what the first slice would have drawn beside it.
    let focus = report(&scene, "focus");
    assert_eq!(focus.split, 2, "both groups are split");
    assert_eq!(focus.marked, 0);
    assert_eq!(focus.intruders, 0);
    assert!(
        focus.intruders_whole > 0,
        "drawn whole, l2 would be inside local's hull"
    );
    assert!(focus.summary.contains("2 split"), "{}", focus.summary);
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

#[test]
fn the_removed_membership_views_are_refused_wherever_they_arrive() {
    // ADR 0022, M1 removed `membershipViews`. A host or a stored snapshot that still carries it must learn so, not have it
    // dropped silently (LayoutOptions accepts unknown keys otherwise).
    let removed = serde_json::json!({"zone": "overlay"});
    let says = |error: String| {
        assert!(
            error.contains("membershipViews was removed") && error.contains("groupingPriority"),
            "the error names the replacement: {error}"
        );
    };

    // An import's options.
    says(
        arqlet_runtime::options_from_patch(&serde_json::json!({ "membershipViews": removed }))
            .expect_err("refused at import"),
    );

    // A configure request.
    let snapshot = import_graph_with(graph(THREE_DIMENSIONS), 500, &LayoutOptions::default())
        .expect("imports");
    says(
        arqlet_runtime::configure_snapshot(
            &snapshot,
            snapshot.revision,
            snapshot.layout_version,
            &serde_json::json!({ "membershipViews": removed }),
        )
        .expect_err("refused by configure"),
    );

    // A snapshot stored by an earlier version.
    let mut stored = serde_json::to_value(&snapshot).expect("serializes");
    stored["options"] = serde_json::json!({ "membershipViews": removed });
    let error = serde_json::from_value::<Snapshot>(stored).expect_err("refused when read");
    says(error.to_string());

    // And the field is never written.
    assert!(!serde_json::to_string(&LayoutOptions::default())
        .unwrap()
        .contains("membershipViews"));
}

#[test]
fn a_group_scattered_over_more_containers_than_the_threshold_is_marked_not_hulled() {
    // Four containers in a left-to-right flow, each with a member `a` and a foreign `x` after it: drawn whole, the group's
    // hull would swallow every `x` between the `a`s, so it falls into one piece per container — four, more than
    // MARKS_ABOVE_PIECES — and is marked member by member instead (ADR 0022 §3, M2b).
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    for i in 0..4 {
        nodes.push(
            serde_json::json!({"id": format!("c{i}"), "kind": "group", "label": format!("C{i}")}),
        );
        nodes.push(serde_json::json!({"id": format!("a{i}"), "kind": "node", "label": format!("a{i}"), "parentId": format!("c{i}")}));
        nodes.push(serde_json::json!({"id": format!("x{i}"), "kind": "node", "label": format!("x{i}"), "parentId": format!("c{i}")}));
        edges.push(serde_json::json!({"id": format!("ax{i}"), "kind": "flow", "source": format!("a{i}"), "target": format!("x{i}")}));
        if i > 0 {
            edges.push(serde_json::json!({"id": format!("xa{i}"), "kind": "flow", "source": format!("x{}", i - 1), "target": format!("a{i}")}));
        }
    }
    let graph: Graph = serde_json::from_value(serde_json::json!({
        "defaultDirection": "lr", "nodes": nodes, "edges": edges,
        "memberships": {"dimensions": [{"id": "team", "groups": [
            {"id": "spread", "label": "Spread", "members": ["a0", "a1", "a2", "a3"]},
            {"id": "pair", "label": "Pair", "members": ["a0", "x0"]}
        ]}]}
    }))
    .expect("a graph");
    let mut options = LayoutOptions::default();
    options.grouping_priority = vec![by("parent"), by("team")];
    let snapshot = import_graph_with(graph, 100, &options).expect("imports");
    let scene = build_scene(&snapshot).expect("a scene");

    let marked: Vec<&str> = scene
        .membership_marks
        .iter()
        .filter(|m| m.group == "spread")
        .map(|m| m.node.as_str())
        .collect();
    assert_eq!(marked, ["a0", "a1", "a2", "a3"], "every member marked");
    assert!(
        scene
            .membership_marks
            .iter()
            .all(|m| m.label == "Spread" && m.dimension == "team"),
        "a mark names its group"
    );
    assert!(
        !scene.membership_hulls.iter().any(|h| h.group == "spread"),
        "a marked group draws no polygon"
    );
    // The pair inside one container stays a hull: one group's marks do not turn the dimension into marks.
    assert!(scene.membership_hulls.iter().any(|h| h.group == "pair"));
    let team = report(&scene, "team");
    assert_eq!((team.marked, team.drawn), (1, 2));
    assert!(team.summary.contains("1 marked"), "{}", team.summary);
    assert!(
        arqlet_scene::MARKS_ABOVE_PIECES < 4,
        "the case stays above the threshold"
    );
}

#[test]
fn a_moved_down_parent_draws_no_hull_around_a_subtree_that_stayed_in_its_own_box() {
    // Found on a real export: with a dimension first that owns nothing below a directory, the parent hierarchy held its
    // nested containers softly and drew a hull around each directory's descendants — a hull tracing the very box the view
    // already draws, and overlaps with every nested one. A member owned by the parent hierarchy through a container at or
    // below the group is inside the group's box; such a group needs no hull.
    let graph: Graph = serde_json::from_value(serde_json::json!({
        "defaultDirection": "lr",
        "nodes": [
            {"id": "dir", "kind": "directory", "label": "dir"},
            {"id": "file", "kind": "file", "label": "file", "parentId": "dir"},
            {"id": "f", "kind": "symbol", "label": "f", "parentId": "file"},
            {"id": "g", "kind": "symbol", "label": "g", "parentId": "file"},
            {"id": "other", "kind": "file", "label": "other"},
            {"id": "h", "kind": "symbol", "label": "h", "parentId": "other"}
        ],
        "edges": [
            {"id": "fg", "kind": "calls", "source": "f", "target": "g"},
            {"id": "gh", "kind": "calls", "source": "g", "target": "h"}
        ],
        "memberships": {"dimensions": [{"id": "doc", "groups": [
            {"id": "notes", "label": "Notes", "members": ["h"]}
        ]}]}
    }))
    .expect("a graph");
    let mut options = LayoutOptions::default();
    options.grouping_priority = vec![by("doc"), by("parent")];
    let snapshot = import_graph_with(graph, 100, &options).expect("imports");
    let scene = build_scene(&snapshot).expect("a scene");
    // `dir` and `file` kept every member in their own boxes: no hull traces them.
    let parent_hulls: Vec<&str> = scene
        .membership_hulls
        .iter()
        .filter(|h| h.dimension == "parent")
        .map(|h| h.group.as_str())
        .collect();
    assert!(
        !parent_hulls.contains(&"dir") && !parent_hulls.contains(&"file"),
        "no parent hull traces a box the view draws: {parent_hulls:?}"
    );
    // `other` lost its only member to the doc's container, so it is no box of this view: that is exactly when the
    // moved-down hierarchy is drawn as a hull, around where its member went.
    assert_eq!(parent_hulls, ["other"]);
    assert!(!snapshot.graph.nodes.iter().any(|n| n.id == "other"));
    assert_eq!(report(&scene, "parent").overlapping_pairs, 0);
}

#[test]
fn the_children_of_a_member_that_is_a_container_are_not_foreign() {
    // Found on a real export: a document names an interface, which holds its properties. The interface's box encloses
    // them wherever its hull is drawn, so counting them as foreign made the hull unsplittable and reported intruders no
    // split can remove. A leaf inside one of the group's own boxes is part of what the group shows, not a stranger.
    let graph: Graph = serde_json::from_value(serde_json::json!({
        "defaultDirection": "lr",
        "nodes": [
            {"id": "file", "kind": "file", "label": "file"},
            {"id": "Shape", "kind": "symbol", "label": "Shape", "parentId": "file"},
            {"id": "width", "kind": "symbol", "label": "width", "parentId": "Shape"},
            {"id": "height", "kind": "symbol", "label": "height", "parentId": "Shape"},
            {"id": "area", "kind": "symbol", "label": "area", "parentId": "file"}
        ],
        "edges": [{"id": "uses", "kind": "calls", "source": "area", "target": "width"}],
        "memberships": {"dimensions": [{"id": "doc", "groups": [
            {"id": "guide", "label": "Guide", "members": ["Shape"]}
        ]}]}
    }))
    .expect("a graph");
    let mut options = LayoutOptions::default();
    options.grouping_priority = vec![by("parent"), by("doc")];
    let snapshot = import_graph_with(graph, 100, &options).expect("imports");
    let scene = build_scene(&snapshot).expect("a scene");
    let guide: Vec<&arqlet_scene::SceneHull> = scene
        .membership_hulls
        .iter()
        .filter(|h| h.group == "guide")
        .collect();
    assert_eq!(guide.len(), 1, "one hull, around the interface");
    assert_eq!(guide[0].boxes, ["Shape"]);
    assert!(
        guide[0].intruders.is_empty(),
        "its own properties are not intruders: {:?}",
        guide[0].intruders
    );
    assert_eq!(report(&scene, "doc").intruders_whole, 0);
}

#[test]
fn a_container_that_kept_its_box_draws_only_the_members_that_left() {
    // Found on a real export: a document taken first took 12 symbols out of a large file that kept its box, and the
    // file's soft group was marked whole — about 146 marks for 12 moves. The box shows the members inside it; only the
    // ones that left need drawing, joined to the box where that encloses nothing foreign.
    let mut nodes = vec![serde_json::json!({"id": "file", "kind": "file", "label": "file"})];
    let mut edges = Vec::new();
    for i in 0..6 {
        nodes.push(serde_json::json!({"id": format!("s{i}"), "kind": "symbol", "label": format!("s{i}"), "parentId": "file"}));
        if i > 0 {
            edges.push(serde_json::json!({"id": format!("e{i}"), "kind": "calls", "source": format!("s{}", i - 1), "target": format!("s{i}")}));
        }
    }
    let graph: Graph = serde_json::from_value(serde_json::json!({
        "defaultDirection": "lr", "nodes": nodes, "edges": edges,
        "memberships": {"dimensions": [{"id": "doc", "groups": [
            {"id": "guide", "label": "Guide", "members": ["s0"]}
        ]}]}
    }))
    .expect("a graph");
    let mut options = LayoutOptions::default();
    options.grouping_priority = vec![by("doc"), by("parent")];
    let snapshot = import_graph_with(graph, 100, &options).expect("imports");
    // s0 moved into the guide's box; the file kept s1 to s5 and its box.
    assert!(snapshot.graph.nodes.iter().any(|n| n.id == "file"));
    let scene = build_scene(&snapshot).expect("a scene");

    assert!(
        !scene.membership_marks.iter().any(|m| m.group == "file"),
        "the file is not marked member by member"
    );
    let drawn: std::collections::BTreeSet<&str> = scene
        .membership_hulls
        .iter()
        .filter(|h| h.group == "file")
        .flat_map(|h| h.boxes.iter().map(String::as_str))
        .collect();
    assert!(
        drawn.contains("s0"),
        "the member that left is drawn: {drawn:?}"
    );
    assert!(
        drawn.iter().all(|b| *b == "s0" || *b == "file"),
        "nothing inside the box is drawn again: {drawn:?}"
    );
    let parent = report(&scene, "parent");
    assert_eq!((parent.kept_box, parent.marked), (1, 0));
    for hull in &scene.membership_hulls {
        assert!(
            hull.intruders.is_empty(),
            "{}: {:?}",
            hull.group,
            hull.intruders
        );
    }
}

/// Two levels of containers and a document that takes one function out of the inner one, with `far` other functions
/// between them.
fn nested_with_a_moved_member(far: usize) -> Scene {
    let mut nodes = vec![
        serde_json::json!({"id": "src", "kind": "dir", "label": "src"}),
        serde_json::json!({"id": "file", "kind": "file", "label": "file", "parentId": "src"}),
        serde_json::json!({"id": "moved", "kind": "symbol", "label": "moved", "parentId": "file"}),
    ];
    for i in 0..far {
        nodes.push(serde_json::json!({"id": format!("s{i}"), "kind": "symbol", "label": format!("s{i}"), "parentId": "file"}));
    }
    let graph: Graph = serde_json::from_value(serde_json::json!({
        "defaultDirection": "lr", "nodes": nodes, "edges": [],
        "memberships": {"dimensions": [{"id": "doc", "groups": [
            {"id": "guide", "label": "Guide", "members": ["moved"]}
        ]}]}
    }))
    .expect("a graph");
    let mut options = LayoutOptions::default();
    options.grouping_priority = vec![by("doc"), by("parent")];
    build_scene(&import_graph_with(graph, 100, &options).expect("imports")).expect("a scene")
}

#[test]
fn a_moved_member_is_drawn_for_its_own_container_and_not_once_per_level() {
    // Seen in the browser: a file and its class drew the same hull, and the root drew one around everything.
    for far in [1, 40] {
        let scene = nested_with_a_moved_member(far);
        let groups: std::collections::BTreeSet<&str> = scene
            .membership_hulls
            .iter()
            .map(|h| h.group.as_str())
            .chain(scene.membership_marks.iter().map(|m| m.group.as_str()))
            .collect();
        assert_eq!(groups, ["file"].into(), "{far} functions stayed");
    }
}

#[test]
fn a_member_that_left_for_a_distant_place_is_marked_and_names_the_box_it_left() {
    let graph: Graph = serde_json::from_value(serde_json::json!({
        "defaultDirection": "lr",
        "nodes": (0..12).map(|i| serde_json::json!({"id": format!("c{i}"), "kind": "symbol", "label": "c"}))
            .chain([
                serde_json::json!({"id": "file", "kind": "file", "label": "file"}),
                serde_json::json!({"id": "stays", "kind": "symbol", "label": "stays", "parentId": "file"}),
                serde_json::json!({"id": "moved", "kind": "symbol", "label": "moved", "parentId": "file"}),
            ]).collect::<Vec<_>>(),
        "edges": (0..13).map(|i| {
            let name = |k: usize| match k { 0 => "stays".to_string(), 13 => "moved".into(), k => format!("c{}", k - 1) };
            serde_json::json!({"id": format!("e{i}"), "kind": "calls", "source": name(i), "target": name(i + 1)})
        }).collect::<Vec<_>>(),
        "memberships": {"dimensions": [{"id": "doc", "groups": [
            {"id": "guide", "label": "Guide", "members": ["moved"]}
        ]}]}
    }))
    .expect("a graph");
    let mut options = LayoutOptions::default();
    options.grouping_priority = vec![by("doc"), by("parent")];
    let scene =
        build_scene(&import_graph_with(graph, 100, &options).expect("imports")).expect("a scene");
    assert!(
        scene.membership_hulls.is_empty(),
        "no hull across the drawing"
    );
    let marks: Vec<_> = scene
        .membership_marks
        .iter()
        .map(|m| (m.group.as_str(), m.node.as_str(), m.own_box.as_deref()))
        .collect();
    assert_eq!(marks, [("file", "moved", Some("file"))]);
    let parent = report(&scene, "parent");
    assert_eq!((parent.kept_box, parent.marked), (1, 1));
}

#[test]
fn a_join_that_would_grow_out_of_proportion_is_marked_even_with_nothing_foreign_in_it() {
    // A tall file and the one function that left it, beside it: the joint rectangle would be a strip as tall as the file.
    // Nothing foreign is in the view at all, so only the compactness rule can decide.
    let mut nodes = vec![
        serde_json::json!({"id": "file", "kind": "file", "label": "file"}),
        serde_json::json!({"id": "moved", "kind": "symbol", "label": "moved", "parentId": "file"}),
    ];
    for i in 0..20 {
        nodes.push(serde_json::json!({"id": format!("s{i}"), "kind": "symbol", "label": format!("s{i}"), "parentId": "file"}));
    }
    let edges: Vec<_> = (0..20)
        .map(|i| serde_json::json!({"id": format!("e{i}"), "kind": "calls", "source": format!("s{i}"), "target": "moved"}))
        .collect();
    let graph: Graph = serde_json::from_value(serde_json::json!({
        "defaultDirection": "lr", "nodes": nodes, "edges": edges,
        "memberships": {"dimensions": [{"id": "doc", "groups": [
            {"id": "guide", "label": "Guide", "members": ["moved"]}
        ]}]}
    }))
    .expect("a graph");
    let mut options = LayoutOptions::default();
    options.grouping_priority = vec![by("doc"), by("parent")];
    let scene =
        build_scene(&import_graph_with(graph, 100, &options).expect("imports")).expect("a scene");
    assert!(scene.membership_hulls.is_empty());
    assert_eq!(scene.membership_marks.len(), 1);
}
