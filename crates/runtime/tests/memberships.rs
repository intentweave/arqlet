//! Test data for ADR 0022 (memberships across the hierarchy), which is proposed and not implemented. Two things are checked
//! here, so the fixtures stay usable while the ADR is still a proposal:
//!
//! 1. Every fixture is a valid graph **today**: `GraphWire` ignores fields it does not know, so the extra `memberships` and
//!    `groupingPriority` keys are dropped and the graph imports and lays out as it would without them. That keeps the
//!    fixtures from drifting away from the engine while the feature is being decided.
//! 2. The `memberships` block of every fixture follows the shape the ADR decides (§1 and §2), checked by a validator that
//!    reads the raw JSON. When the model learns the field, this validator is what `crates/model` has to implement; until
//!    then it keeps the test data honest.
//!
//! The per-fixture expectations at the bottom pin what each fixture is *for* (the case it carries), so a later edit cannot
//! quietly turn the Euler case into a satisfiable one.

use arqlet_model::{Graph, GroupingCriterion, LayoutOptions, Snapshot};
use arqlet_runtime::{configure_snapshot, import_graph, import_graph_with};
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
const ORG_CHART: &str =
    include_str!("../../../examples/fixtures/memberships/org-chart-projects.json");
const OVERLAPPING_BOXES: &str =
    include_str!("../../../examples/fixtures/memberships/overlapping-boxes.json");

fn fixtures() -> Vec<(&'static str, &'static str)> {
    vec![
        ("three-dimensions", THREE_DIMENSIONS),
        ("pools-and-lanes", POOLS_AND_LANES),
        ("non-contiguous", NON_CONTIGUOUS),
        ("crosses-containers", CROSSES_CONTAINERS),
        ("one-group-per-node", ONE_GROUP_PER_NODE),
        ("org-chart-projects", ORG_CHART),
        ("overlapping-boxes", OVERLAPPING_BOXES),
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
        let priority = raw
            .get("groupingPriority")
            .unwrap_or_else(|| panic!("{name}: every fixture says the view it is meant for"));
        read_priority(name, priority, &dimensions);
        assert!(
            raw.get("membershipViews").is_none(),
            "{name}: membershipViews was replaced by groupingPriority (M1)"
        );
    }
}

/// One criterion of a `groupingPriority` (ADR 0022 §2, revised 2026-11-22): `parent` or a dimension, with its value order
/// (the listed groups first, then the rest in declared order) and its rendering.
struct Criterion {
    by: String,
    values: Vec<String>,
    lanes: bool,
}

/// Reads and checks a `groupingPriority` against §2 and the refusals of §5: each criterion is `parent` or a declared
/// dimension and appears once, `values` only on dimensions and only declared groups, `as` is `containers` or `lanes`, at
/// most two criteria as lanes, and no dimension may be called `parent`.
fn read_priority(
    name: &str,
    priority: &serde_json::Value,
    dimensions: &[Dimension],
) -> Vec<Criterion> {
    assert!(
        dimensions.iter().all(|d| d.id != "parent"),
        "{name}: `parent` is reserved and may not name a dimension"
    );
    let entries = priority
        .as_array()
        .unwrap_or_else(|| panic!("{name}: groupingPriority must be an array"));
    assert!(
        !entries.is_empty(),
        "{name}: an empty priority has no owner for anything"
    );
    let mut criteria: Vec<Criterion> = Vec::new();
    for entry in entries {
        let by = entry["by"]
            .as_str()
            .unwrap_or_else(|| panic!("{name}: a criterion without `by`"))
            .to_string();
        assert!(
            !criteria.iter().any(|c| c.by == by),
            "{name}: the criterion {by} is listed twice"
        );
        let dimension = dimensions.iter().find(|d| d.id == by);
        assert!(
            by == "parent" || dimension.is_some(),
            "{name}: the criterion {by} is neither `parent` nor a declared dimension"
        );
        let values: Vec<String> = match entry.get("values") {
            None => Vec::new(),
            Some(v) => {
                let d = dimension.unwrap_or_else(|| {
                    panic!("{name}: `values` belong to a dimension, not to {by}")
                });
                v.as_array()
                    .unwrap_or_else(|| panic!("{name}: values of {by} must be an array"))
                    .iter()
                    .map(|g| {
                        let g = g.as_str().expect("a group id").to_string();
                        assert!(d.order.contains(&g), "{name}: {by} declares no group {g}");
                        g
                    })
                    .collect()
            }
        };
        let lanes = match entry.get("as").and_then(|a| a.as_str()) {
            None | Some("containers") => false,
            Some("lanes") => true,
            Some(other) => panic!("{name}: {by} is rendered as {other}, not containers or lanes"),
        };
        criteria.push(Criterion { by, values, lanes });
    }
    let lanes = criteria.iter().filter(|c| c.lanes).count();
    assert!(
        lanes <= 2,
        "{name}: {lanes} criteria as lanes; §2 allows two"
    );
    criteria
}

/// The ownership rule of §2 as an oracle: a node is owned by its group in the first criterion, by rank, that assigns it
/// one; `parent` assigns every node (its container, or the root); in a dimension, the value order picks among several
/// groups. `None` is a node no criterion assigns. M1's projection must agree with this on every fixture.
fn owner(
    node: &str,
    criteria: &[Criterion],
    dimensions: &[Dimension],
    parents: &BTreeMap<&str, Option<&str>>,
) -> Option<(String, String)> {
    for c in criteria {
        if c.by == "parent" {
            let container = parents[node].unwrap_or("<root>");
            return Some(("parent".into(), container.into()));
        }
        let d = dimensions.iter().find(|d| d.id == c.by).expect("validated");
        let groups = d.groups_of(node);
        if groups.is_empty() {
            continue;
        }
        let order = c
            .values
            .iter()
            .chain(d.order.iter().filter(|g| !c.values.contains(g)));
        let chosen = order
            .into_iter()
            .find(|g| groups.contains(&g.as_str()))
            .expect("one of the node's groups");
        return Some((c.by.clone(), chosen.clone()));
    }
    None
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
    // Two dimensions group the same way when they induce the same partition, which is a question about *pairs* of nodes:
    // do these two nodes share a group here exactly when they share one there? Comparing the group ids instead would never
    // find a difference, because no two dimensions reuse an id.
    let leaves: Vec<&String> = node_ids
        .iter()
        .filter(|n| parents[n.as_str()].is_some())
        .collect();
    let together = |d: &Dimension, x: &str, y: &str| {
        let gx: BTreeSet<&str> = d.groups_of(x).into_iter().collect();
        d.groups_of(y).iter().any(|g| gx.contains(g))
    };
    let differs = |da: &Dimension, db: &Dimension| {
        leaves.iter().enumerate().any(|(i, x)| {
            leaves[i + 1..]
                .iter()
                .any(|y| together(da, x, y) != together(db, x, y))
        })
    };
    for (a, b) in [("zone", "team"), ("zone", "layer"), ("team", "layer")] {
        let da = dimensions.iter().find(|d| d.id == a).unwrap();
        let db = dimensions.iter().find(|d| d.id == b).unwrap();
        assert!(
            differs(da, db),
            "the dimensions {a} and {b} induce the same partition, so they group the same way"
        );
    }
    // The comparison above is only worth making if it can come out the other way: a dimension against itself must be found
    // to group the same way. Without this, an assertion that merely cannot fail would look like a passing check.
    for d in &dimensions {
        assert!(
            !differs(d, d),
            "the comparison does not recognise {} as grouping like itself, so it proves nothing",
            d.id
        );
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

    // Several groups of one dimension. Held softly after the parent hierarchy, which this fixture declares, both hold the
    // node; as the first criterion the value order would choose one owner (§2, rule 2).
    assert_eq!(
        zone.groups_of("shared"),
        ["north", "south"],
        "shared is the node in two groups of one dimension"
    );
    let criteria = read_priority("one-group-per-node", &raw["groupingPriority"], &dimensions);
    assert_eq!(
        criteria.iter().map(|c| c.by.as_str()).collect::<Vec<_>>(),
        ["parent", "zone"],
        "the zone is held softly, after the parent hierarchy"
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

#[test]
fn the_org_chart_is_owned_by_project_first_with_the_value_priority_deciding() {
    let raw: serde_json::Value = serde_json::from_str(ORG_CHART).expect("json");
    let parents: BTreeMap<&str, Option<&str>> = raw["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| (n["id"].as_str().unwrap(), n["parentId"].as_str()))
        .collect();
    let node_ids: BTreeSet<String> = parents.keys().map(|k| k.to_string()).collect();
    let dimensions = read_memberships("org-chart-projects", &raw, &node_ids);
    let criteria = read_priority("org-chart-projects", &raw["groupingPriority"], &dimensions);
    let owned = |node: &str, criteria: &[Criterion]| owner(node, criteria, &dimensions, &parents);
    let pair = |a: &str, b: &str| Some((a.to_string(), b.to_string()));

    // The case of the revision: projects cut across departments, so they cannot be the reporting line's containers.
    let project = dimensions.iter().find(|d| d.id == "project").unwrap();
    let departments: BTreeSet<Option<&str>> = project.members["apollo"]
        .iter()
        .map(|m| parents[m.as_str()])
        .collect();
    assert!(departments.len() > 1, "apollo spans departments");

    // Dora is on two projects; the value priority names zeus first, so zeus owns her and apollo holds her softly.
    assert_eq!(project.groups_of("dora"), ["apollo", "zeus"]);
    assert_eq!(owned("dora", &criteria), pair("project", "zeus"));
    assert_eq!(owned("anna", &criteria), pair("project", "apollo"));
    assert_eq!(owned("ben", &criteria), pair("project", "hermes"));
    // Gus is on no project, so the next criterion, the reporting line, owns him.
    assert_eq!(owned("gus", &criteria), pair("parent", "ops"));

    // The value priority matters: with apollo first, apollo owns Dora.
    let mut apollo_first =
        read_priority("org-chart-projects", &raw["groupingPriority"], &dimensions);
    apollo_first[0].values = vec!["apollo".into(), "zeus".into()];
    assert_eq!(owned("dora", &apollo_first), pair("project", "apollo"));

    // Without `parent`, the next criterion that assigns Gus owns him: he is on no project, but at the Munich site.
    let without_parent: Vec<Criterion> =
        read_priority("org-chart-projects", &raw["groupingPriority"], &dimensions)
            .into_iter()
            .filter(|c| c.by != "parent")
            .collect();
    assert_eq!(owned("gus", &without_parent), pair("site", "munich"));
    assert_eq!(owned("dora", &without_parent), pair("project", "zeus"));

    // Only when no listed criterion assigns a node is it left without owner: placed at the root and reported (§2, rule 3).
    let projects_only: Vec<Criterion> =
        read_priority("org-chart-projects", &raw["groupingPriority"], &dimensions)
            .into_iter()
            .filter(|c| c.by == "project")
            .collect();
    assert_eq!(owned("gus", &projects_only), None);
    assert_eq!(owned("dora", &projects_only), pair("project", "zeus"));

    // The default priority, `parent` alone, is today's view: everybody in their department.
    let default = vec![Criterion {
        by: "parent".into(),
        values: Vec::new(),
        lanes: false,
    }];
    assert_eq!(owned("dora", &default), pair("parent", "eng"));
    assert_eq!(owned("gus", &default), pair("parent", "ops"));
}

#[test]
fn the_target_picture_shares_exactly_one_node_between_a_container_and_a_soft_group() {
    let raw: serde_json::Value = serde_json::from_str(OVERLAPPING_BOXES).expect("json");
    let parents: BTreeMap<&str, Option<&str>> = raw["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| (n["id"].as_str().unwrap(), n["parentId"].as_str()))
        .collect();
    let node_ids: BTreeSet<String> = parents.keys().map(|k| k.to_string()).collect();
    let dimensions = read_memberships("overlapping-boxes", &raw, &node_ids);
    let criteria = read_priority("overlapping-boxes", &raw["groupingPriority"], &dimensions);

    // The two groupings of the picture: the container A by containment, the group B by membership.
    let in_a: BTreeSet<&str> = parents
        .iter()
        .filter(|(_, p)| **p == Some("A"))
        .map(|(id, _)| *id)
        .collect();
    let team = dimensions.iter().find(|d| d.id == "team").unwrap();
    let in_b: BTreeSet<&str> = team.members["B"].iter().map(String::as_str).collect();
    let shared: Vec<&&str> = in_a.intersection(&in_b).collect();
    assert_eq!(
        shared,
        [&"s"],
        "exactly one node lies in both, which is the overlap of the picture"
    );
    assert!(
        in_b.iter().any(|m| !in_a.contains(m)),
        "B reaches outside A"
    );
    assert!(
        in_a.iter().any(|m| !in_b.contains(m)),
        "A holds nodes B does not"
    );

    // With the containment first, A owns s and B holds it softly: B can only be one box as a region across A's border.
    assert_eq!(
        owner("s", &criteria, &dimensions, &parents),
        Some(("parent".to_string(), "A".to_string()))
    );
    assert_eq!(
        owner("c", &criteria, &dimensions, &parents),
        Some(("parent".to_string(), "<root>".to_string()))
    );

    // Edges run across both borders: from B outside A into A, and inside each.
    let edges: Vec<(&str, &str)> = raw["edges"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| (e["source"].as_str().unwrap(), e["target"].as_str().unwrap()))
        .collect();
    assert!(edges
        .iter()
        .any(|(s, t)| !in_a.contains(s) && in_a.contains(t) && !in_b.contains(t)));
    assert!(edges
        .iter()
        .any(|(s, t)| in_b.contains(s) && in_b.contains(t)));
}

/// The container each node of the view sits in.
fn view_parents(snapshot: &Snapshot) -> BTreeMap<String, Option<String>> {
    snapshot
        .graph
        .nodes
        .iter()
        .map(|n| (n.id.clone(), n.parent_id.clone()))
        .collect()
}

/// The fixture's own priority as options.
fn priority_of(raw: &serde_json::Value) -> LayoutOptions {
    LayoutOptions {
        grouping_priority: serde_json::from_value::<Vec<GroupingCriterion>>(
            raw["groupingPriority"].clone(),
        )
        .expect("a valid priority"),
        ..LayoutOptions::default()
    }
}

#[test]
fn m1_projects_every_node_into_the_owner_the_oracle_names() {
    // ADR 0022, M1: the engine's projection must agree with the ownership rule of §2, node by node.
    let raw: serde_json::Value = serde_json::from_str(ORG_CHART).expect("json");
    let graph: Graph = serde_json::from_str(ORG_CHART).expect("a graph");
    let parents: BTreeMap<&str, Option<&str>> = raw["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| (n["id"].as_str().unwrap(), n["parentId"].as_str()))
        .collect();
    let node_ids: BTreeSet<String> = parents.keys().map(|k| k.to_string()).collect();
    let dimensions = read_memberships("org-chart-projects", &raw, &node_ids);
    let criteria = read_priority("org-chart-projects", &raw["groupingPriority"], &dimensions);

    let snapshot = import_graph_with(graph, 500, &priority_of(&raw)).expect("imports");
    let view = view_parents(&snapshot);
    for person in ["anna", "ben", "carl", "dora", "emil", "fay", "gus"] {
        let expected = match owner(person, &criteria, &dimensions, &parents) {
            Some((by, group)) if by == "parent" => (group != "<root>").then_some(group),
            Some((by, group)) => Some(format!("membership:{by}/{group}")),
            None => None,
        };
        assert_eq!(
            view[person], expected,
            "{person} sits where the oracle puts it"
        );
    }
    // Dora, on two projects, is in zeus; Gus, on none, stays in Operations, which therefore stays a box.
    assert_eq!(view["dora"].as_deref(), Some("membership:project/zeus"));
    assert_eq!(view["gus"].as_deref(), Some("ops"));
    assert!(view.contains_key("ops"));
    // Sales and Engineering lost every member to a project: not boxes of this view, but soft groups, drawn as hulls.
    assert!(!view.contains_key("sales") && !view.contains_key("eng"));
    // The project containers are labelled with their groups and hold what the oracle says.
    let zeus = snapshot
        .graph
        .nodes
        .iter()
        .find(|n| n.id == "membership:project/zeus")
        .expect("zeus is a container of the view");
    assert_eq!(zeus.label, "Zeus");
    // The canonical graph is untouched: the source keeps every department and every parentId.
    let source = snapshot.source_graph.as_deref().expect("a source graph");
    assert_eq!(
        source
            .nodes
            .iter()
            .find(|n| n.id == "dora")
            .unwrap()
            .parent_id
            .as_deref(),
        Some("eng")
    );
    // Every edge keeps its logical direction, the self-loop and the parallel pair included.
    let edges: BTreeSet<(String, String, String)> = snapshot
        .graph
        .edges
        .iter()
        .map(|e| {
            (
                e.id.clone(),
                e.source.clone(),
                e.target.clone().unwrap_or_default(),
            )
        })
        .collect();
    assert!(edges.contains(&("c8".into(), "dora".into(), "dora".into())));
    assert!(edges.contains(&("c1b".into(), "anna".into(), "carl".into())));
}

#[test]
fn m1_reports_what_the_projection_decided() {
    let raw: serde_json::Value = serde_json::from_str(ORG_CHART).expect("json");
    let graph: Graph = serde_json::from_str(ORG_CHART).expect("a graph");
    let snapshot = import_graph_with(graph, 500, &priority_of(&raw)).expect("imports");
    let scene = arqlet_scene::build_scene(&snapshot).expect("a scene");
    let entry = |by: &str| {
        scene
            .membership_report
            .iter()
            .find(|r| r.dimension == by)
            .unwrap_or_else(|| panic!("no report for {by}"))
    };
    let project = entry("project");
    assert_eq!(project.mode, "owner");
    assert_eq!(project.owned, 6, "six people are on a project");
    assert_eq!(project.multiple_owners, 1, "Dora is on two");
    assert_eq!(
        project.unassigned, 0,
        "the parent hierarchy assigns the rest"
    );
    assert_eq!(project.drawn, 0, "the owner draws boxes, not hulls");
    // The reporting line moved down: one hull per department whose members are not all in its own box.
    let parent = entry("parent");
    assert_eq!(parent.mode, "soft");
    let departments: BTreeSet<&str> = scene
        .membership_hulls
        .iter()
        .filter(|h| h.dimension == "parent")
        .map(|h| h.label.as_str())
        .collect();
    assert_eq!(
        departments,
        ["Engineering", "Operations", "Sales"].into_iter().collect()
    );
    assert_eq!(entry("site").mode, "soft");

    // Without `parent`, Gus is owned by his site; with projects alone he is reported as unassigned.
    let mut projects_only = priority_of(&raw);
    projects_only.grouping_priority.truncate(1);
    let graph: Graph = serde_json::from_str(ORG_CHART).unwrap();
    let alone = import_graph_with(graph, 500, &projects_only).expect("imports");
    assert_eq!(view_parents(&alone)["gus"], None, "at the root");
    let scene = arqlet_scene::build_scene(&alone).unwrap();
    // Gus and the three departments: no criterion assigns them.
    assert_eq!(scene.membership_report[0].unassigned, 4);
    assert!(scene.membership_report[0]
        .summary
        .contains("no criterion assigns"));
}

#[test]
fn m1_a_change_of_priority_projects_again_and_back() {
    // The kept view structure is keyed by every option that shapes it; a change of priority must not reuse it.
    let raw: serde_json::Value = serde_json::from_str(ORG_CHART).expect("json");
    let graph: Graph = serde_json::from_str(ORG_CHART).expect("a graph");
    let plain = import_graph(graph, 500).expect("imports");
    let before = view_parents(&plain);
    assert_eq!(before["dora"].as_deref(), Some("eng"));

    let patch = serde_json::json!({ "groupingPriority": raw["groupingPriority"] });
    let projected = configure_snapshot(&plain, plain.revision, plain.layout_version, &patch)
        .expect("configures");
    assert_eq!(
        view_parents(&projected)["dora"].as_deref(),
        Some("membership:project/zeus")
    );

    let back = configure_snapshot(
        &projected,
        projected.revision,
        projected.layout_version,
        &serde_json::json!({ "groupingPriority": [] }),
    )
    .expect("configures");
    assert_eq!(
        view_parents(&back),
        before,
        "the default priority is the source tree again"
    );
    assert!(
        !serde_json::to_string(&back.options)
            .unwrap()
            .contains("groupingPriority"),
        "and the default is omitted again"
    );
}

#[test]
fn m1_the_default_priority_is_the_view_of_today() {
    // Empty and `[parent]` are both the default: the view is the source tree, and no report or hull appears.
    for priority in [serde_json::json!([]), serde_json::json!([{"by": "parent"}])] {
        let graph: Graph = serde_json::from_str(ORG_CHART).expect("a graph");
        let options = LayoutOptions {
            grouping_priority: serde_json::from_value(priority.clone()).unwrap(),
            ..LayoutOptions::default()
        };
        let snapshot = import_graph_with(graph.clone(), 500, &options).expect("imports");
        let plain = import_graph(graph, 500).expect("imports");
        assert_eq!(view_parents(&snapshot), view_parents(&plain), "{priority}");
        let scene = arqlet_scene::build_scene(&snapshot).unwrap();
        assert!(scene.membership_hulls.is_empty() && scene.membership_report.is_empty());
    }
}

#[test]
fn m1_details_name_the_owner_and_the_soft_groups_per_criterion() {
    let raw: serde_json::Value = serde_json::from_str(ORG_CHART).expect("json");
    let graph: Graph = serde_json::from_str(ORG_CHART).expect("a graph");
    let snapshot = import_graph_with(graph.clone(), 500, &priority_of(&raw)).expect("imports");
    let answer = arqlet_runtime::details::details_answer_on(
        &snapshot,
        &serde_json::json!({"nodeId": "dora"}),
    )
    .expect("details");
    let node = answer.node.expect("a node");
    let groupings: Vec<(&str, &str, bool)> = node
        .groupings
        .iter()
        .map(|g| (g.by.as_str(), g.group.as_str(), g.owner))
        .collect();
    // In the order of the ranks: her projects (zeus owns her, apollo holds her softly), her department, her site.
    assert_eq!(
        groupings,
        [
            ("project", "apollo", false),
            ("project", "zeus", true),
            ("parent", "eng", false),
            ("site", "munich", false),
        ]
    );
    // The source containment stays apart: `parents` still names her department.
    assert_eq!(node.parents, ["eng"]);

    // Under the default priority the field is empty and left out of the answer.
    let plain = import_graph(graph, 500).expect("imports");
    let answer =
        arqlet_runtime::details::details_answer_on(&plain, &serde_json::json!({"nodeId": "dora"}))
            .expect("details");
    assert!(answer.node.as_ref().unwrap().groupings.is_empty());
    assert!(!serde_json::to_string(&answer)
        .unwrap()
        .contains("groupings"));
}

#[test]
fn m1_a_membership_patch_projects_again_where_the_dimension_owns_and_moves_nothing_where_it_is_soft(
) {
    let raw: serde_json::Value = serde_json::from_str(ORG_CHART).expect("json");
    let graph: Graph = serde_json::from_str(ORG_CHART).expect("a graph");
    // Gus joins hermes: a change of the project dimension.
    let patch: arqlet_model::GraphPatch = serde_json::from_value(serde_json::json!({
        "memberships": {"dimensions": [{"id": "project", "label": "Project", "groups": [
            {"id": "apollo", "label": "Apollo", "members": ["anna", "carl", "dora"]},
            {"id": "zeus", "label": "Zeus", "members": ["dora", "emil", "fay"]},
            {"id": "hermes", "label": "Hermes", "members": ["ben", "gus"]}
        ]}]}
    }))
    .expect("a patch");

    // Projects first: they own nodes, so the change moves Gus into hermes, a new projection.
    let owned = import_graph_with(graph.clone(), 500, &priority_of(&raw)).expect("imports");
    assert_eq!(view_parents(&owned)["gus"].as_deref(), Some("ops"));
    let after =
        arqlet_runtime::patch::patch_snapshot(&owned, owned.revision, owned.layout_version, &patch)
            .expect("patches");
    assert_eq!(
        view_parents(&after)["gus"].as_deref(),
        Some("membership:project/hermes")
    );
    assert!(
        !view_parents(&after).contains_key("ops"),
        "Operations lost its last member to a project"
    );

    // Projects after the containment: held softly, drawn only, so the same change moves nothing.
    let soft = LayoutOptions {
        grouping_priority: serde_json::from_value(
            serde_json::json!([{"by": "parent"}, {"by": "project"}]),
        )
        .unwrap(),
        ..LayoutOptions::default()
    };
    let held = import_graph_with(graph, 500, &soft).expect("imports");
    let after =
        arqlet_runtime::patch::patch_snapshot(&held, held.revision, held.layout_version, &patch)
            .expect("patches");
    assert_eq!(view_parents(&after), view_parents(&held));
    assert_eq!(after.boxes.len(), held.boxes.len());
    assert!(
        held.boxes
            .iter()
            .all(|(id, b)| after.boxes.get(id) == Some(b)),
        "a soft change moves no box"
    );
    assert_eq!(
        after.revision,
        held.revision + 1,
        "but it is a new revision"
    );
}

#[test]
fn m2b_no_hull_on_any_fixture_encloses_a_foreign_box_and_every_group_is_drawn_whole_or_in_pieces() {
    // The exit of M2b: under each fixture's own priority, every soft group is drawn — whole, in pieces or as marks — with
    // all the boxes that stand for its members, and no hull encloses a leaf that is not a member.
    for (name, text) in fixtures() {
        let raw: serde_json::Value = serde_json::from_str(text).expect("json");
        let graph: Graph = serde_json::from_str(text).expect("a graph");
        let snapshot = import_graph_with(graph, 500, &priority_of(&raw)).expect("imports");
        let scene = arqlet_scene::build_scene(&snapshot).expect("a scene");
        for hull in &scene.membership_hulls {
            assert!(
                hull.intruders.is_empty(),
                "{name}: {}/{} piece {} of {} encloses {:?}",
                hull.dimension,
                hull.group,
                hull.piece,
                hull.pieces,
                hull.intruders
            );
            assert!(hull.pieces <= arqlet_scene::MARKS_ABOVE_PIECES, "{name}");
        }
        for report in scene.membership_report.iter().filter(|r| r.mode == "soft") {
            assert_eq!(report.intruders, 0, "{name}: {}", report.dimension);
            assert!(
                report.intruders_whole >= report.intruders,
                "{name}: the whole-group count is the first slice's, never below the split one"
            );
            assert!(report.split + report.marked <= report.drawn, "{name}");
        }
        // A piece's boxes belong to its group: the same box is never in two pieces of one group.
        let mut seen: BTreeSet<(String, String, String)> = BTreeSet::new();
        for hull in &scene.membership_hulls {
            for b in &hull.boxes {
                assert!(
                    seen.insert((hull.dimension.clone(), hull.group.clone(), b.clone())),
                    "{name}: {b} in two pieces of {}/{}",
                    hull.dimension,
                    hull.group
                );
            }
        }
    }
}

/// A priority from JSON, for the `show` tests.
fn with_priority(priority: serde_json::Value) -> LayoutOptions {
    LayoutOptions {
        grouping_priority: serde_json::from_value(priority).expect("a priority"),
        ..LayoutOptions::default()
    }
}

#[test]
fn show_as_the_owner_makes_containers_of_the_shown_groups_only() {
    // One project among several: Zeus becomes a box; whoever is on another project only stays in their department.
    let graph: Graph = serde_json::from_str(ORG_CHART).expect("a graph");
    let options = with_priority(serde_json::json!([
        {"by": "project", "show": ["zeus"]}, {"by": "parent"}
    ]));
    let snapshot = import_graph_with(graph, 500, &options).expect("imports");
    let view = view_parents(&snapshot);
    for person in ["dora", "emil", "fay"] {
        assert_eq!(
            view[person].as_deref(),
            Some("membership:project/zeus"),
            "{person} is on Zeus"
        );
    }
    assert_eq!(
        view["anna"].as_deref(),
        Some("sales"),
        "Apollo is not shown"
    );
    assert_eq!(view["carl"].as_deref(), Some("eng"));
    assert!(
        !view
            .keys()
            .any(|id| id == "membership:project/apollo" || id == "membership:project/hermes"),
        "a group not shown is no container"
    );
    let scene = arqlet_scene::build_scene(&snapshot).expect("a scene");
    let project = &scene.membership_report[0];
    assert_eq!(
        (project.owned, project.groups, project.multiple_owners),
        (3, 1, 0)
    );
}

#[test]
fn show_as_a_soft_criterion_draws_the_shown_groups_only() {
    let graph: Graph = serde_json::from_str(THREE_DIMENSIONS).expect("a graph");
    let options = with_priority(serde_json::json!([
        {"by": "parent"}, {"by": "team", "show": ["payments"]}
    ]));
    let snapshot = import_graph_with(graph.clone(), 500, &options).expect("imports");
    let scene = arqlet_scene::build_scene(&snapshot).expect("a scene");
    let groups: BTreeSet<&str> = scene
        .membership_hulls
        .iter()
        .map(|h| h.group.as_str())
        .chain(scene.membership_marks.iter().map(|m| m.group.as_str()))
        .collect();
    assert_eq!(
        groups,
        ["payments"].into_iter().collect(),
        "one team, not three"
    );
    let drawn: BTreeSet<&str> = scene
        .membership_hulls
        .iter()
        .flat_map(|h| h.boxes.iter().map(String::as_str))
        .chain(scene.membership_marks.iter().map(|m| m.node.as_str()))
        .collect();
    assert_eq!(drawn, ["billing", "ledger", "orders"].into_iter().collect());
    let team = scene
        .membership_report
        .iter()
        .find(|r| r.dimension == "team")
        .expect("a team entry");
    assert_eq!((team.groups, team.drawn), (1, 1));
    // Drawing only: the boxes are the plain view's.
    let plain = import_graph_with(graph, 500, &LayoutOptions::default()).expect("imports");
    assert_eq!(snapshot.boxes, plain.boxes);
}

#[test]
fn show_is_refused_on_parent_and_for_an_undeclared_group() {
    let error = arqlet_runtime::options_from_patch(&serde_json::json!({
        "groupingPriority": [{"by": "parent", "show": ["eng"]}]
    }))
    .expect_err("show on parent");
    assert!(error.contains("not to parent"), "{error}");
    let graph: Graph = serde_json::from_str(ORG_CHART).expect("a graph");
    let options =
        with_priority(serde_json::json!([{"by": "project", "show": ["ares"]}, {"by": "parent"}]));
    let error = import_graph_with(graph, 500, &options).expect_err("an undeclared group");
    assert!(error.contains("declares no group"), "{error}");
}
