//! Communities in the structured view (option `communities`, provisional): virtual containers of the view, collapsed at
//! first and opened with `toggle`; the source graph stays as it was.
use arqlet_runtime::dispatch;
use serde_json::{json, Value};

/// A flat graph of four groups of 15 functions: a ring with chords in each group, one edge between neighbouring
/// groups; plus a cycle across two groups, a self-loop, a parallel edge and an unresolved edge.
fn flat() -> Value {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    for g in 0..4 {
        for k in 0..15 {
            let id = format!("g{g}n{k}");
            nodes.push(json!({"id": id, "kind": "function", "label": format!("f{g}_{k}"), "parentId": null}));
            edges.push(json!({"id": format!("r{g}_{k}"), "kind": "calls", "source": id, "target": format!("g{g}n{}", (k + 1) % 15)}));
            if k % 3 == 0 {
                edges.push(json!({"id": format!("c{g}_{k}"), "kind": "calls", "source": id, "target": format!("g{g}n{}", (k + 7) % 15)}));
            }
        }
        edges.push(json!({"id": format!("x{g}"), "kind": "calls", "source": format!("g{g}n4"), "target": format!("g{}n4", (g + 1) % 4)}));
    }
    edges.push(json!({"id": "back", "kind": "calls", "source": "g1n4", "target": "g0n4"}));
    edges.push(json!({"id": "self", "kind": "calls", "source": "g2n2", "target": "g2n2"}));
    edges.push(json!({"id": "twin", "kind": "calls", "source": "g3n0", "target": "g3n1"}));
    edges.push(json!({"id": "loose", "kind": "calls", "source": "g3n5", "target": null}));
    json!({"nodes": nodes, "edges": edges})
}

fn call(request: Value) -> Value {
    serde_json::from_str(&dispatch(&request.to_string())).unwrap()
}

fn ok(response: Value) -> Value {
    assert_eq!(response["ok"], json!(true), "{}", response["error"]);
    response["snapshot"].clone()
}

fn ids(snapshot: &Value, kind: &str) -> Vec<String> {
    snapshot["graph"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|n| n["kind"] == kind)
        .map(|n| n["id"].as_str().unwrap().to_string())
        .collect()
}

fn change(snapshot: &Value, request: Value) -> Value {
    let mut request = request;
    request["snapshot"] = snapshot.clone();
    request["expectedRevision"] = snapshot["revision"].clone();
    request["expectedLayoutVersion"] = snapshot["layoutVersion"].clone();
    call(request)
}

fn import() -> Value {
    ok(call(
        json!({"op": "import", "graph": flat(), "limit": 100, "options": {"communities": 30}}),
    ))
}

#[test]
fn an_import_shows_the_communities_collapsed_and_keeps_the_source_graph() {
    let snapshot = import();
    let communities = ids(&snapshot, "community");
    assert_eq!(communities.len(), 4, "{communities:?}");
    assert!(
        ids(&snapshot, "function").is_empty(),
        "every function is inside a collapsed community"
    );
    // Every community is collapsed, the nested ones too (Louvain may find one inside a group).
    let collapsed: Vec<String> = serde_json::from_value(snapshot["collapsedIds"].clone()).unwrap();
    assert!(communities.iter().all(|c| collapsed.contains(c)));
    assert!(collapsed.iter().all(|c| c.starts_with("community:")));
    assert_eq!(snapshot["geometryIssues"], json!([]));
    // The source graph has no community: the view derives them.
    let source = snapshot["sourceGraph"]["nodes"].as_array().unwrap();
    assert_eq!(source.len(), 60);
    assert!(source
        .iter()
        .all(|n| n["kind"] == "function" && n["parentId"].is_null()));
    // The edges between communities run between communities.
    let between: Vec<(String, String)> = snapshot["graph"]["edges"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                e["source"].as_str().unwrap().into(),
                e["target"].as_str().unwrap().into(),
            )
        })
        .collect();
    assert!(between
        .iter()
        .all(|(s, t)| communities.contains(s) && communities.contains(t)));
    // Both directions of a pair share one edge (its weights tell them apart; see the test below).
    assert!(
        between
            .iter()
            .all(|(s, t)| !between.contains(&(t.clone(), s.clone()))),
        "one edge per pair of communities"
    );
}

#[test]
fn a_community_opens_and_closes_in_place_and_refuses_a_stale_request() {
    let snapshot = import();
    let community = ids(&snapshot, "community")[1].clone();
    let open = ok(change(
        &snapshot,
        json!({"op": "toggle", "nodeId": community}),
    ));
    let inside: Vec<&Value> = open["graph"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|n| n["parentId"] == json!(community))
        .collect();
    assert!(inside.len() >= 2, "{inside:?}");
    assert_eq!(open["geometryIssues"], json!([]));
    // One group per community: its functions shown, a nested community of the same group collapsed.
    let functions: Vec<&str> = inside
        .iter()
        .filter(|n| n["kind"] == "function")
        .map(|n| n["id"].as_str().unwrap())
        .collect();
    assert!(!functions.is_empty());
    let group = &functions[0][..2];
    assert!(
        functions.iter().all(|id| id.starts_with(group)),
        "{functions:?}"
    );
    // The others did not move.
    for other in ids(&snapshot, "community")
        .iter()
        .filter(|c| **c != community)
    {
        assert!(open["boxes"][other].is_object());
    }
    let stale = change(&open, json!({"op": "toggle", "nodeId": community}));
    assert_eq!(ok(stale)["collapsedIds"], snapshot["collapsedIds"]);
    let refused = call(
        json!({"op": "toggle", "snapshot": open, "nodeId": community,
        "expectedRevision": snapshot["revision"], "expectedLayoutVersion": snapshot["layoutVersion"]}),
    );
    assert_eq!(
        refused["ok"],
        json!(false),
        "an answer to an older layout version is refused"
    );
}

#[test]
fn every_community_open_shows_all_edges_including_cycle_self_loop_and_parallel_edges() {
    let mut snapshot = import();
    for community in ids(&snapshot, "community") {
        snapshot = ok(change(
            &snapshot,
            json!({"op": "toggle", "nodeId": community}),
        ));
    }
    let edges: Vec<String> = snapshot["graph"]["edges"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["id"].as_str().unwrap().to_string())
        .collect();
    for id in ["back", "x0", "self", "twin", "r3_0"] {
        assert!(edges.contains(&id.to_string()), "{id} missing in {edges:?}");
    }
    assert!(
        !edges.contains(&"loose".to_string()),
        "unresolved edges are excluded"
    );
    assert_eq!(snapshot["omittedUnresolvedEdges"], json!(1));
    assert_eq!(snapshot["geometryIssues"], json!([]));
    let back = snapshot["graph"]["edges"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == "back")
        .unwrap();
    assert_eq!(
        (back["source"].as_str(), back["target"].as_str()),
        (Some("g1n4"), Some("g0n4"))
    );
}

#[test]
fn edits_keep_the_source_graph_free_of_communities() {
    let snapshot = import();
    let community = ids(&snapshot, "community")[0].clone();
    let open = ok(change(
        &snapshot,
        json!({"op": "toggle", "nodeId": community}),
    ));
    let member = open["graph"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["parentId"] == json!(community))
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let added = ok(change(
        &open,
        json!({"op": "add", "sourceId": member, "label": "new"}),
    ));
    let source = added["sourceGraph"]["nodes"].as_array().unwrap();
    let new = source.iter().find(|n| n["label"] == "new").unwrap();
    assert!(
        new["parentId"].is_null(),
        "the new node's parent is the source graph's, not a community"
    );
    assert!(source.iter().all(|n| n["kind"] != "community"));
    assert_eq!(added["geometryIssues"], json!([]));
    let turned = ok(change(
        &added,
        json!({"op": "direction", "nodeId": null, "direction": "tb"}),
    ));
    assert!(turned["sourceGraph"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .all(|n| n["kind"] != "community"));
    assert!(
        !ids(&turned, "community").is_empty(),
        "the view still has its communities"
    );
    assert_eq!(turned["geometryIssues"], json!([]));
}

#[test]
fn without_the_option_nothing_changes() {
    let plain = ok(call(json!({"op": "import", "graph": flat(), "limit": 100})));
    assert!(ids(&plain, "community").is_empty());
    assert_eq!(plain["collapsedIds"], json!([]));
    assert!(
        plain["options"].get("communities").is_none(),
        "the default is omitted"
    );
}

/// Both directions between two collapsed communities share one route; its weights count every edge it stands for, so
/// that the scene accounts for each edge between communities exactly once and keeps its direction.
#[test]
fn routes_between_communities_carry_their_edges_by_direction() {
    let snapshot = import();
    let scene = call(json!({"op": "scene", "snapshot": snapshot}))["scene"].clone();
    let routes = scene["routes"].as_array().unwrap();
    let mut counted = 0;
    let mut pairs = std::collections::BTreeSet::new();
    for route in routes {
        let back = route["backWeight"]
            .as_u64()
            .expect("every route here stands for collapsed content");
        counted += route["weight"].as_u64().unwrap() + back;
        let (s, t) = (
            route["source"].as_str().unwrap(),
            route["target"].as_str().unwrap(),
        );
        assert!(
            pairs.insert(if s < t { (s, t) } else { (t, s) }),
            "one route per pair: {s} {t}"
        );
    }
    // Edges of the source graph whose ends lie in different communities (the unresolved one excluded).
    let owner: std::collections::BTreeMap<String, String> = {
        let mut owner = std::collections::BTreeMap::new();
        // Open every community, nested ones too, until nothing is collapsed.
        let mut open = snapshot.clone();
        while let Some(next) = open["collapsedIds"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c.as_str().unwrap().to_string())
            .find(|c| {
                open["graph"]["nodes"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|n| n["id"] == c.as_str())
            })
        {
            open = ok(change(&open, json!({"op": "toggle", "nodeId": next})));
        }
        for n in open["graph"]["nodes"].as_array().unwrap() {
            if n["kind"] == "function" {
                // The outermost community above the function.
                let mut at = n["parentId"].as_str().map(String::from);
                let mut top = None;
                while let Some(p) = at {
                    top = Some(p.clone());
                    at = open["graph"]["nodes"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|m| m["id"] == p.as_str())
                        .and_then(|m| m["parentId"].as_str().map(String::from));
                }
                owner.insert(n["id"].as_str().unwrap().to_string(), top.unwrap());
            }
        }
        owner
    };
    let between = flat()["edges"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["target"].is_string())
        .filter(|e| owner[e["source"].as_str().unwrap()] != owner[e["target"].as_str().unwrap()])
        .count() as u64;
    assert_eq!(counted, between);
    assert!(
        routes.iter().any(|r| r["backWeight"].as_u64().unwrap() > 0),
        "the cycle 0 ⇄ 1 runs both ways on one route"
    );
}

/// Groups the domain gives: groups 0 and 1 are "core" (files a and b), group 2 is "io", group 3 has no path.
fn domain_groups() -> Value {
    let mut groups = serde_json::Map::new();
    for g in 0..3 {
        for k in 0..15 {
            let path = match g {
                0 => json!(["core", "a"]),
                1 => json!(["core", "b"]),
                _ => json!(["io"]),
            };
            groups.insert(format!("g{g}n{k}"), path);
        }
    }
    Value::Object(groups)
}

#[test]
fn given_groups_replace_louvain_where_the_domain_says_and_open_level_by_level() {
    let snapshot = ok(call(json!({"op": "import", "graph": flat(), "limit": 100,
        "options": {"groups": domain_groups()}})));
    let groups = ids(&snapshot, "group");
    assert_eq!(groups, ["group:#:core", "group:#:io"]);
    assert_eq!(
        ids(&snapshot, "function").len(),
        15,
        "the functions without a path stay visible"
    );
    assert!(
        ids(&snapshot, "community").is_empty(),
        "no Louvain without `communities`"
    );
    assert_eq!(snapshot["geometryIssues"], json!([]));
    assert!(snapshot["sourceGraph"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .all(|n| n["kind"] == "function"));
    let label = |s: &Value, id: &str| {
        s["graph"]["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["id"] == id)
            .unwrap()["label"]
            .clone()
    };
    assert_eq!(label(&snapshot, "group:#:core"), json!("core"));
    // Opening "core" shows its two files, collapsed; opening "a" shows its 15 functions.
    let core = ok(change(
        &snapshot,
        json!({"op": "toggle", "nodeId": "group:#:core"}),
    ));
    assert_eq!(
        ids(&core, "group"),
        [
            "group:#:core",
            "group:#:core/a",
            "group:#:core/b",
            "group:#:io"
        ]
    );
    let a = ok(change(
        &core,
        json!({"op": "toggle", "nodeId": "group:#:core/a"}),
    ));
    let inside = a["graph"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|n| n["parentId"] == "group:#:core/a")
        .count();
    assert_eq!(inside, 15);
    assert_eq!(a["geometryIssues"], json!([]));
}

#[test]
fn louvain_divides_what_the_given_groups_leave_large() {
    // One domain group "all" holds every function: 60 children, four dense parts. Louvain divides it; the group stays.
    let mut groups = serde_json::Map::new();
    for g in 0..4 {
        for k in 0..15 {
            groups.insert(format!("g{g}n{k}"), json!(["all"]));
        }
    }
    let snapshot = ok(call(json!({"op": "import", "graph": flat(), "limit": 100,
        "options": {"groups": groups, "communities": 30}})));
    assert_eq!(ids(&snapshot, "group"), ["group:#:all"]);
    let open = ok(change(
        &snapshot,
        json!({"op": "toggle", "nodeId": "group:#:all"}),
    ));
    let communities: Vec<&Value> = open["graph"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|n| n["kind"] == "community")
        .collect();
    assert_eq!(communities.len(), 4, "{communities:?}");
    assert!(communities.iter().all(|c| c["parentId"] == "group:#:all"));
    assert_eq!(open["geometryIssues"], json!([]));
}

/// Two directories of three files with four functions each, calls inside and across.
fn nested() -> Value {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    for d in 0..2 {
        nodes.push(json!({"id": format!("d{d}"), "kind": "directory", "label": format!("d{d}"), "parentId": null}));
        for f in 0..3 {
            let file = format!("d{d}/f{f}");
            nodes.push(
                json!({"id": file, "kind": "file", "label": file, "parentId": format!("d{d}")}),
            );
            for k in 0..4 {
                nodes.push(json!({"id": format!("{file}/s{k}"), "kind": "function", "label": format!("s{k}"), "parentId": file}));
                edges.push(json!({"id": format!("{file}/e{k}"), "kind": "calls", "source": format!("{file}/s{k}"), "target": format!("{file}/s{}", (k + 1) % 4)}));
            }
            edges.push(json!({"id": format!("x{d}{f}"), "kind": "calls", "source": format!("{file}/s0"), "target": format!("d{}/f{}/s1", 1 - d, (f + 1) % 3)}));
        }
    }
    json!({"nodes": nodes, "edges": edges})
}

#[test]
fn a_level_of_detail_shows_containers_collapsed_by_depth_or_kind() {
    let all = ok(call(
        json!({"op": "import", "graph": nested(), "limit": 100, "options": {"levelOfDetail": {"depth": 0}}}),
    ));
    let visible: Vec<&str> = all["graph"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["id"].as_str().unwrap())
        .collect();
    assert_eq!(visible, ["d0", "d1"], "depth 0: everything collapsed");
    // The calls between the two directories: one route for both directions, with its weights.
    let scene = call(json!({"op": "scene", "snapshot": all}))["scene"].clone();
    let routes = scene["routes"].as_array().unwrap();
    assert_eq!(routes.len(), 1);
    assert_eq!(
        routes[0]["weight"].as_u64().unwrap() + routes[0]["backWeight"].as_u64().unwrap(),
        6
    );
    let files = ok(call(
        json!({"op": "import", "graph": nested(), "limit": 100, "options": {"levelOfDetail": {"kinds": ["file"]}}}),
    ));
    let kinds: Vec<&str> = files["graph"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds.iter().filter(|k| **k == "file").count(), 6);
    assert_eq!(kinds.iter().filter(|k| **k == "function").count(), 0);
    let one = ok(call(
        json!({"op": "import", "graph": nested(), "limit": 100, "options": {"levelOfDetail": {"depth": 1}}}),
    ));
    assert_eq!(
        ids(&one, "file").len(),
        6,
        "depth 1: the directories open, the files collapsed"
    );
    assert_eq!(one["geometryIssues"], json!([]));
    // Opening a file shows its functions; the others stay collapsed.
    let open = ok(change(&one, json!({"op": "toggle", "nodeId": "d0/f1"})));
    assert_eq!(ids(&open, "function").len(), 4);
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_str().unwrap().to_string())
        .collect()
}

/// Path cut-out, steps 1 and 2: `trace` shows which collapsed boxes and routes carry the path from g0n2 to g1n9 (through
/// x0: g0n4 → g1n4); `reveal` opens exactly the communities that hold the path's nodes.
#[test]
fn a_trace_is_shown_on_collapsed_communities_and_revealed_by_opening_only_them() {
    let snapshot = import();
    let traced = call(json!({"op": "trace", "snapshot": snapshot, "from": "g0n2", "to": "g1n9"}));
    assert_eq!(traced["ok"], json!(true), "{}", traced["error"]);
    assert_eq!(traced["found"], json!(true));
    assert_eq!(traced["reversed"], json!(false));
    let nodes = strings(&traced["nodes"]);
    assert!(nodes.contains(&"g0n4".to_string()) && nodes.contains(&"g1n4".to_string()));
    assert_eq!(
        traced["length"],
        json!(nodes.len() - 1),
        "one shortest path here"
    );
    // In the collapsed view the path runs through communities and the routes between them.
    let visible = strings(&traced["visible"]["nodes"]);
    assert!(
        visible.iter().all(|n| n.starts_with("community:")),
        "{visible:?}"
    );
    assert!(visible.len() >= 2);
    let routes = strings(&traced["visible"]["routes"]);
    assert!(!routes.is_empty());
    let drawn: Vec<String> = snapshot["graph"]["edges"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["id"].as_str().unwrap().to_string())
        .collect();
    assert!(
        routes.iter().all(|r| drawn.contains(r)),
        "only drawn routes"
    );
    for n in &visible {
        assert_eq!(traced["distance"][n], json!(0));
    }
    // Reveal the path: exactly the communities above its nodes open.
    let revealed = ok(change(&snapshot, json!({"op": "reveal", "nodeIds": nodes})));
    let shown: Vec<String> = revealed["graph"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["id"].as_str().unwrap().to_string())
        .collect();
    assert!(
        nodes.iter().all(|n| shown.contains(n)),
        "every path node is visible"
    );
    let still: Vec<String> = serde_json::from_value(revealed["collapsedIds"].clone()).unwrap();
    assert!(!still.is_empty(), "communities off the path stay collapsed");
    assert_eq!(revealed["geometryIssues"], json!([]));
    // Traced again, the path is its own nodes and edges.
    let again = call(json!({"op": "trace", "snapshot": revealed, "from": "g0n2", "to": "g1n9"}));
    let mut visible = strings(&again["visible"]["nodes"]);
    visible.sort();
    let mut expected = nodes.clone();
    expected.sort();
    assert_eq!(visible, expected);
    let edges = strings(&again["edges"]);
    assert!(edges.contains(&"x0".to_string()));
    assert_eq!(strings(&again["visible"]["routes"]).len(), edges.len());
    // Revealing what is visible changes nothing.
    let same = ok(change(
        &revealed,
        json!({"op": "reveal", "nodeIds": ["g0n2"]}),
    ));
    assert_eq!(same["layoutVersion"], revealed["layoutVersion"]);
    // A stale request is refused.
    let stale = call(
        json!({"op": "reveal", "snapshot": revealed, "nodeIds": ["g3n1"],
        "expectedRevision": snapshot["revision"], "expectedLayoutVersion": snapshot["layoutVersion"]}),
    );
    assert_eq!(stale["ok"], json!(false));
}

#[test]
fn a_trace_against_the_edges_is_marked_reversed_and_unknown_nodes_are_errors() {
    let snapshot = import();
    let back = call(
        json!({"op": "trace", "snapshot": snapshot, "from": "g1n9", "to": "g0n2", "mode": "shortest"}),
    );
    assert_eq!(back["found"], json!(true));
    // g1 → g0 exists too (back: g1n4 → g0n4), so this one runs forward.
    assert_eq!(back["reversed"], json!(false));
    let missing = call(json!({"op": "trace", "snapshot": snapshot, "from": "nope", "to": "g0n2"}));
    assert_eq!(missing["ok"], json!(false));
    let bad = call(
        json!({"op": "trace", "snapshot": snapshot, "from": "g0n2", "to": "g1n9", "mode": "sideways"}),
    );
    assert_eq!(bad["ok"], json!(false));
    let all = call(
        json!({"op": "trace", "snapshot": snapshot, "from": "g0n2", "to": "g1n9", "mode": "all"}),
    );
    assert!(
        strings(&all["nodes"]).len()
            >= strings(
                &call(json!({"op": "trace", "snapshot": snapshot, "from": "g0n2", "to": "g1n9"}))
                    ["nodes"]
            )
            .len()
    );
}

fn visible_ids(s: &Value) -> Vec<String> {
    s["graph"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["id"].as_str().unwrap().to_string())
        .collect()
}

/// Partial reveal: a group opened for a node shows that node and gathers its siblings in a collapsed rest; the rest opens
/// to show them all; a node taken from the rest leaves it; `partial: false` opens whole.
#[test]
fn a_partial_reveal_shows_only_the_nodes_asked_for_and_a_rest() {
    let snapshot = ok(call(json!({"op": "import", "graph": flat(), "limit": 100,
        "options": {"groups": domain_groups()}})));
    let one = ok(change(
        &snapshot,
        json!({"op": "reveal", "nodeIds": ["g0n3"]}),
    ));
    let shown = visible_ids(&one);
    assert!(shown.contains(&"g0n3".to_string()));
    assert!(
        !shown.contains(&"g0n4".to_string()),
        "its siblings are in the rest"
    );
    // core → a → g0n3: "core" shows only "a" (b in its rest), "a" only g0n3 (14 in its rest).
    assert!(shown.contains(&"rest:group:#:core".to_string()));
    let rest = one["graph"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == "rest:group:#:core/a")
        .unwrap();
    assert_eq!(rest["label"], json!("+14 more"));
    assert_eq!(rest["kind"], json!("rest"));
    assert_eq!(rest["parentId"], json!("group:#:core/a"));
    assert!(one["sourceGraph"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .all(|n| n["kind"] == "function"));
    assert_eq!(one["geometryIssues"], json!([]));
    // A second node from the same file leaves the rest.
    let two = ok(change(&one, json!({"op": "reveal", "nodeIds": ["g0n9"]})));
    assert!(visible_ids(&two).contains(&"g0n9".to_string()));
    let rest = two["graph"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == "rest:group:#:core/a")
        .unwrap();
    assert_eq!(rest["label"], json!("+13 more"));
    // Opening the rest shows the whole file again.
    let whole = ok(change(
        &two,
        json!({"op": "toggle", "nodeId": "rest:group:#:core/a"}),
    ));
    let shown = visible_ids(&whole);
    assert_eq!(shown.iter().filter(|id| id.starts_with("g0n")).count(), 15);
    assert!(!shown.contains(&"rest:group:#:core/a".to_string()));
    assert!(
        shown.contains(&"rest:group:#:core".to_string()),
        "the other rest stays"
    );
    assert_eq!(whole["geometryIssues"], json!([]));
    // Without `partial`, the groups open whole.
    let full = ok(change(
        &snapshot,
        json!({"op": "reveal", "nodeIds": ["g0n3"], "partial": false}),
    ));
    let shown = visible_ids(&full);
    assert_eq!(shown.iter().filter(|id| id.starts_with("g0n")).count(), 15);
    assert!(!shown.iter().any(|id| id.starts_with("rest:")));
    // Collapsing a partly shown container and opening it again shows it whole.
    let closed = ok(change(
        &one,
        json!({"op": "toggle", "nodeId": "group:#:core/a"}),
    ));
    let again = ok(change(
        &closed,
        json!({"op": "toggle", "nodeId": "group:#:core/a"}),
    ));
    assert_eq!(
        visible_ids(&again)
            .iter()
            .filter(|id| id.starts_with("g0n"))
            .count(),
        15
    );
}

/// The path cut-out with a partial reveal shows the path's nodes, not their whole files, and the trace still finds them.
#[test]
fn a_path_revealed_in_part_keeps_the_view_small() {
    let snapshot = ok(call(json!({"op": "import", "graph": flat(), "limit": 100,
        "options": {"groups": domain_groups()}})));
    let traced = call(json!({"op": "trace", "snapshot": snapshot, "from": "g0n2", "to": "g1n9"}));
    let nodes = strings(&traced["nodes"]);
    let revealed = ok(change(&snapshot, json!({"op": "reveal", "nodeIds": nodes})));
    let shown = visible_ids(&revealed);
    assert!(nodes.iter().all(|n| shown.contains(n)));
    let functions = shown
        .iter()
        .filter(|id| id.starts_with('g') && !id.contains(':'))
        .count();
    assert_eq!(
        functions,
        nodes.len() + shown.iter().filter(|id| id.starts_with("g3")).count(),
        "only the path's functions, besides g3 that has no group"
    );
    let again = call(json!({"op": "trace", "snapshot": revealed, "from": "g0n2", "to": "g1n9"}));
    let mut visible = strings(&again["visible"]["nodes"]);
    visible.sort();
    let mut expected = nodes.clone();
    expected.sort();
    assert_eq!(visible, expected);
    assert_eq!(revealed["geometryIssues"], json!([]));
}

/// An abstracted view is laid out from its graph and view alone: opening a group and closing it again gives the
/// drawing of the import, box for box and route for route, whatever was opened in between.
#[test]
fn opening_and_closing_a_group_returns_to_the_drawing_of_the_import() {
    let snapshot = ok(call(json!({"op": "import", "graph": flat(), "limit": 100,
        "options": {"groups": domain_groups()}})));
    let open = ok(change(
        &snapshot,
        json!({"op": "toggle", "nodeId": "group:#:core"}),
    ));
    let deeper = ok(change(
        &open,
        json!({"op": "toggle", "nodeId": "group:#:core/a"}),
    ));
    let up = ok(change(
        &deeper,
        json!({"op": "toggle", "nodeId": "group:#:core/a"}),
    ));
    let closed = ok(change(
        &up,
        json!({"op": "toggle", "nodeId": "group:#:core"}),
    ));
    assert_eq!(closed["collapsedIds"], snapshot["collapsedIds"]);
    assert_eq!(closed["boxes"], snapshot["boxes"]);
    assert_eq!(closed["routes"], snapshot["routes"]);
    assert_eq!(
        up["boxes"], open["boxes"],
        "the same view, the same drawing"
    );
}

#[test]
fn the_name_before_engine_0_4_is_refused_with_the_new_one() {
    let graph = serde_json::json!({"nodes": [{"id": "a", "kind": "n", "label": "a"}], "edges": []});
    for request in [
        serde_json::json!({"op": "import", "graph": graph, "limit": 10, "options": {"initialCollapse": {"depth": 0}}}),
        serde_json::json!({"op": "import", "graph": graph, "limit": 10, "options": {"levelOfDetail": {"depth": 0}}}),
    ] {
        let out: serde_json::Value =
            serde_json::from_str(&arqlet_runtime::dispatch(&request.to_string())).unwrap();
        if request["options"].get("initialCollapse").is_some() {
            assert_eq!(out["ok"], false);
            assert!(
                out["error"]
                    .as_str()
                    .unwrap()
                    .contains("renamed to levelOfDetail"),
                "{out}"
            );
        } else {
            assert_eq!(out["ok"], true, "{out}");
        }
    }
}
