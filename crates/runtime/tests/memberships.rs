//! Test data for ADR 0022 (memberships across the hierarchy), which is proposed and not implemented. Two things are checked
//! here, so the fixtures stay usable while the ADR is still a proposal:
//!
//! 1. Every fixture is a valid graph **today**: `GraphWire` ignores fields it does not know, so the extra `memberships` and
//!    `membershipViews` keys are dropped and the graph imports and lays out as it would without them. That keeps the
//!    fixtures from drifting away from the engine while the feature is being decided.
//! 2. The `memberships` block of every fixture follows the shape the ADR decides (§1 and §2), checked by a validator that
//!    reads the raw JSON. When the model learns the field, this validator is what `crates/model` has to implement; until
//!    then it keeps the test data honest.
//!
//! The per-fixture expectations at the bottom pin what each fixture is *for* (the case it carries), so a later edit cannot
//! quietly turn the Euler case into a satisfiable one.

use arqlet_model::Graph;
use arqlet_runtime::import_graph;
use std::collections::{BTreeMap, BTreeSet};

const THREE_DIMENSIONS: &str =
    include_str!("../../../examples/fixtures/memberships/three-dimensions.json");
const POOLS_AND_LANES: &str =
    include_str!("../../../examples/fixtures/memberships/pools-and-lanes.json");
const NON_CONTIGUOUS: &str =
    include_str!("../../../examples/fixtures/memberships/non-contiguous.json");
const CROSSES_CONTAINERS: &str =
    include_str!("../../../examples/fixtures/memberships/crosses-containers.json");
const ONE_GROUP_PER_NODE: &str =
    include_str!("../../../examples/fixtures/memberships/one-group-per-node.json");

fn fixtures() -> Vec<(&'static str, &'static str)> {
    vec![
        ("three-dimensions", THREE_DIMENSIONS),
        ("pools-and-lanes", POOLS_AND_LANES),
        ("non-contiguous", NON_CONTIGUOUS),
        ("crosses-containers", CROSSES_CONTAINERS),
        ("one-group-per-node", ONE_GROUP_PER_NODE),
    ]
}

/// One dimension as the validator reads it: the declared group order, each group's parent and its members.
struct Dimension {
    id: String,
    order: Vec<String>,
    parent: BTreeMap<String, Option<String>>,
    members: BTreeMap<String, Vec<String>>,
}

impl Dimension {
    /// The groups of `node` in this dimension, in the declared order.
    fn groups_of(&self, node: &str) -> Vec<&str> {
        self.order
            .iter()
            .filter(|g| self.members[*g].iter().any(|m| m == node))
            .map(String::as_str)
            .collect()
    }

    /// `group` and its ancestors, outermost last.
    fn path(&self, group: &str) -> Vec<String> {
        let mut path = vec![group.to_string()];
        let mut at = group;
        while let Some(Some(p)) = self.parent.get(at) {
            path.push(p.clone());
            at = p.as_str();
        }
        path
    }
}

/// Reads and checks the `memberships` block against ADR 0022 §1, returning the dimensions in their declared order. Every
/// violation is a panic with the fixture's name, because a fixture that breaks the shape is a broken fixture, not a case.
fn read_memberships(
    name: &str,
    raw: &serde_json::Value,
    node_ids: &BTreeSet<String>,
) -> Vec<Dimension> {
    let block = raw
        .get("memberships")
        .unwrap_or_else(|| panic!("{name}: no memberships block"));
    let declared = block
        .get("dimensions")
        .and_then(|d| d.as_array())
        .unwrap_or_else(|| panic!("{name}: memberships.dimensions must be an array (ADR 0022 §1)"));
    assert!(!declared.is_empty(), "{name}: no dimension declared");

    let mut dimensions: Vec<Dimension> = Vec::new();
    for value in declared {
        let id = value
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or_else(|| panic!("{name}: a dimension without an id"))
            .to_string();
        assert!(
            !dimensions.iter().any(|d| d.id == id),
            "{name}: the dimension id {id} is declared twice"
        );
        let groups = value
            .get("groups")
            .and_then(|g| g.as_array())
            .unwrap_or_else(|| panic!("{name}: dimension {id} has no groups array"));
        assert!(!groups.is_empty(), "{name}: dimension {id} has no group");

        let mut order: Vec<String> = Vec::new();
        let mut parent = BTreeMap::new();
        let mut members = BTreeMap::new();
        for group in groups {
            let gid = group
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or_else(|| panic!("{name}: a group of {id} without an id"))
                .to_string();
            assert!(
                !order.contains(&gid),
                "{name}: the group id {id}/{gid} is declared twice"
            );
            let list: Vec<String> = group
                .get("members")
                .and_then(|m| m.as_array())
                .unwrap_or_else(|| panic!("{name}: group {id}/{gid} has no members array"))
                .iter()
                .map(|m| {
                    m.as_str()
                        .unwrap_or_else(|| panic!("{name}: a member of {id}/{gid} is not a string"))
                        .to_string()
                })
                .collect();
            assert!(
                !list.is_empty(),
                "{name}: group {id}/{gid} has no member; an empty group has nothing to draw"
            );
            let mut seen = BTreeSet::new();
            for m in &list {
                assert!(
                    seen.insert(m.clone()),
                    "{name}: group {id}/{gid} lists the member {m} twice"
                );
            }
            parent.insert(
                gid.clone(),
                group
                    .get("parent")
                    .and_then(|p| p.as_str())
                    .map(str::to_string),
            );
            members.insert(gid.clone(), list);
            order.push(gid);
        }

        // A parent names a group of the same dimension, and the parents form no cycle (ADR 0022 §1).
        for gid in &order {
            if let Some(Some(p)) = parent.get(gid) {
                assert!(
                    order.contains(p),
                    "{name}: group {id}/{gid} has the parent {p}, which {id} does not declare"
                );
            }
            let mut at = gid.as_str();
            let mut walked = 0usize;
            while let Some(Some(p)) = parent.get(at) {
                at = p.as_str();
                walked += 1;
                assert!(
                    walked <= order.len(),
                    "{name}: the parents of {id}/{gid} form a cycle"
                );
            }
        }

        dimensions.push(Dimension {
            id,
            order,
            parent,
            members,
        });
    }

    // Member ids the graph does not have are ignored and counted, not an error (ADR 0022 §1), so they are only reported.
    let unknown: Vec<String> = dimensions
        .iter()
        .flat_map(|d| {
            d.order.iter().flat_map(move |g| {
                d.members[g]
                    .iter()
                    .filter(|m| !node_ids.contains(*m))
                    .map(move |m| format!("{}/{g}:{m}", d.id))
            })
        })
        .collect();
    assert!(
        unknown.len() <= 1,
        "{name}: more unknown member ids than the fixtures mean to carry: {unknown:?}"
    );

    dimensions
}

/// Checks `membershipViews` against ADR 0022 §2: a known mode per declared dimension, at most one `hierarchy` and at most
/// two `lanes`, and for those modes a node's groups must form one path (nesting implies the ancestors).
fn check_views(
    name: &str,
    raw: &serde_json::Value,
    dimensions: &[Dimension],
    node_ids: &BTreeSet<String>,
) {
    let views = raw
        .get("membershipViews")
        .and_then(|v| v.as_object())
        .unwrap_or_else(|| panic!("{name}: membershipViews must be an object"));
    let mut hierarchies = 0;
    let mut lanes = 0;
    for (dimension, mode) in views {
        assert!(
            dimensions.iter().any(|d| &d.id == dimension),
            "{name}: membershipViews names {dimension}, which memberships does not declare"
        );
        let mode = mode
            .as_str()
            .unwrap_or_else(|| panic!("{name}: the mode of {dimension} is not a string"));
        match mode {
            "off" => continue,
            "hierarchy" => hierarchies += 1,
            "lanes" => lanes += 1,
            "overlay" => {}
            other => panic!("{name}: {dimension} has the unknown mode {other}"),
        }
        if mode == "hierarchy" || mode == "lanes" {
            let d = dimensions.iter().find(|d| &d.id == dimension).unwrap();
            for node in node_ids {
                let groups = d.groups_of(node);
                if groups.len() < 2 {
                    continue;
                }
                // Several groups are allowed only when they lie on one path, since a nested group implies its ancestors.
                let deepest = groups
                    .iter()
                    .max_by_key(|g| d.path(g).len())
                    .expect("a group");
                let path = d.path(deepest);
                assert!(
                    groups.iter().all(|g| path.iter().any(|p| p == g)),
                    "{name}: {node} is in several groups of {dimension} that are not one path ({groups:?}), \
                     which a {mode} dimension cannot show (membership-multiple-groups)"
                );
            }
        }
    }
    assert!(
        hierarchies <= 1,
        "{name}: {hierarchies} hierarchy dimensions; ADR 0022 §2 allows one"
    );
    assert!(
        lanes <= 2,
        "{name}: {lanes} lanes dimensions; ADR 0022 §2 allows two"
    );
}

#[test]
fn every_membership_fixture_follows_the_shape_the_adr_decides() {
    for (name, text) in fixtures() {
        let raw: serde_json::Value =
            serde_json::from_str(text).unwrap_or_else(|e| panic!("{name}: not valid JSON: {e}"));
        let node_ids: BTreeSet<String> = raw["nodes"]
            .as_array()
            .unwrap_or_else(|| panic!("{name}: nodes must be an array"))
            .iter()
            .map(|n| n["id"].as_str().expect("a node id").to_string())
            .collect();
        let dimensions = read_memberships(name, &raw, &node_ids);
        check_views(name, &raw, &dimensions, &node_ids);
    }
}

#[test]
fn the_fixtures_import_and_lay_out_with_the_extra_fields_ignored() {
    for (name, text) in fixtures() {
        let graph: Graph = serde_json::from_str(text)
            .unwrap_or_else(|e| panic!("{name}: does not parse as a graph: {e}"));
        assert!(!graph.nodes.is_empty(), "{name}: no node");
        let snapshot = import_graph(graph, 500).unwrap_or_else(|e| panic!("{name}: {e}"));
        // Every node of the view is placed, and no edge was dropped as unresolved.
        for node in &snapshot.graph.nodes {
            assert!(
                snapshot.boxes.contains_key(&node.id),
                "{name}: {} has no box",
                node.id
            );
        }
        assert_eq!(
            snapshot.omitted_unresolved_edges, 0,
            "{name}: the fixtures resolve every edge"
        );
        assert!(
            snapshot.geometry_issues.is_empty(),
            "{name}: geometry issues {:?}",
            snapshot.geometry_issues
        );
    }
}

#[test]
fn three_dimensions_cut_across_the_containers_and_across_each_other() {
    let raw: serde_json::Value = serde_json::from_str(THREE_DIMENSIONS).expect("json");
    let parents: BTreeMap<&str, Option<&str>> = raw["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| (n["id"].as_str().unwrap(), n["parentId"].as_str()))
        .collect();
    let node_ids: BTreeSet<String> = parents.keys().map(|k| k.to_string()).collect();
    let dimensions = read_memberships("three-dimensions", &raw, &node_ids);
    assert_eq!(
        dimensions.iter().map(|d| d.id.as_str()).collect::<Vec<_>>(),
        ["zone", "team", "layer"],
        "the declared dimension order is the order everything derived uses (ADR 0022 §1)"
    );

    // The point of the fixture: every dimension puts nodes of different containers into one group, so no dimension can be
    // the containment tree, and no two dimensions agree.
    for d in &dimensions {
        let mut crosses = false;
        for g in &d.order {
            let containers: BTreeSet<Option<&str>> =
                d.members[g].iter().map(|m| parents[m.as_str()]).collect();
            if containers.len() > 1 {
                crosses = true;
            }
        }
        assert!(
            crosses,
            "dimension {} lies inside the containers and would not need this feature",
            d.id
        );
    }
    for (a, b) in [("zone", "team"), ("zone", "layer"), ("team", "layer")] {
        let da = dimensions.iter().find(|d| d.id == a).unwrap();
        let db = dimensions.iter().find(|d| d.id == b).unwrap();
        let same = node_ids.iter().all(|n| {
            let ga: BTreeSet<&str> = da.groups_of(n).into_iter().collect();
            let gb: BTreeSet<&str> = db.groups_of(n).into_iter().collect();
            ga.len() == gb.len() && ga.iter().zip(gb.iter()).all(|(x, y)| x == y)
        });
        assert!(!same, "the dimensions {a} and {b} group the same way");
    }

    // The leaves are partitioned, so zone can be shown as lanes: one path per node, and nesting carries the ancestor.
    let zone = dimensions.iter().find(|d| d.id == "zone").unwrap();
    assert_eq!(
        zone.parent["restricted"].as_deref(),
        Some("internal"),
        "the fixture carries the nested-group case"
    );
    for node in &node_ids {
        if parents[node.as_str()].is_none() {
            continue; // a container, which the dimensions do not list
        }
        assert_eq!(
            zone.groups_of(node).len(),
            1,
            "{node} must be in exactly one zone group for the lanes mode"
        );
    }
}

#[test]
fn the_pools_and_lanes_fixture_is_a_full_grid() {
    let raw: serde_json::Value = serde_json::from_str(POOLS_AND_LANES).expect("json");
    let node_ids: BTreeSet<String> = raw["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["id"].as_str().unwrap().to_string())
        .collect();
    let dimensions = read_memberships("pools-and-lanes", &raw, &node_ids);
    let pool = dimensions.iter().find(|d| d.id == "pool").unwrap();
    let phase = dimensions.iter().find(|d| d.id == "phase").unwrap();

    // Two lanes dimensions give the grid (ADR 0022 §2). Every cell holds at least one node, so the fixture exercises the
    // grid and not a degenerate case, and every node sits in exactly one cell.
    for p in &pool.order {
        for q in &phase.order {
            let cell = pool.members[p]
                .iter()
                .filter(|n| phase.members[q].contains(n))
                .count();
            assert!(cell > 0, "the cell {p}×{q} is empty");
        }
    }
    for node in &node_ids {
        assert_eq!(pool.groups_of(node).len(), 1, "{node} is not in one pool");
        assert_eq!(phase.groups_of(node).len(), 1, "{node} is not in one phase");
    }
}

#[test]
fn the_euler_fixture_cannot_keep_every_group_contiguous_in_any_order() {
    let raw: serde_json::Value = serde_json::from_str(NON_CONTIGUOUS).expect("json");
    let node_ids: BTreeSet<String> = raw["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["id"].as_str().unwrap().to_string())
        .collect();
    let dimensions = read_memberships("non-contiguous", &raw, &node_ids);
    let d = &dimensions[0];
    assert_eq!(d.order.len(), 3, "the Euler case needs three groups");

    // Brute force over every order of the six nodes: no order makes all three groups one run. This is what makes the
    // fixture a proof and not an observation, and it is why non-contiguity must be reported rather than avoided (§4, §5).
    let nodes: Vec<&String> = node_ids.iter().collect();
    let mut order: Vec<&String> = nodes.clone();
    let mut best = usize::MAX;
    permutations(&mut order, 0, &mut |candidate: &[&String]| {
        let breaks: usize = d
            .order
            .iter()
            .map(|g| {
                let mut runs: usize = 0;
                let mut inside = false;
                for n in candidate {
                    let member = d.members[g].iter().any(|m| m == *n);
                    if member && !inside {
                        runs += 1;
                    }
                    inside = member;
                }
                runs.saturating_sub(1)
            })
            .sum();
        best = best.min(breaks);
    });
    assert!(
        best > 0,
        "an order keeps all three groups contiguous, so this is not the Euler case any more"
    );
    assert_eq!(
        best, 1,
        "the best order leaves exactly one contiguity break; the fixture pins that number"
    );
}

/// Calls `f` for every permutation of `items`.
fn permutations<'a>(items: &mut Vec<&'a String>, at: usize, f: &mut impl FnMut(&[&'a String])) {
    if at == items.len() {
        f(items);
        return;
    }
    for i in at..items.len() {
        items.swap(at, i);
        permutations(items, at + 1, f);
        items.swap(at, i);
    }
}

#[test]
fn the_crossing_fixture_has_a_group_across_a_border_and_a_group_with_an_intruder() {
    let raw: serde_json::Value = serde_json::from_str(CROSSES_CONTAINERS).expect("json");
    let parents: BTreeMap<&str, Option<&str>> = raw["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| (n["id"].as_str().unwrap(), n["parentId"].as_str()))
        .collect();
    let node_ids: BTreeSet<String> = parents.keys().map(|k| k.to_string()).collect();
    let dimensions = read_memberships("crosses-containers", &raw, &node_ids);
    let focus = &dimensions[0];

    let hot: BTreeSet<Option<&str>> = focus.members["hot-path"]
        .iter()
        .map(|m| parents[m.as_str()])
        .collect();
    assert!(
        hot.len() > 1,
        "hot-path must span two containers, so its hull crosses a border (ADR 0022 §3)"
    );

    // `local` holds l1 and l3 but not l2, which lies between them in the container, so the hull must report an intruder.
    let local = &focus.members["local"];
    let same_container: BTreeSet<Option<&str>> =
        local.iter().map(|m| parents[m.as_str()]).collect();
    assert_eq!(same_container.len(), 1, "local stays inside one container");
    let siblings: Vec<&str> = parents
        .iter()
        .filter(|(_, p)| **p == *same_container.iter().next().unwrap())
        .map(|(id, _)| *id)
        .collect();
    assert!(
        siblings.iter().any(|s| !local.contains(&s.to_string())),
        "local must leave a sibling out, so the hull has an intruder to count"
    );
}

#[test]
fn the_overlapping_fixture_shares_a_node_and_names_one_unknown_member() {
    let raw: serde_json::Value = serde_json::from_str(ONE_GROUP_PER_NODE).expect("json");
    let node_ids: BTreeSet<String> = raw["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["id"].as_str().unwrap().to_string())
        .collect();
    let dimensions = read_memberships("one-group-per-node", &raw, &node_ids);
    let zone = &dimensions[0];

    // Several groups of one dimension: allowed as an overlay, which this fixture declares, and the conflict
    // `membership-multiple-groups` as a hierarchy or lanes dimension (§3, §5).
    assert_eq!(
        zone.groups_of("shared"),
        ["north", "south"],
        "shared is the node in two groups of one dimension"
    );
    assert_eq!(
        raw["membershipViews"]["zone"].as_str(),
        Some("overlay"),
        "only an overlay can show this dimension"
    );
    assert!(
        zone.parent.values().all(|p| p.is_none()),
        "the two groups are siblings, not a path, which is what makes it a conflict for the other modes"
    );

    // An id the graph does not have is ignored and counted, not an error (§1).
    let unknown: Vec<&String> = zone
        .order
        .iter()
        .flat_map(|g| zone.members[g].iter())
        .filter(|m| !node_ids.contains(*m))
        .collect();
    assert_eq!(
        unknown.len(),
        1,
        "the fixture carries exactly one unknown member id"
    );
}
