//! The `overview` operation (ADR 0018): a whole graph as discs in its containment hierarchy.
use arqlet_runtime::dispatch;
use serde_json::{json, Value};

/// A deterministic hierarchical graph: `groups` top clusters of `files` sub-clusters with `leaves` leaves each; edges
/// inside each sub-cluster (a ring), between neighbouring sub-clusters and a few across clusters; plus a cycle, a
/// self-loop, a parallel edge and an unresolved edge.
fn hierarchical(groups: usize, files: usize, leaves: usize) -> Value {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut seed = 7u64;
    let mut next = || {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (seed >> 33) as usize
    };
    for g in 0..groups {
        nodes.push(json!({"id": format!("g{g}"), "kind": "dir", "label": format!("g{g}"), "parentId": null}));
        for f in 0..files {
            let file = format!("g{g}/f{f}");
            nodes.push(
                json!({"id": file, "kind": "file", "label": file, "parentId": format!("g{g}")}),
            );
            for l in 0..leaves {
                nodes.push(json!({"id": format!("{file}/n{l}"), "kind": "fn", "label": "n", "parentId": file}));
                edges.push(json!({"id": format!("{file}/r{l}"), "kind": "call", "source": format!("{file}/n{l}"),
                    "target": format!("{file}/n{}", (l + 1) % leaves)}));
            }
            if f > 0 {
                edges.push(json!({"id": format!("{file}/up"), "kind": "call", "source": format!("{file}/n0"),
                    "target": format!("g{g}/f{}/n1", f - 1)}));
            }
        }
        let other = next() % groups;
        edges.push(
            json!({"id": format!("g{g}/x"), "kind": "call", "source": format!("g{g}/f0/n0"),
            "target": format!("g{other}/f{}/n0", next() % files)}),
        );
    }
    edges.push(json!({"id": "self", "kind": "call", "source": "g0/f0/n0", "target": "g0/f0/n0"}));
    edges.push(
        json!({"id": "parallel", "kind": "call", "source": "g0/f0/n0", "target": "g0/f0/n1"}),
    );
    edges.push(json!({"id": "open", "kind": "call", "source": "g0/f0/n0", "target": null}));
    json!({"nodes": nodes, "edges": edges})
}

fn overview(graph: &Value, threads: usize) -> Value {
    let response: Value = serde_json::from_str(&dispatch(
        &json!({"op": "overview", "graph": graph, "threads": threads}).to_string(),
    ))
    .unwrap();
    assert_eq!(response["ok"], true, "{response}");
    response["overview"].clone()
}

#[test]
fn the_overview_is_identical_for_any_thread_count() {
    let graph = hierarchical(6, 5, 12);
    let one = overview(&graph, 1);
    for threads in [2, 4, 8] {
        assert_eq!(overview(&graph, threads), one, "threads {threads}");
    }
}

#[test]
fn every_node_gets_a_disc_inside_its_container_and_edges_keep_their_direction() {
    let graph = hierarchical(4, 3, 10);
    let o = overview(&graph, 4);
    let n = graph["nodes"].as_array().unwrap().len();
    assert_eq!(o["ids"].as_array().unwrap().len(), n);
    assert_eq!(o["stats"]["overlaps"], 0);
    assert_eq!(o["stats"]["unresolvedEdges"], 1);
    assert_eq!(o["stats"]["containers"], 4 + 4 * 3);
    // Kinds as a table in order of first use, and an index per node.
    assert_eq!(o["kinds"], json!(["dir", "file", "fn"]));
    assert_eq!(o["kind"][0], 0);
    assert_eq!(o["kind"][1], 1);
    assert_eq!(o["kind"][2], 2);
    let f = |key: &str, i: usize| o[key][i].as_f64().unwrap();
    for i in 0..n {
        let p = o["parent"][i].as_i64().unwrap();
        if p >= 0 {
            let p = p as usize;
            let d = ((f("x", i) - f("x", p)).powi(2) + (f("y", i) - f("y", p)).powi(2)).sqrt();
            // Hundredths are rounded in the response.
            assert!(d + f("r", i) <= f("r", p) + 0.02, "node {i} leaves {p}");
        }
    }
    // Edges as index pairs in input order, source first: the cycle, self-loop and parallel edge are all there.
    let ids: Vec<&str> = o["ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    let edges = o["edges"].as_array().unwrap();
    let resolved: Vec<&Value> = graph["edges"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| !e["target"].is_null())
        .collect();
    assert_eq!(edges.len(), 2 * resolved.len());
    for (k, e) in resolved.iter().enumerate() {
        assert_eq!(
            ids[edges[2 * k].as_u64().unwrap() as usize],
            e["source"].as_str().unwrap()
        );
        assert_eq!(
            ids[edges[2 * k + 1].as_u64().unwrap() as usize],
            e["target"].as_str().unwrap()
        );
    }
}

#[test]
fn bad_requests_are_refused_explicitly() {
    let graph = hierarchical(1, 1, 3);
    for (request, needle) in [
        (
            json!({"op": "overview", "graph": graph, "options": {"ticks": 0}}),
            "ticks",
        ),
        (
            json!({"op": "overview", "graph": graph, "options": {"spread": 1}}),
            "Unknown overview option",
        ),
        (
            json!({"op": "overview", "graph": {"nodes": [{"id": "a", "kind": "n", "label": "a", "parentId": "missing"}], "edges": []}}),
            "Missing parent",
        ),
    ] {
        let response: Value = serde_json::from_str(&dispatch(&request.to_string())).unwrap();
        assert_eq!(response["ok"], false);
        assert!(
            response["error"].as_str().unwrap().contains(needle),
            "{response}"
        );
    }
}

#[test]
fn a_structured_cluster_gets_its_radius_and_a_continuation_keeps_the_rest() {
    let graph = hierarchical(5, 4, 8);
    let first = overview(&graph, 1);
    let ids: Vec<&str> = first["ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    let file = ids.iter().position(|id| *id == "g2/f1").unwrap();
    let r0 = first["r"][file].as_f64().unwrap();
    let start = json!({"x": first["x"], "y": first["y"], "r": first["r"]});
    let request = |radii: Value, start: &Value, threads: usize| {
        let response: Value = serde_json::from_str(&dispatch(
            &json!({"op": "overview", "graph": graph, "radii": radii, "start": start, "threads": threads}).to_string(),
        ))
        .unwrap();
        assert_eq!(response["ok"], true, "{response}");
        response["overview"].clone()
    };
    let grown = request(json!({"g2/f1": r0 * 4.}), &start, 1);
    assert_eq!(grown["stats"]["overlaps"], 0);
    assert!((grown["r"][file].as_f64().unwrap() - r0 * 4.).abs() < 0.01);
    // Identical for any thread count, with hints too.
    assert_eq!(request(json!({"g2/f1": r0 * 4.}), &start, 4), grown);
    // Group g0 is not on the changed path: inside it nothing moves relative to its centre (hundredths are rounded).
    let g0 = ids.iter().position(|id| *id == "g0").unwrap();
    for (i, id) in ids.iter().enumerate() {
        if id.starts_with("g0/") {
            let rel = |o: &Value, k: &str| o[k][i].as_f64().unwrap() - o[k][g0].as_f64().unwrap();
            assert!(
                (rel(&first, "x") - rel(&grown, "x")).abs() < 0.02
                    && (rel(&first, "y") - rel(&grown, "y")).abs() < 0.02,
                "{id}"
            );
        }
    }
    // A start from another graph is refused.
    let wrong = json!({"x": [0.0], "y": [0.0], "r": [1.0]});
    let response: Value = serde_json::from_str(&dispatch(
        &json!({"op": "overview", "graph": graph, "start": wrong}).to_string(),
    ))
    .unwrap();
    assert!(response["error"].as_str().unwrap().contains("same graph"));
}

#[test]
fn decisions_and_rules_are_engine_values() {
    let graph = hierarchical(2, 2, 4);
    // A shipped preset's config set, with rules for the overview added.
    let preset: Value = serde_json::from_str(include_str!("../../../presets/auto.json")).unwrap();
    let mut config = preset["configSet"].clone();
    config["overview"] =
        json!({"rules": {"file": "structured", "module": "collapsed"}, "auto": false});
    let response: Value = serde_json::from_str(&dispatch(
        &json!({"op": "overview", "graph": graph, "configSet": config,
            "intent": {"decisions": {"g1": "collapsed", "g0/f1": "organic"}}})
        .to_string(),
    ))
    .unwrap();
    assert_eq!(response["ok"], true, "{response}");
    let r = &response["overview"]["representation"];
    assert_eq!(
        r["decisions"],
        json!({"g0/f1": "organic", "g1": "collapsed"})
    );
    assert_eq!(
        r["rules"],
        json!({"file": "structured", "module": "collapsed"})
    );
    assert_eq!(r["auto"], false);
    // The graph has no kind `module`: reported, not refused.
    assert_eq!(r["unknownKinds"], json!(["module"]));
    // A decision for a node the graph does not have, or a value that is no representation, is refused.
    for (intent, needle) in [
        (json!({"decisions": {"nowhere": "organic"}}), "unknown node"),
        (json!({"decisions": {"g1": "flat"}}), "intent"),
    ] {
        let response: Value = serde_json::from_str(&dispatch(
            &json!({"op": "overview", "graph": graph, "intent": intent}).to_string(),
        ))
        .unwrap();
        assert_eq!(response["ok"], false);
        assert!(
            response["error"].as_str().unwrap().contains(needle),
            "{response}"
        );
    }
    // Without them: no decisions, no rules, the zoom decides.
    let plain = overview(&graph, 1);
    assert_eq!(
        plain["representation"],
        json!({"decisions": {}, "rules": {}, "auto": true, "unknownKinds": []})
    );
}

#[test]
fn a_flat_graph_is_split_into_communities_that_drill_down_can_use() {
    // 200 nodes in four rings of 50, each ring a dense neighbourhood, one edge between neighbouring rings: no containers.
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    for r in 0..4 {
        for i in 0..50 {
            nodes.push(
                json!({"id": format!("r{r}n{i}"), "kind": "fn", "label": "n", "parentId": null}),
            );
            for d in 1..4 {
                edges.push(json!({"id": format!("r{r}e{i}-{d}"), "kind": "call", "source": format!("r{r}n{i}"),
                    "target": format!("r{r}n{}", (i + d) % 50)}));
            }
        }
        edges.push(json!({"id": format!("x{r}"), "kind": "call", "source": format!("r{r}n0"), "target": format!("r{}n25", (r + 1) % 4)}));
    }
    let graph = json!({"nodes": nodes, "edges": edges});
    let o = overview(&graph, 4);
    let communities = o["stats"]["communities"].as_u64().unwrap();
    assert!(communities >= 4, "{communities} communities");
    assert_eq!(o["stats"]["overlaps"], 0);
    let ids: Vec<&str> = o["ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(ids.len(), 200 + communities as usize);
    // The added clusters come after the graph's nodes, as kind `community`, and every node lies in one.
    let kinds = o["kinds"].as_array().unwrap();
    for (i, id) in ids.iter().enumerate().skip(200) {
        assert!(id.starts_with("community:"), "{id}");
        assert_eq!(kinds[o["kind"][i].as_u64().unwrap() as usize], "community");
    }
    assert!((0..200).all(|i| o["parent"][i].as_i64().unwrap() >= 200));
    // Identical for any thread count; switched off, the graph stays flat.
    assert_eq!(overview(&graph, 1), o);
    let flat: Value = serde_json::from_str(&dispatch(
        &json!({"op": "overview", "graph": graph, "options": {"communities": 0}}).to_string(),
    ))
    .unwrap();
    assert_eq!(flat["overview"]["stats"]["communities"], 0);
}

#[test]
fn satellites_are_gathered_with_their_hub_and_added_clusters_are_named() {
    // A hub with three satellites, and a pair linked to each other and to the hub.
    let mut nodes = vec![json!({"id": "hub", "kind": "service", "label": "Hub"})];
    let mut edges = Vec::new();
    for s in ["s1", "s2", "s3"] {
        nodes.push(json!({"id": s, "kind": "service", "label": s}));
        edges.push(json!({"id": format!("{s}-hub"), "kind": "e", "source": s, "target": "hub"}));
    }
    for a in ["p", "q"] {
        nodes.push(json!({"id": a, "kind": "service", "label": a}));
    }
    for (a, b) in [("p", "q"), ("q", "p"), ("p", "hub"), ("q", "hub")] {
        edges.push(json!({"id": format!("{a}-{b}"), "kind": "e", "source": a, "target": b}));
    }
    let graph = json!({"nodes": nodes, "edges": edges});
    let request = |options: Value| {
        let out: Value = serde_json::from_str(&dispatch(
            &json!({"op": "overview", "graph": graph, "threads": 1, "options": options})
                .to_string(),
        ))
        .unwrap();
        out
    };
    let plain = request(json!({}));
    assert_eq!(
        plain["overview"]["stats"]["communities"], 0,
        "off by default"
    );
    let gathered = request(json!({"satellites": 1}));
    let o = &gathered["overview"];
    let ids: Vec<&str> = o["ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    let cluster = ids
        .iter()
        .position(|id| id.ends_with(":sat.0"))
        .expect("a cluster for the hub");
    assert_eq!(o["labels"][cluster], "Hub +3");
    for s in ["hub", "s1", "s2", "s3"] {
        let i = ids.iter().position(|id| *id == s).unwrap();
        assert_eq!(
            o["parent"][i], cluster as i64,
            "{s} lies in the hub's cluster"
        );
    }
    let p = ids.iter().position(|id| *id == "p").unwrap();
    assert_eq!(
        o["parent"][p], -1,
        "p has half of its edges to q: no satellite at share 1"
    );
    let refused = request(json!({"satellites": 1.5}));
    assert_eq!(refused["ok"], false);
}

#[test]
fn chains_of_single_children_share_a_disc_when_asked() {
    let graph = json!({"nodes": [
        {"id": "f", "kind": "package", "label": "f"},
        {"id": "p", "kind": "module", "label": "p", "parentId": "f"},
        {"id": "s1", "kind": "file", "label": "s1", "parentId": "p"},
        {"id": "s2", "kind": "file", "label": "s2", "parentId": "p"},
        {"id": "x", "kind": "package", "label": "x"}
    ], "edges": []});
    let run = |options: Value| -> Value {
        serde_json::from_str(&dispatch(
            &json!({"op": "overview", "graph": graph, "threads": 1, "options": options})
                .to_string(),
        ))
        .unwrap()
    };
    let r = |o: &Value, i: usize| o["overview"]["r"][i].as_f64().unwrap();
    let chained = run(json!({"chains": true}));
    assert_eq!(r(&chained, 0), r(&chained, 1));
    let plain = run(json!({}));
    assert!(r(&plain, 0) > r(&plain, 1), "off by default");
    assert_eq!(run(json!({"chains": 1}))["ok"], false);
}

#[test]
fn the_overview_takes_groups_and_a_level_of_detail() {
    let graph = json!({"nodes": [
        {"id": "a", "kind": "service", "label": "A"},
        {"id": "b", "kind": "service", "label": "B"},
        {"id": "c", "kind": "service", "label": "C"}
    ], "edges": [{"id": "e", "kind": "e", "source": "a", "target": "b"}]});
    let preset: Value = serde_json::from_str(include_str!("../../../presets/auto.json")).unwrap();
    let mut config = preset["configSet"].clone();
    config["overview"] = json!({"levelOfDetail": {"depth": 0, "kinds": ["group"]}});
    let out: Value = serde_json::from_str(&dispatch(
        &json!({"op": "overview", "graph": graph, "threads": 1,
            "options": {"groups": {"a": ["Core"], "b": ["Core"]}}, "configSet": config})
        .to_string(),
    ))
    .unwrap();
    assert_eq!(out["ok"], true, "{out}");
    let o = &out["overview"];
    let ids: Vec<&str> = o["ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    let group = ids
        .iter()
        .position(|id| id.starts_with("group:"))
        .expect("a group container");
    assert_eq!(o["labels"][group], "Core");
    assert_eq!(
        o["kinds"][o["kind"][group].as_u64().unwrap() as usize],
        "group"
    );
    for member in ["a", "b"] {
        let i = ids.iter().position(|id| *id == member).unwrap();
        assert_eq!(o["parent"][i], group as i64);
    }
    assert_eq!(
        o["representation"]["levelOfDetail"],
        json!({"depth": 0, "kinds": ["group"]})
    );
}

#[test]
fn a_session_keeps_the_graph_and_answers_as_dispatch_does() {
    use arqlet_runtime::engine_session::EngineSession;
    let graph = hierarchical(5, 4, 8);
    let options = json!({"communities": 6, "satellites": 0.5});
    let mut session = EngineSession::new();
    // Before any graph: refused, and nothing is kept.
    let empty = session.run_value(&json!({"op": "overview"}));
    assert!(
        empty["error"].as_str().unwrap().contains("graph"),
        "{empty}"
    );
    let first = session
        .run_value(&json!({"op": "overview", "graph": graph, "options": options, "threads": 2}));
    assert_eq!(first["ok"], true, "{first}");
    assert_eq!(
        first["overview"],
        overview_with(&graph, &options),
        "the same as dispatch"
    );
    // Later requests leave the graph out; `start` without positions continues from the last layout.
    let ids: Vec<&str> = first["overview"]["ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    let file = ids.iter().position(|id| *id == "g2/f1").unwrap();
    let r0 = first["overview"]["r"][file].as_f64().unwrap();
    let radii = json!({"g2/f1": r0 * 3.});
    let grown = session.run_value(
        &json!({"op": "overview", "options": options, "radii": radii,
        "start": {"settle": true}}),
    );
    assert_eq!(grown["ok"], true, "{grown}");
    let o = &first["overview"];
    let start = json!({"x": o["x"], "y": o["y"], "r": o["r"], "settle": true});
    let full = json!({"op": "overview", "graph": graph, "options": options, "radii": radii, "start": start});
    let expected: Value = serde_json::from_str(&dispatch(&full.to_string())).unwrap();
    assert_eq!(
        grown, expected,
        "the same as dispatch given the graph and the positions"
    );
    // Other clusters (other options) cannot continue from the last positions; a request without start can.
    let other = json!({"communities": 0});
    let refused = session.run_value(&json!({"op": "overview", "options": other, "start": {}}));
    assert!(
        refused["error"]
            .as_str()
            .unwrap()
            .contains("clusters changed"),
        "{refused}"
    );
    let regrouped = session.run_value(&json!({"op": "overview", "options": other}));
    assert_eq!(regrouped["overview"], overview_with(&graph, &other));
    // The refused request changed nothing: continuing on the regrouped clusters works.
    let next = session.run_value(&json!({"op": "overview", "options": other, "start": {}}));
    assert_eq!(next["ok"], true, "{next}");
    // A new graph replaces the old one; a start then needs a layout of the new graph first.
    let small = hierarchical(2, 2, 3);
    let fresh = session.run_value(&json!({"op": "overview", "graph": small, "start": {}}));
    assert!(
        fresh["error"].as_str().unwrap().contains("continue"),
        "{fresh}"
    );
    let replaced = session.run_value(&json!({"op": "overview", "graph": small}));
    assert_eq!(replaced["overview"], overview_with(&small, &json!({})));
    // The layered view of the same session is unaffected.
    assert!(session.run_value(&json!({"op": "scene"}))["error"]
        .as_str()
        .unwrap()
        .contains("no view"));
}

fn overview_with(graph: &Value, options: &Value) -> Value {
    let request = json!({"op": "overview", "graph": graph, "options": options});
    let response: Value = serde_json::from_str(&dispatch(&request.to_string())).unwrap();
    assert_eq!(response["ok"], true, "{response}");
    response["overview"].clone()
}

#[test]
fn every_answer_is_the_typed_overview_contract() {
    // The answer is written from `arqlet_model::Overview`; reading it back with unknown fields refused keeps the
    // contract types in `@arqlet/contracts` exact, with and without the optional parts.
    let graph = hierarchical(3, 3, 5);
    for request in [
        json!({"op": "overview", "graph": graph}),
        json!({"op": "overview", "graph": graph, "options": {"satellites": 0.5, "chains": true, "communities": 2},
            "intent": {"decisions": {"g0": "organic"}}}),
    ] {
        let response: Value = serde_json::from_str(&dispatch(&request.to_string())).unwrap();
        assert_eq!(response["ok"], true, "{response}");
        let typed: arqlet_model::Overview =
            serde_json::from_value(response["overview"].clone()).unwrap();
        assert_eq!(serde_json::to_value(&typed).unwrap(), response["overview"]);
    }
    // A typed request is a request the operation takes.
    let request = arqlet_model::OverviewRequest {
        graph: Some(serde_json::from_value(graph).unwrap()),
        options: Some(arqlet_model::OverviewLayoutOptions {
            node_radius: Some(3.),
            gap: Some(1.),
            padding: Some(4.),
            charge: Some(0.05),
            gravity: Some(0.1),
            communities: Some(4),
            satellites: Some(0.5),
            chains: Some(true),
            bus_rows: Some(true),
            ticks: Some(50),
            groups: Some([("g0/f0".to_string(), vec!["A".to_string()])].into()),
            hubs: Some(arqlet_model::CommunityHubs {
                min_links: Some(10),
                top: None,
                nodes: vec!["g1".into()],
            }),
        }),
        radii: Some([("g1".to_string(), 80.)].into()),
        threads: Some(2),
        ..Default::default()
    };
    let mut v = serde_json::to_value(&request).unwrap();
    v["op"] = json!("overview");
    let response: Value = serde_json::from_str(&dispatch(&v.to_string())).unwrap();
    assert_eq!(response["ok"], true, "{response}");
}

#[test]
fn a_session_answers_only_what_moves_while_the_clusters_stay() {
    use arqlet_runtime::engine_session::EngineSession;
    let graph = hierarchical(4, 3, 6);
    let options = json!({"communities": 0});
    let mut session = EngineSession::new();
    // With a new graph the answer is whole, positionsOnly or not.
    let first = session.run_value(
        &json!({"op": "overview", "graph": graph, "options": options, "positionsOnly": true}),
    );
    assert!(first["overview"]["ids"].is_array(), "whole: {first}");
    // Same clusters, positionsOnly: only what moves, equal to the whole answer of the same request.
    let request =
        json!({"op": "overview", "options": options, "radii": {"g1": 120.0}, "start": {}});
    let mut only = request.clone();
    only["positionsOnly"] = json!(true);
    let moved = session.run_value(&only);
    assert_eq!(moved["ok"], true, "{moved}");
    let o = &moved["overview"];
    assert_eq!(o["positionsOnly"], true);
    assert!(o.get("ids").is_none() && o.get("labels").is_none() && o.get("edges").is_none());
    let _: arqlet_model::OverviewPositions = serde_json::from_value(o.clone()).unwrap();
    let mut twin = EngineSession::new();
    twin.run_value(&json!({"op": "overview", "graph": graph, "options": options}));
    let whole = twin.run_value(&request);
    for key in ["x", "y", "r", "rootRadius", "stats", "representation"] {
        assert_eq!(o[key], whole["overview"][key], "{key}");
    }
    // Other clusters: whole again.
    let other = session.run_value(
        &json!({"op": "overview", "options": {"communities": 4}, "positionsOnly": true}),
    );
    assert!(other["overview"]["ids"].is_array());
    assert!(other["overview"].get("positionsOnly").is_none());
}

/// The numbers of a columnar overview answer are those of the JSON answer to the same request, and the rest of it stays
/// as it was (provisional, ADR 0019 step 4).
#[test]
fn a_columnar_overview_equals_the_json_overview() {
    use arqlet_runtime::columns::Column;
    use arqlet_runtime::engine_session::EngineSession;
    let graph = hierarchical(4, 3, 6);
    let options = json!({"communities": 0});
    let mut columnar = EngineSession::new();
    let mut plain = EngineSession::new();
    let first = json!({"op": "overview", "graph": graph, "options": options});
    let mut asked = first.clone();
    asked["columns"] = json!(true);
    let whole = plain.run_value(&first)["overview"].clone();
    let answer = columnar.run_value(&asked);
    assert_eq!(answer["ok"], true, "{answer}");
    let meta = &answer["overview"];
    let columns = columnar.overview_columns().expect("columns kept");
    let numbers = |name: &str| -> Value {
        match columns
            .columns()
            .into_iter()
            .find(|(n, _)| *n == name)
            .unwrap()
            .1
        {
            Column::F64(c) => json!(c),
            Column::U32(c) => json!(c),
            Column::I32(c) => json!(c),
            Column::F32(c) => json!(c),
        }
    };
    for key in ["x", "y", "r", "parent", "kind", "edges", "links"] {
        assert!(
            meta.get(key).is_none(),
            "{key} is a column, not in the answer"
        );
        assert_eq!(numbers(key), whole[key], "{key}");
    }
    for key in [
        "schemaVersion",
        "representation",
        "ids",
        "labels",
        "kinds",
        "rootRadius",
        "stats",
    ] {
        assert_eq!(meta[key], whole[key], "{key}");
    }
    let listed: Vec<_> = meta["columns"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].clone())
        .collect();
    assert_eq!(
        listed,
        json!(["x", "y", "r", "parent", "kind", "edges", "links"])
            .as_array()
            .unwrap()
            .clone()
    );
    // A continuation on the same clusters with positionsOnly: three columns, equal to the whole answer's.
    let next = json!({"op": "overview", "options": options, "radii": {"g1": 120.0}, "start": {}});
    let mut moved_request = next.clone();
    moved_request["columns"] = json!(true);
    moved_request["positionsOnly"] = json!(true);
    let moved = columnar.run_value(&moved_request);
    assert_eq!(moved["overview"]["positionsOnly"], true, "{moved}");
    assert!(moved["overview"].get("ids").is_none());
    let twin_whole = plain.run_value(&next)["overview"].clone();
    let columns = columnar.overview_columns().unwrap();
    assert_eq!(columns.columns().len(), 3);
    assert!(columns.column(3).is_none());
    for (name, column) in columns.columns() {
        let Column::F64(c) = column else {
            panic!("{name}")
        };
        assert_eq!(json!(c), twin_whole[name], "{name}");
    }
    assert_eq!(moved["overview"]["rootRadius"], twin_whole["rootRadius"]);
    // A request without columns drops them.
    columnar.run_value(&json!({"op": "overview", "options": options}));
    assert!(columnar.overview_columns().is_none());
}

#[test]
fn paths_and_neighbourhoods_on_the_overview_graph() {
    use arqlet_runtime::engine_session::EngineSession;
    let graph = hierarchical(3, 3, 4);
    // Without a session, the request brings the graph.
    let r: Value = serde_json::from_str(&dispatch(
        &json!({"op": "overviewTrace", "graph": graph, "from": "g0/f1/n0", "to": "g0/f0/n2"})
            .to_string(),
    ))
    .unwrap();
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(r["found"], true);
    let typed: arqlet_model::GraphTrace = serde_json::from_value({
        let mut a = r.clone();
        a.as_object_mut().unwrap().remove("ok");
        a
    })
    .unwrap();
    assert!(
        typed.nodes.contains(&"g0/f1/n0".to_string())
            && typed.nodes.contains(&"g0/f0/n2".to_string())
    );
    assert!(typed.edges.contains(&"g0/f1/up".to_string()), "{typed:?}");
    // In a session the graph the overview keeps is used, and the answer is the same.
    let mut session = EngineSession::new();
    session.run_value(&json!({"op": "overview", "graph": graph}));
    let s =
        session.run_value(&json!({"op": "overviewTrace", "from": "g0/f1/n0", "to": "g0/f0/n2"}));
    assert_eq!(s, r);
    let n = session.run_value(
        &json!({"op": "overviewNeighbourhood", "nodeId": "g0/f0/n0", "hops": 1, "direction": "out"}),
    );
    assert_eq!(n["ok"], true, "{n}");
    assert_eq!(n["hops"]["g0/f0/n0"], 0);
    assert!(n["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|x| x == "g0/f0/n1"));
    // Errors: no overview yet in a session, an unknown node.
    let empty =
        EngineSession::new().run_value(&json!({"op": "overviewTrace", "from": "a", "to": "b"}));
    assert!(empty["error"].as_str().unwrap().contains("no overview yet"));
    let unknown =
        session.run_value(&json!({"op": "overviewTrace", "from": "nowhere", "to": "g0/f0/n2"}));
    assert_eq!(unknown["ok"], false);
}

#[test]
fn a_bus_line_node_is_a_line_with_taps_in_the_overview() {
    // A bus with five members and a second bus with one: each a line; the members' taps meet it at their projections.
    let mut nodes = vec![
        json!({"id": "can", "kind": "bus", "label": "CAN", "shape": "busLine"}),
        json!({"id": "lin", "kind": "bus", "label": "LIN", "shape": "busLine"}),
    ];
    let mut edges = Vec::new();
    for i in 0..5 {
        nodes.push(json!({"id": format!("e{i}"), "kind": "ecu", "label": format!("E{i}")}));
        edges.push(json!({"id": format!("c{i}"), "kind": "connects", "source": format!("e{i}"), "target": "can"}));
    }
    edges.push(json!({"id": "l0", "kind": "connects", "source": "e0", "target": "lin"}));
    let graph = json!({"nodes": nodes, "edges": edges});
    let o = overview(&graph, 1);
    let lines = o["busLines"].as_array().unwrap();
    assert_eq!(lines.len(), 2);
    let can = &lines[0];
    assert_eq!(can["node"], 0);
    assert_eq!(can["taps"], json!([2, 3, 4, 5, 6]));
    let (x1, y1, x2, y2) = (
        can["x1"].as_f64().unwrap(),
        can["y1"].as_f64().unwrap(),
        can["x2"].as_f64().unwrap(),
        can["y2"].as_f64().unwrap(),
    );
    let len = ((x2 - x1).powi(2) + (y2 - y1).powi(2)).sqrt();
    assert!(len > 0.);
    // Every foot lies on the line, and a tap is perpendicular to it (within rounding).
    let feet = can["feet"].as_array().unwrap();
    for (k, tap) in can["taps"].as_array().unwrap().iter().enumerate() {
        let (fx, fy) = (
            feet[2 * k].as_f64().unwrap(),
            feet[2 * k + 1].as_f64().unwrap(),
        );
        let cross = ((x2 - x1) * (fy - y1) - (y2 - y1) * (fx - x1)) / len;
        assert!(cross.abs() < 0.05, "foot off the line: {cross}");
        let j = tap.as_u64().unwrap() as usize;
        let (mx, my) = (o["x"][j].as_f64().unwrap(), o["y"][j].as_f64().unwrap());
        let dot = ((x2 - x1) * (mx - fx) + (y2 - y1) * (my - fy)) / len;
        assert!(dot.abs() < 0.05, "tap not perpendicular: {dot}");
    }
    // The bus with one member: a short line across its own centre.
    assert_eq!(lines[1]["taps"], json!([2]));
    // Without busLine nodes nothing is added.
    let plain = overview(
        &json!({"nodes": [{"id": "a", "kind": "k", "label": "a"}], "edges": []}),
        1,
    );
    assert!(plain.get("busLines").is_none());
    // The same for any thread count.
    assert_eq!(overview(&graph, 4)["busLines"], o["busLines"]);
}

/// #147: buses given as data (`Graph.buses` with taps, no node) are drawn as bus lines in the overview, as `busLine` nodes are.
#[test]
fn a_bus_given_as_data_is_a_bus_line_in_the_overview() {
    let mut nodes = Vec::new();
    let mut can = Vec::new();
    for i in 0..5 {
        nodes.push(json!({"id": format!("e{i}"), "kind": "ecu", "label": format!("E{i}")}));
        can.push(json!({"edge": format!("c{i}"), "node": format!("e{i}"), "toBus": true, "kind": "connects"}));
    }
    let lin = vec![json!({"edge": "l0", "node": "e0", "toBus": true, "kind": "connects"})];
    let graph = json!({"nodes": nodes, "edges": [], "buses": [
        {"id": "can", "kind": "bus", "label": "CAN", "taps": can},
        {"id": "lin", "kind": "bus", "label": "LIN", "taps": lin}
    ]});
    let o = overview(&graph, 1);
    let ids: Vec<&str> = o["ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    let lines = o["busLines"]
        .as_array()
        .expect("bus lines for buses given as data");
    assert_eq!(lines.len(), 2);
    let members = |line: &Value| -> (String, Vec<String>) {
        let bus = ids[line["node"].as_u64().unwrap() as usize].to_string();
        let mut taps: Vec<String> = line["taps"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| ids[t.as_u64().unwrap() as usize].to_string())
            .collect();
        taps.sort();
        (bus, taps)
    };
    let mut found: Vec<(String, Vec<String>)> = lines.iter().map(members).collect();
    found.sort();
    assert_eq!(
        found[0],
        (
            "can".to_string(),
            vec!["e0", "e1", "e2", "e3", "e4"]
                .into_iter()
                .map(String::from)
                .collect::<Vec<_>>()
        )
    );
    assert_eq!(found[1], ("lin".to_string(), vec!["e0".to_string()]));
    assert_eq!(overview(&graph, 4)["busLines"], o["busLines"]);
}

#[test]
fn a_member_in_another_cluster_taps_the_bus_line_without_stretching_it() {
    // A zone with a bus and four members, and a gateway outside on the same bus: the line stays within the zone's disc.
    let mut nodes = vec![
        json!({"id": "zone", "kind": "zone", "label": "zone"}),
        json!({"id": "can", "kind": "bus", "label": "CAN", "shape": "busLine", "parentId": "zone"}),
        json!({"id": "gw", "kind": "ecu", "label": "gw"}),
        json!({"id": "far", "kind": "ecu", "label": "far"}),
    ];
    let mut edges = vec![
        json!({"id": "g", "kind": "connects", "source": "gw", "target": "can"}),
        json!({"id": "f", "kind": "connects", "source": "gw", "target": "far"}),
    ];
    for i in 0..4 {
        nodes.push(json!({"id": format!("e{i}"), "kind": "ecu", "label": format!("E{i}"), "parentId": "zone"}));
        edges.push(json!({"id": format!("c{i}"), "kind": "connects", "source": format!("e{i}"), "target": "can"}));
    }
    let o = overview(&json!({"nodes": nodes, "edges": edges}), 1);
    let line = &o["busLines"][0];
    let len = (line["x2"].as_f64().unwrap() - line["x1"].as_f64().unwrap())
        .hypot(line["y2"].as_f64().unwrap() - line["y1"].as_f64().unwrap());
    let zone = o["r"][0].as_f64().unwrap();
    assert!(len <= 2. * zone, "line {len} within the zone's disc {zone}");
    assert_eq!(
        line["taps"].as_array().unwrap().len(),
        5,
        "the gateway taps it too"
    );
}

#[test]
fn with_bus_rows_the_members_of_a_bus_stand_in_a_row_on_its_line() {
    // Eight members of one bus: without the option the forces place them around it; with it they stand in a row, and the
    // line runs through their centres.
    let mut nodes = vec![json!({"id": "can", "kind": "bus", "label": "CAN", "shape": "busLine"})];
    let mut edges = Vec::new();
    for i in 0..8 {
        nodes.push(json!({"id": format!("e{i}"), "kind": "ecu", "label": format!("E{i}")}));
        edges.push(json!({"id": format!("c{i}"), "kind": "connects", "source": format!("e{i}"), "target": "can"}));
    }
    let graph = json!({"nodes": nodes, "edges": edges});
    let run = |options: Value| -> Value {
        let response: Value = serde_json::from_str(&dispatch(
            &json!({"op": "overview", "graph": graph, "options": options}).to_string(),
        ))
        .unwrap();
        assert_eq!(response["ok"], true, "{response}");
        response["overview"].clone()
    };
    let off_line = |o: &Value| -> f64 {
        let l = &o["busLines"][0];
        let horizontal = l["y1"] == l["y2"];
        (1..=8)
            .map(|j| {
                let (x, y) = (o["x"][j].as_f64().unwrap(), o["y"][j].as_f64().unwrap());
                if horizontal {
                    (y - l["y1"].as_f64().unwrap()).abs()
                } else {
                    (x - l["x1"].as_f64().unwrap()).abs()
                }
            })
            .fold(0., f64::max)
    };
    let free = run(json!({}));
    let rows = run(json!({"busRows": true}));
    assert!(
        off_line(&free) > 1.,
        "around the bus without the option: {}",
        off_line(&free)
    );
    assert!(
        off_line(&rows) < 0.05,
        "in a row with it: {}",
        off_line(&rows)
    );
    // No overlaps, and the same for any thread count.
    assert_eq!(rows["stats"]["overlaps"], 0);
    let response: Value = serde_json::from_str(&dispatch(
        &json!({"op": "overview", "graph": graph, "options": {"busRows": true}, "threads": 4})
            .to_string(),
    ))
    .unwrap();
    assert_eq!(response["overview"]["x"], rows["x"]);
}

#[test]
fn a_moved_node_stays_where_it_was_put_and_its_neighbours_follow() {
    // A chain of twelve nodes; the end node is moved far out. It stays exactly there, its neighbour moves after it, and
    // nothing overlaps.
    let nodes: Vec<Value> = (0..12)
        .map(|i| json!({"id": format!("n{i}"), "kind": "k", "label": format!("n{i}")}))
        .collect();
    let edges: Vec<Value> = (0..11)
        .map(|i| json!({"id": format!("e{i}"), "kind": "k", "source": format!("n{i}"), "target": format!("n{}", i + 1)}))
        .collect();
    let graph = json!({"nodes": nodes, "edges": edges});
    let first = overview(&graph, 1);
    let at = |o: &Value, i: usize| (o["x"][i].as_f64().unwrap(), o["y"][i].as_f64().unwrap());
    let (x0, y0) = at(&first, 0);
    let target = (x0 + 80., y0 + 60.);
    let start = json!({"x": first["x"], "y": first["y"], "r": first["r"], "moved": {"n0": {"x": target.0, "y": target.1}}});
    let response: Value = serde_json::from_str(&dispatch(
        &json!({"op": "overview", "graph": graph, "start": start}).to_string(),
    ))
    .unwrap();
    assert_eq!(response["ok"], true, "{response}");
    let moved = &response["overview"];
    let (mx, my) = at(moved, 0);
    assert!(
        (mx - target.0).abs() < 0.02 && (my - target.1).abs() < 0.02,
        "held at the target: {mx}, {my}"
    );
    let step = |o: &Value| {
        let (a, b) = (at(o, 0), at(o, 1));
        ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
    };
    // Its neighbour followed: not as far from it as it would be had nothing moved.
    let unmoved =
        ((target.0 - at(&first, 1).0).powi(2) + (target.1 - at(&first, 1).1).powi(2)).sqrt();
    assert!(
        step(moved) < unmoved * 0.8,
        "neighbour follows: {} vs {unmoved}",
        step(moved)
    );
    assert_eq!(moved["stats"]["overlaps"], 0);
    // An unknown node is refused.
    let bad = json!({"x": first["x"], "y": first["y"], "r": first["r"], "moved": {"nope": {"x": 0, "y": 0}}});
    let response: Value = serde_json::from_str(&dispatch(
        &json!({"op": "overview", "graph": graph, "start": bad}).to_string(),
    ))
    .unwrap();
    assert_eq!(response["ok"], false);
}

#[test]
fn a_moved_cluster_takes_its_children_along() {
    // Three clusters of leaves; one cluster is moved: it stands at the target, its children keep their places relative
    // to it, nothing overlaps.
    let graph = hierarchical(3, 2, 4);
    let first = overview(&graph, 1);
    let ids: Vec<&str> = first["ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    let parent = |i: usize| first["parent"][i].as_i64().unwrap();
    let cluster = ids.iter().position(|id| *id == "g1").unwrap();
    let child = (0..ids.len())
        .find(|&i| parent(i) == cluster as i64)
        .unwrap();
    let at = |o: &Value, i: usize| (o["x"][i].as_f64().unwrap(), o["y"][i].as_f64().unwrap());
    let (cx, cy) = at(&first, cluster);
    let target = (cx + 300., cy - 200.);
    let start = json!({"x": first["x"], "y": first["y"], "r": first["r"], "moved": {"g1": {"x": target.0, "y": target.1}}});
    let response: Value = serde_json::from_str(&dispatch(
        &json!({"op": "overview", "graph": graph, "start": start}).to_string(),
    ))
    .unwrap();
    assert_eq!(response["ok"], true, "{response}");
    let moved = &response["overview"];
    let (mx, my) = at(moved, cluster);
    assert!(
        (mx - target.0).abs() < 0.02 && (my - target.1).abs() < 0.02,
        "held at the target: {mx}, {my}"
    );
    let before = (at(&first, child).0 - cx, at(&first, child).1 - cy);
    let after = (at(moved, child).0 - mx, at(moved, child).1 - my);
    assert!(
        (before.0 - after.0).abs() < 0.05 && (before.1 - after.1).abs() < 0.05,
        "the child moved along"
    );
    assert_eq!(moved["stats"]["overlaps"], 0);
}

#[test]
fn a_leaf_moved_out_of_its_cluster_recentres_the_cluster_and_stays_put() {
    // A leaf of g1 is moved far out: it stays at the target, and g1 is centred on its children again, so its disc is
    // about half the new span, not the distance from its old centre.
    let graph = hierarchical(3, 2, 4);
    let first = overview(&graph, 1);
    let ids: Vec<&str> = first["ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    let parent = |o: &Value, i: usize| o["parent"][i].as_i64().unwrap();
    let file = (0..ids.len())
        .find(|&i| {
            ids[i].starts_with("g1/")
                && parent(&first, i) >= 0
                && ids[parent(&first, i) as usize] == "g1"
        })
        .unwrap();
    let leaf = (0..ids.len())
        .find(|&i| parent(&first, i) == file as i64)
        .unwrap();
    let at = |o: &Value, i: usize| (o["x"][i].as_f64().unwrap(), o["y"][i].as_f64().unwrap());
    let (lx, ly) = at(&first, leaf);
    let target = (lx + 120., ly);
    let start = json!({"x": first["x"], "y": first["y"], "r": first["r"], "moved": {ids[leaf]: {"x": target.0, "y": target.1}}});
    let response: Value = serde_json::from_str(&dispatch(
        &json!({"op": "overview", "graph": graph, "start": start}).to_string(),
    ))
    .unwrap();
    assert_eq!(response["ok"], true, "{response}");
    let o = &response["overview"];
    let (mx, my) = at(o, leaf);
    assert!(
        (mx - target.0).abs() < 0.05 && (my - target.1).abs() < 0.05,
        "held at the target: {mx}, {my}"
    );
    let r = |i: usize| o["r"][i].as_f64().unwrap();
    let (fx, fy) = at(o, file);
    let from_old =
        ((target.0 - at(&first, file).0).powi(2) + (target.1 - at(&first, file).1).powi(2)).sqrt();
    assert!(
        r(file) < from_old,
        "centred again: radius {} against {from_old} from the old centre",
        r(file)
    );
    // The leaf lies in its cluster's disc.
    assert!(((mx - fx).powi(2) + (my - fy).powi(2)).sqrt() + r(leaf) <= r(file) + 0.05);
    assert_eq!(o["stats"]["overlaps"], 0);
}

/// Any `overview` request through `dispatch`, answered.
fn answer(request: &Value) -> Value {
    let response: Value = serde_json::from_str(&dispatch(&request.to_string())).unwrap();
    assert_eq!(response["ok"], true, "{response}");
    response["overview"].clone()
}

/// The error of a refused request.
fn refusal(request: &Value) -> String {
    let response: Value = serde_json::from_str(&dispatch(&request.to_string())).unwrap();
    assert_eq!(response["ok"], false, "{response}");
    response["error"].as_str().unwrap().to_string()
}

#[test]
fn edge_data_gives_every_edge_its_kind_and_weight_on_request() {
    let mut graph = hierarchical(3, 3, 5);
    let plain = answer(&json!({"op": "overview", "graph": graph}));
    for key in ["edgeKinds", "edgeKind", "edgeWeight", "continuation"] {
        assert!(plain.get(key).is_none(), "{key} only on request");
    }
    // `false` is the same as left out, byte for byte.
    let off = json!({"op": "overview", "graph": graph, "edgeData": false});
    assert_eq!(
        dispatch(&off.to_string()),
        dispatch(&json!({"op": "overview", "graph": graph}).to_string())
    );
    // Every edge counts 1: kinds only, and nothing else of the answer differs.
    let mut asked = answer(&json!({"op": "overview", "graph": graph, "edgeData": true}));
    assert_eq!(asked["edgeKinds"], json!(["call"]));
    let edges = asked["edges"].as_array().unwrap().len() / 2;
    assert_eq!(asked["edgeKind"], json!(vec![0; edges]));
    assert!(asked.get("edgeWeight").is_none(), "every edge counts 1");
    let o = asked.as_object_mut().unwrap();
    o.remove("edgeKinds");
    o.remove("edgeKind");
    assert_eq!(asked, plain);

    // Other kinds and weights; an unresolved edge first, which has no entry.
    let list = graph["edges"].as_array_mut().unwrap();
    for (k, e) in list.iter_mut().enumerate() {
        if k % 3 == 1 {
            e["kind"] = json!("import");
        }
        if k % 4 == 2 {
            e["weight"] = json!(k);
        }
    }
    list.insert(
        0,
        json!({"id": "first", "kind": "never", "source": "g0/f0/n0", "target": null, "weight": 9}),
    );
    let o = answer(&json!({"op": "overview", "graph": graph, "edgeData": true, "threads": 1}));
    assert_eq!(
        answer(&json!({"op": "overview", "graph": graph, "edgeData": true, "threads": 4})),
        o
    );
    let resolved: Vec<&Value> = graph["edges"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| !e["target"].is_null())
        .collect();
    assert_eq!(o["edgeKind"].as_array().unwrap().len(), resolved.len());
    assert_eq!(o["edgeWeight"].as_array().unwrap().len(), resolved.len());
    // The table in order of first use among the resolved edges.
    assert_eq!(o["edgeKinds"], json!(["call", "import"]));
    for (k, e) in resolved.iter().enumerate() {
        let kind = o["edgeKind"][k].as_u64().unwrap() as usize;
        assert_eq!(o["edgeKinds"][kind], e["kind"], "edge {k}");
        assert_eq!(
            o["edgeWeight"][k].as_u64().unwrap(),
            e["weight"].as_u64().unwrap_or(1).max(1),
            "edge {k}"
        );
        let source = o["edges"][2 * k].as_u64().unwrap() as usize;
        assert_eq!(o["ids"][source], e["source"], "edge {k}");
    }
    let typed: arqlet_model::Overview = serde_json::from_value(o.clone()).unwrap();
    assert_eq!(serde_json::to_value(&typed).unwrap(), o);
    assert!(
        refusal(&json!({"op": "overview", "graph": graph, "edgeData": "yes"})).contains("edgeData")
    );
}

#[test]
fn edge_data_covers_the_taps_of_a_bus_given_as_data_and_graphs_without_edges() {
    // The taps are edges of the overview that the input has no edge for.
    let graph = json!({"nodes": [
        {"id": "a", "kind": "ecu", "label": "a"}, {"id": "b", "kind": "ecu", "label": "b"}
    ], "edges": [{"id": "e", "kind": "wire", "source": "a", "target": "b", "weight": 3}],
    "buses": [{"id": "can", "taps": [
        {"edge": "t1", "node": "a", "toBus": true, "kind": "sends"},
        {"edge": "t2", "node": "b", "toBus": false, "kind": "receives"}
    ]}]});
    let o = answer(&json!({"op": "overview", "graph": graph, "edgeData": true}));
    assert_eq!(o["ids"], json!(["a", "b", "can"]));
    assert_eq!(o["edges"], json!([0, 1, 0, 2, 2, 1]));
    assert_eq!(o["edgeKinds"], json!(["wire", "sends", "receives"]));
    assert_eq!(o["edgeKind"], json!([0, 1, 2]));
    assert_eq!(o["edgeWeight"], json!([3, 1, 1]));
    // Asked for, the fields are there even without an edge.
    let empty = json!({"nodes": [{"id": "a", "kind": "n", "label": "a"}], "edges": []});
    let o = answer(&json!({"op": "overview", "graph": empty, "edgeData": true}));
    assert_eq!((&o["edgeKinds"], &o["edgeKind"]), (&json!([]), &json!([])));
    assert!(o.get("edgeWeight").is_none());
}

#[test]
fn edge_data_comes_as_columns_and_a_session_sends_it_again_when_asked_anew() {
    use arqlet_runtime::columns::Column;
    use arqlet_runtime::engine_session::EngineSession;
    let mut graph = hierarchical(3, 3, 5);
    graph["edges"][0]["weight"] = json!(4);
    let options = json!({"communities": 0});
    let whole =
        answer(&json!({"op": "overview", "graph": graph, "options": options, "edgeData": true}));
    let mut session = EngineSession::new();
    let first = session.run_value(
        &json!({"op": "overview", "graph": graph, "options": options,
        "edgeData": true, "columns": true}),
    );
    assert_eq!(first["ok"], true, "{first}");
    let meta = &first["overview"];
    assert_eq!(meta["edgeKinds"], whole["edgeKinds"]);
    assert!(meta.get("edgeKind").is_none() && meta.get("edgeWeight").is_none());
    let columns = session.overview_columns().unwrap().columns();
    let names: Vec<&str> = columns.iter().map(|(n, _)| *n).collect();
    assert_eq!(
        names,
        [
            "x",
            "y",
            "r",
            "parent",
            "kind",
            "edges",
            "links",
            "edgeKind",
            "edgeWeight"
        ]
    );
    for (name, column) in &columns[7..] {
        let Column::U32(c) = column else {
            panic!("{name}")
        };
        assert_eq!(json!(c), whole[name], "{name}");
    }
    assert_eq!(meta["columns"][8]["name"], "edgeWeight");
    // The same edge data as before: only what moves, which leaves the edges out.
    let next = json!({"op": "overview", "options": options, "start": {}, "positionsOnly": true,
        "edgeData": true});
    let moved = session.run_value(&next);
    assert_eq!(moved["overview"]["positionsOnly"], true, "{moved}");
    assert!(moved["overview"].get("edgeKind").is_none());
    // Without it: the host may drop what it had, so the whole overview.
    let mut without = next.clone();
    without["edgeData"] = json!(false);
    let o = session.run_value(&without);
    assert!(o["overview"]["ids"].is_array() && o["overview"].get("edgeKind").is_none());
    assert_eq!(
        session.run_value(&without)["overview"]["positionsOnly"],
        true
    );
    // Asked for again: whole, with the data.
    let o = session.run_value(&next);
    assert_eq!(o["overview"]["edgeKind"], whole["edgeKind"], "{o}");
    // A graph whose edges all count 1 has no weight column.
    let mut plain = EngineSession::new();
    plain.run_value(
        &json!({"op": "overview", "graph": hierarchical(2, 2, 3), "edgeData": true,
        "columns": true}),
    );
    assert_eq!(plain.overview_columns().unwrap().columns().len(), 8);
}

/// Four rings of 40 nodes, each a dense neighbourhood, one edge between neighbouring rings, no containers: communities.
fn rings() -> Value {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    for r in 0..4 {
        for i in 0..40 {
            nodes.push(
                json!({"id": format!("r{r}n{i}"), "kind": "fn", "label": "n", "parentId": null}),
            );
            for d in 1..4 {
                edges.push(json!({"id": format!("r{r}e{i}-{d}"), "kind": "call", "source": format!("r{r}n{i}"),
                    "target": format!("r{r}n{}", (i + d) % 40)}));
            }
        }
        edges.push(
            json!({"id": format!("x{r}"), "kind": "call", "source": format!("r{r}n0"),
            "target": format!("r{}n20", (r + 1) % 4)}),
        );
    }
    json!({"nodes": nodes, "edges": edges})
}

/// An overview's places by id, as a host keeps them for a later `start.nodes`.
fn places(o: &Value) -> Value {
    let map: serde_json::Map<String, Value> = o["ids"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .map(|(i, id)| {
            (
                id.as_str().unwrap().to_string(),
                json!({"x": o["x"][i], "y": o["y"][i], "r": o["r"][i]}),
            )
        })
        .collect();
    Value::Object(map)
}

fn at(o: &Value, id: &str) -> usize {
    o["ids"]
        .as_array()
        .unwrap()
        .iter()
        .position(|x| x == id)
        .unwrap_or_else(|| panic!("no {id}"))
}

/// How far node `id` lies from where it was.
fn moved_by(before: &Value, after: &Value, id: &str) -> f64 {
    let (a, b) = (at(before, id), at(after, id));
    let d = |k: &str| after[k][b].as_f64().unwrap() - before[k][a].as_f64().unwrap();
    d("x").hypot(d("y"))
}

/// Every node lies in its parent's disc, and no two discs of one container overlap.
fn assert_nested(o: &Value) {
    let n = o["ids"].as_array().unwrap().len();
    let f = |k: &str, i: usize| o[k][i].as_f64().unwrap();
    let distance = |i: usize, j: usize| (f("x", i) - f("x", j)).hypot(f("y", i) - f("y", j));
    assert_eq!(o["stats"]["overlaps"], 0);
    for i in 0..n {
        let p = o["parent"][i].as_i64().unwrap();
        if p >= 0 {
            assert!(
                distance(i, p as usize) + f("r", i) <= f("r", p as usize) + 0.05,
                "{} leaves its container",
                o["ids"][i]
            );
        }
        for j in i + 1..n {
            if o["parent"][j] == o["parent"][i] {
                assert!(
                    distance(i, j) >= f("r", i) + f("r", j) - 0.05,
                    "{} and {} overlap",
                    o["ids"][i],
                    o["ids"][j]
                );
            }
        }
    }
}

/// An answer without what became of the places.
fn without_continuation(mut o: Value) -> Value {
    o.as_object_mut().unwrap().remove("continuation");
    o
}

#[test]
fn places_by_id_continue_an_unchanged_graph_as_places_by_index_do() {
    let nested = hierarchical(5, 4, 8);
    let flat = rings();
    let mut bused = hierarchical(3, 2, 6);
    bused["buses"] = json!([{"id": "can", "taps": [
        {"edge": "t1", "node": "g0/f0/n1", "toBus": true, "kind": "sends"},
        {"edge": "t2", "node": "g1/f0/n1", "toBus": false, "kind": "receives"}]}]);
    for (graph, options) in [
        (&nested, json!({})),
        (&nested, json!({"communities": 3, "satellites": 0.5})),
        (
            &nested,
            json!({"chains": true, "groups": {"g0/f0": ["A"], "g0/f1": ["A"]}}),
        ),
        (&flat, json!({})),
        (&flat, json!({"communities": 30, "satellites": 0.3})),
        (&bused, json!({"busRows": true})),
    ] {
        let first = answer(&json!({"op": "overview", "graph": graph, "options": options}));
        // A cluster grown by the host, then back: both ways of giving the places answer the same.
        let by_index = json!({"x": first["x"], "y": first["y"], "r": first["r"]});
        let by_id = json!({"nodes": places(&first)});
        let grown = &first["ids"][first["ids"].as_array().unwrap().len() / 3];
        for settle in [false, true] {
            for radii in [json!({}), json!({grown.as_str().unwrap(): 60.0})] {
                let request = |mut start: Value| {
                    start["settle"] = json!(settle);
                    answer(
                        &json!({"op": "overview", "graph": graph, "options": options,
                        "radii": radii, "start": start, "threads": 4}),
                    )
                };
                let (index, id) = (request(by_index.clone()), request(by_id.clone()));
                let report = id["continuation"].clone();
                assert_eq!(report["anew"], 0, "{options}");
                assert_eq!(report["ignored"], json!([]), "{options}");
                assert_eq!(
                    report["continued"].as_u64().unwrap(),
                    first["stats"]["nodes"].as_u64().unwrap()
                        - first["stats"]["communities"].as_u64().unwrap(),
                    "{options}"
                );
                assert!(index.get("continuation").is_none());
                assert_eq!(
                    without_continuation(id),
                    index,
                    "{options} {settle} {radii}"
                );
            }
        }
        // No place at all: as without a start.
        let none = answer(
            &json!({"op": "overview", "graph": graph, "options": options,
            "start": {"nodes": {}}}),
        );
        assert_eq!(none["continuation"]["continued"], 0);
        assert_eq!(without_continuation(none), first, "{options}");
    }
}

#[test]
fn an_added_node_is_placed_in_its_cluster_and_the_rest_stays() {
    let graph = hierarchical(5, 4, 8);
    let first = answer(&json!({"op": "overview", "graph": graph}));
    let mut changed = graph.clone();
    changed["nodes"].as_array_mut().unwrap().insert(
        3,
        json!({"id": "new", "kind": "fn", "label": "new", "parentId": "g2/f1"}),
    );
    changed["edges"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id": "to-new", "kind": "call", "source": "g2/f1/n3", "target": "new"}));
    // A new container with two nodes of its own, which have no place either.
    for node in [
        json!({"id": "g1/new", "kind": "file", "label": "f", "parentId": "g1"}),
        json!({"id": "g1/new/a", "kind": "fn", "label": "a", "parentId": "g1/new"}),
        json!({"id": "g1/new/b", "kind": "fn", "label": "b", "parentId": "g1/new"}),
    ] {
        changed["nodes"].as_array_mut().unwrap().push(node);
    }
    for settle in [false, true] {
        let request = json!({"op": "overview", "graph": changed,
            "start": {"nodes": places(&first), "settle": settle}, "threads": 1});
        let o = answer(&request);
        let mut parallel = request.clone();
        parallel["threads"] = json!(8);
        assert_eq!(answer(&parallel), o);
        assert_nested(&o);
        let n = graph["nodes"].as_array().unwrap().len();
        assert_eq!(
            o["continuation"],
            json!({"continued": n, "anew": 4, "ignored": []})
        );
        // The new node is near the node it is linked to.
        let (new, linked) = (at(&o, "new"), at(&o, "g2/f1/n3"));
        let apart = (o["x"][new].as_f64().unwrap() - o["x"][linked].as_f64().unwrap())
            .hypot(o["y"][new].as_f64().unwrap() - o["y"][linked].as_f64().unwrap());
        assert!(apart < o["r"][at(&o, "g2/f1")].as_f64().unwrap(), "{apart}");
        // A group that took nothing new keeps its inside as it was.
        for id in graph["nodes"].as_array().unwrap() {
            let id = id["id"].as_str().unwrap();
            if id.starts_with("g4/") {
                let rel = |o: &Value, k: &str| {
                    o[k][at(o, id)].as_f64().unwrap() - o[k][at(o, "g4")].as_f64().unwrap()
                };
                assert!(
                    (rel(&first, "x") - rel(&o, "x")).abs() < 0.02
                        && (rel(&first, "y") - rel(&o, "y")).abs() < 0.02,
                    "{id}"
                );
            }
        }
        // The drawing moved little: no node further than a file is wide.
        let wide = 2. * first["r"][at(&first, "g2/f1")].as_f64().unwrap();
        let most = graph["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|node| moved_by(&first, &o, node["id"].as_str().unwrap()))
            .fold(0., f64::max);
        eprintln!("added, settle {settle}: moved at most {most:.2} (file {wide:.2})");
        assert!(most < wide, "settle {settle}: {most} >= {wide}");
    }
}

#[test]
fn a_removed_node_is_ignored_and_reported_and_the_rest_stays() {
    let graph = hierarchical(5, 4, 8);
    let first = answer(&json!({"op": "overview", "graph": graph}));
    let gone = ["g2/f1/n4", "g3/f2/n0"];
    let mut changed = graph.clone();
    changed["nodes"]
        .as_array_mut()
        .unwrap()
        .retain(|n| !gone.contains(&n["id"].as_str().unwrap()));
    changed["edges"].as_array_mut().unwrap().retain(|e| {
        ["source", "target"]
            .iter()
            .all(|k| e[k].as_str().is_none_or(|id| !gone.contains(&id)))
    });
    for settle in [false, true] {
        let o = answer(&json!({"op": "overview", "graph": changed,
            "start": {"nodes": places(&first), "settle": settle}}));
        assert_nested(&o);
        assert_eq!(
            o["continuation"],
            json!({"continued": changed["nodes"].as_array().unwrap().len(), "anew": 0,
                "ignored": gone})
        );
        let wide = 2. * first["r"][at(&first, "g2/f1")].as_f64().unwrap();
        let most = changed["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|node| moved_by(&first, &o, node["id"].as_str().unwrap()))
            .fold(0., f64::max);
        eprintln!("removed, settle {settle}: moved at most {most:.2} (file {wide:.2})");
        assert!(most < wide, "settle {settle}: {most} >= {wide}");
        // Without settling, a file that lost nothing keeps its nodes where they were.
        if !settle {
            for l in 0..8 {
                assert!(moved_by(&first, &o, &format!("g4/f0/n{l}")) < wide / 4.);
            }
        } else {
            // Settling, the file that lost a node settles again.
            let still = answer(&json!({"op": "overview", "graph": changed,
                "start": {"nodes": places(&first)}}));
            assert!(
                moved_by(&still, &o, "g2/f1/n3") > 0.02,
                "g2/f1 did not settle"
            );
        }
    }
}

/// The cluster of a node: its parent's id in the overview, or "" at the top level.
fn cluster_of(o: &Value, id: &str) -> String {
    match o["parent"][at(o, id)].as_i64().unwrap() {
        -1 => String::new(),
        p => o["ids"][p as usize].as_str().unwrap().to_string(),
    }
}

#[test]
fn clusters_keep_their_ids_where_the_graph_alone_would_name_others() {
    let graph = rings();
    let first = answer(&json!({"op": "overview", "graph": graph}));
    assert_eq!(first["stats"]["communities"], 4);
    // The first ring's nodes last: Louvain numbers the communities by their first node, so clustered anew the same ids
    // name other rings.
    let mut turned = graph.clone();
    turned["nodes"].as_array_mut().unwrap().rotate_left(40);
    let fresh = answer(&json!({"op": "overview", "graph": turned}));
    assert_ne!(cluster_of(&fresh, "r1n0"), cluster_of(&first, "r1n0"));
    let o = answer(&json!({"op": "overview", "graph": turned,
        "start": {"nodes": places(&first)}}));
    assert_eq!(
        o["continuation"],
        json!({"continued": 160, "anew": 0, "ignored": []})
    );
    for r in 0..4 {
        for i in 0..40 {
            let id = format!("r{r}n{i}");
            assert_eq!(cluster_of(&o, &id), cluster_of(&first, &id), "{id}");
            assert!(moved_by(&first, &o, &id) < 0.02, "{id}");
        }
        // Hundredths: a radius is measured again from rounded places.
        let cluster = cluster_of(&first, &format!("r{r}n0"));
        for k in ["x", "y", "r"] {
            let (was, is) = (&first[k][at(&first, &cluster)], &o[k][at(&o, &cluster)]);
            assert!(
                (was.as_f64().unwrap() - is.as_f64().unwrap()).abs() < 0.02,
                "{cluster}: {k} {was} {is}"
            );
        }
    }
}

#[test]
fn a_changed_community_keeps_its_cluster_and_takes_the_new_nodes_linked_to_it() {
    let graph = rings();
    let first = answer(&json!({"op": "overview", "graph": graph}));
    // Ring 1 loses half of its nodes, ring 2 gets six new ones, and r0n7 now calls only into ring 2.
    let gone: Vec<String> = (20..40).map(|i| format!("r1n{i}")).collect();
    let mut changed = graph.clone();
    changed["nodes"]
        .as_array_mut()
        .unwrap()
        .retain(|n| !gone.iter().any(|g| n["id"] == **g));
    let edges = changed["edges"].as_array_mut().unwrap();
    edges.retain(|e| {
        ["source", "target"]
            .iter()
            .all(|k| !gone.iter().any(|g| e[k] == **g))
    });
    for (k, e) in edges.iter_mut().enumerate() {
        for end in ["source", "target"] {
            if e[end] == "r0n7" {
                let other = if end == "source" { "target" } else { "source" };
                e[other] = json!(format!("r2n{}", k % 40));
            }
        }
    }
    for i in 0..6 {
        let id = format!("new{i}");
        for d in 0..3 {
            changed["edges"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id": format!("{id}-{d}"),
                "kind": "call", "source": id, "target": format!("r2n{}", i * 5 + d)}));
        }
        changed["nodes"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id": id, "kind": "fn", "label": id, "parentId": null}));
    }
    // Clustered anew, r0n7 goes with ring 2.
    let fresh = answer(&json!({"op": "overview", "graph": changed}));
    assert_eq!(cluster_of(&fresh, "r0n7"), cluster_of(&fresh, "r2n0"));
    for settle in [false, true] {
        let request = json!({"op": "overview", "graph": changed,
            "start": {"nodes": places(&first), "settle": settle}, "threads": 3});
        let o = answer(&request);
        assert_nested(&o);
        assert_eq!(
            o["continuation"],
            json!({"continued": 140, "anew": 6, "ignored": sorted(&gone)})
        );
        // The clusters are the earlier ones: r0n7 stays where it was, the new nodes join the ring they call.
        assert_eq!(o["stats"]["communities"], 4);
        assert_eq!(cluster_of(&o, "r0n7"), cluster_of(&first, "r0n7"));
        assert_eq!(cluster_of(&o, "r1n0"), cluster_of(&first, "r1n0"));
        for i in 0..6 {
            assert_eq!(
                cluster_of(&o, &format!("new{i}")),
                cluster_of(&first, "r2n0")
            );
        }
        let radius = |o: &Value, id: &str| o["r"][at(o, &cluster_of(o, id))].as_f64().unwrap();
        if settle {
            // The smaller cluster closes up.
            assert!(radius(&o, "r1n0") < 0.9 * radius(&first, "r1n0"));
        }
        let wide = radius(&first, "r2n0");
        let most = changed["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n["id"].as_str().unwrap())
            .filter(|id| !id.starts_with("new"))
            .map(|id| moved_by(&first, &o, id))
            .fold(0., f64::max);
        eprintln!("changed community, settle {settle}: moved at most {most:.2} (community radius {wide:.2})");
        assert!(most < wide, "settle {settle}: {most} >= {wide}");
        // Continuing that answer by id, unchanged, finds the same clusters and moves nothing.
        let again = answer(&json!({"op": "overview", "graph": changed,
            "start": {"nodes": places(&o)}}));
        assert_eq!(again["ids"], o["ids"]);
        assert_eq!(again["parent"], o["parent"]);
        for id in o["ids"].as_array().unwrap() {
            assert!(moved_by(&o, &again, id.as_str().unwrap()) < 0.05, "{id}");
        }
    }
}

/// Ids as `continuation.ignored` lists them.
fn sorted(ids: &[String]) -> Vec<String> {
    let mut ids = ids.to_vec();
    ids.sort();
    ids
}

#[test]
fn clusters_are_dissolved_made_or_left_out_as_the_graph_and_the_options_say() {
    let graph = rings();
    let first = answer(&json!({"op": "overview", "graph": graph}));
    // All of ring 3 but one node: its cluster is dissolved, the node stays where it was.
    let mut changed = graph.clone();
    changed["nodes"]
        .as_array_mut()
        .unwrap()
        .retain(|n| n["id"] == "r3n0" || !n["id"].as_str().unwrap().starts_with("r3"));
    changed["edges"].as_array_mut().unwrap().retain(|e| {
        ["source", "target"].iter().all(|k| {
            let id = e[k].as_str().unwrap();
            id == "r3n0" || !id.starts_with("r3")
        })
    });
    let o = answer(&json!({"op": "overview", "graph": changed,
        "start": {"nodes": places(&first)}}));
    assert_nested(&o);
    assert_eq!(o["stats"]["communities"], 3);
    assert_eq!(cluster_of(&o, "r3n0"), "");
    assert!(moved_by(&first, &o, "r3n0") < 0.05);
    // Communities switched off: none is kept, and the nodes stay where they were.
    let o = answer(
        &json!({"op": "overview", "graph": graph, "options": {"communities": 0},
        "start": {"nodes": places(&first)}}),
    );
    assert_eq!(o["stats"]["communities"], 0);
    assert_eq!(o["continuation"]["continued"], 160);
    for i in 0..40 {
        assert!(moved_by(&first, &o, &format!("r1n{i}")) < 0.05);
    }
    // Without the places of the earlier clusters, the graph is clustered anew and those clusters laid out anew.
    let mut leaves = places(&first);
    leaves
        .as_object_mut()
        .unwrap()
        .retain(|id, _| !id.starts_with("community:"));
    let o = answer(&json!({"op": "overview", "graph": graph, "start": {"nodes": leaves}}));
    assert_nested(&o);
    assert_eq!(
        o["continuation"],
        json!({"continued": 0, "anew": 160, "ignored": []})
    );
    // A new container with many children is clustered, whatever else is kept.
    let mut changed = graph.clone();
    changed["nodes"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id": "box", "kind": "dir", "label": "box", "parentId": null}));
    for node in rings()["nodes"].as_array().unwrap() {
        let id = format!("box/{}", node["id"].as_str().unwrap());
        changed["nodes"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id": id, "kind": "fn", "label": id, "parentId": "box"}));
    }
    for edge in rings()["edges"].as_array().unwrap() {
        let mut e = edge.clone();
        for k in ["id", "source", "target"] {
            e[k] = json!(format!("box/{}", e[k].as_str().unwrap()));
        }
        changed["edges"].as_array_mut().unwrap().push(e);
    }
    let o = answer(&json!({"op": "overview", "graph": changed,
        "start": {"nodes": places(&first)}}));
    assert_nested(&o);
    assert_eq!(o["stats"]["communities"], 8);
    assert!(cluster_of(&o, "box/r0n0").starts_with("community:box:"));
    assert_eq!(cluster_of(&o, "r0n0"), cluster_of(&first, "r0n0"));
}

#[test]
fn a_node_that_changed_its_container_is_placed_anew() {
    let graph = hierarchical(4, 3, 8);
    let first = answer(&json!({"op": "overview", "graph": graph}));
    let mut changed = graph.clone();
    for node in changed["nodes"].as_array_mut().unwrap() {
        if node["id"] == "g0/f0/n5" {
            node["parentId"] = json!("g3/f2");
        }
    }
    let o = answer(&json!({"op": "overview", "graph": changed,
        "start": {"nodes": places(&first)}}));
    assert_nested(&o);
    assert_eq!(o["continuation"]["anew"], 1);
    // Its new container grew by one node, not to where the node was.
    let r = |o: &Value| o["r"][at(o, "g3/f2")].as_f64().unwrap();
    assert!(r(&o) < 1.5 * r(&first), "{} -> {}", r(&first), r(&o));
    // Moved up, to the group around its file or to the top level, it starts where it was, inside its old file's disc:
    // the two part.
    for parent in [json!("g0"), Value::Null] {
        let mut changed = graph.clone();
        for node in changed["nodes"].as_array_mut().unwrap() {
            if node["id"] == "g0/f0/n5" {
                node["parentId"] = parent.clone();
            }
        }
        for settle in [false, true] {
            let o = answer(&json!({"op": "overview", "graph": changed,
                "start": {"nodes": places(&first), "settle": settle}}));
            assert_nested(&o);
        }
    }
    // A whole file moved to another group keeps its inside.
    let mut changed = graph.clone();
    for node in changed["nodes"].as_array_mut().unwrap() {
        if node["id"] == "g0/f1" {
            node["parentId"] = json!("g2");
        }
    }
    let o = answer(&json!({"op": "overview", "graph": changed,
        "start": {"nodes": places(&first)}}));
    assert_nested(&o);
    assert_eq!(o["continuation"]["anew"], 1);
    for l in 0..8 {
        let id = format!("g0/f1/n{l}");
        let rel = |o: &Value, k: &str| {
            o[k][at(o, &id)].as_f64().unwrap() - o[k][at(o, "g0/f1")].as_f64().unwrap()
        };
        assert!((rel(&first, "x") - rel(&o, "x")).abs() < 0.02, "{id}");
        assert!((rel(&first, "y") - rel(&o, "y")).abs() < 0.02, "{id}");
    }
}

#[test]
fn places_by_id_are_checked_and_work_in_a_session() {
    use arqlet_runtime::engine_session::EngineSession;
    let graph = hierarchical(3, 3, 5);
    let first = answer(&json!({"op": "overview", "graph": graph}));
    let request = |start: Value| json!({"op": "overview", "graph": graph, "start": start});
    for (start, needle) in [
        (
            json!({"nodes": places(&first), "x": first["x"]}),
            "not both",
        ),
        (json!({"nodes": {}, "r": []}), "not both"),
        (json!({"nodes": [1]}), "must be an object"),
        (json!({"nodes": {"g0": {"x": 1, "y": 2}}}), "g0 needs"),
        (
            json!({"nodes": {"g0": {"x": 1, "y": 2, "r": -1}}}),
            "g0 needs",
        ),
        (
            json!({"nodes": {}, "moved": {"g0": {"x": 1, "y": 2}}}),
            "no earlier place",
        ),
    ] {
        let error = refusal(&request(start.clone()));
        assert!(error.contains(needle), "{start}: {error}");
    }
    // A moved node with a place stays where it was put, as with places by index.
    let id = "g1/f1/n2";
    let to = json!({"x": first["x"][at(&first, id)].as_f64().unwrap() + 5.,
        "y": first["y"][at(&first, id)]});
    let moved = |mut start: Value| {
        start["moved"] = json!({id: to});
        answer(&request(start))
    };
    assert_eq!(
        without_continuation(moved(json!({"nodes": places(&first)}))),
        moved(json!({"x": first["x"], "y": first["y"], "r": first["r"]}))
    );
    // In a session, with the graph it has and with a changed one: as `dispatch` answers, in whole or what moves.
    let mut session = EngineSession::new();
    session.run_value(&json!({"op": "overview", "graph": graph}));
    let same = session.run_value(
        &json!({"op": "overview", "start": {"nodes": places(&first)},
        "positionsOnly": true}),
    );
    assert_eq!(same["overview"]["positionsOnly"], true, "{same}");
    let direct = answer(&request(json!({"nodes": places(&first)})));
    assert_eq!(same["overview"]["continuation"], direct["continuation"]);
    assert_eq!(same["overview"]["x"], direct["x"]);
    let _: arqlet_model::OverviewPositions =
        serde_json::from_value(same["overview"].clone()).unwrap();
    let columnar = session.run_value(&json!({"op": "overview", "columns": true,
        "positionsOnly": true, "start": {"nodes": places(&first)}}));
    assert_eq!(columnar["overview"]["continuation"], direct["continuation"]);
    let smaller = hierarchical(2, 3, 5);
    let next = session.run_value(&json!({"op": "overview", "graph": smaller,
        "start": {"nodes": places(&first)}}));
    assert_eq!(
        next["overview"],
        answer(&json!({"op": "overview", "graph": smaller, "start": {"nodes": places(&first)}}))
    );
    assert!(!next["overview"]["continuation"]["ignored"]
        .as_array()
        .unwrap()
        .is_empty());
    // The session then continues from that answer without positions.
    let after = session.run_value(&json!({"op": "overview", "start": {}}));
    assert_eq!(after["ok"], true, "{after}");
    // Places keep their clusters where the graph alone would make others; the session builds on them while requests
    // have a start, and clusters the graph anew without one.
    let flat = rings();
    let mut turned = flat.clone();
    turned["nodes"].as_array_mut().unwrap().rotate_left(40);
    let first = session.run_value(&json!({"op": "overview", "graph": flat}));
    let from = places(&first["overview"]);
    let kept = session.run_value(&json!({"op": "overview", "graph": turned,
        "start": {"nodes": from}}));
    let moved = session.run_value(&json!({"op": "overview", "start": {}, "positionsOnly": true}));
    assert_eq!(moved["overview"]["positionsOnly"], true, "{moved}");
    for (i, x) in moved["overview"]["x"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        let was = kept["overview"]["x"][i].as_f64().unwrap();
        assert!((x.as_f64().unwrap() - was).abs() < 0.05, "node {i}");
    }
    let anew = session.run_value(&json!({"op": "overview", "positionsOnly": true}));
    assert_eq!(
        anew["overview"],
        answer(&json!({"op": "overview", "graph": turned}))
    );
    // The same graph, with places that name the other clusters: whole, since the host's ids and parents change.
    let other = session.run_value(&json!({"op": "overview", "positionsOnly": true,
        "start": {"nodes": from}}));
    assert!(other["overview"]["parent"].is_array(), "{other}");
    assert_eq!(other["overview"]["parent"], kept["overview"]["parent"]);
    // A typed request with places is a request the operation takes.
    let typed = arqlet_model::OverviewRequest {
        graph: Some(serde_json::from_value(graph).unwrap()),
        start: Some(arqlet_model::OverviewStart {
            nodes: Some(
                [(
                    "g0".to_string(),
                    arqlet_model::OverviewPlace {
                        x: 0.,
                        y: 0.,
                        r: 50.,
                    },
                )]
                .into(),
            ),
            ..Default::default()
        }),
        edge_data: Some(true),
        ..Default::default()
    };
    let mut v = serde_json::to_value(&typed).unwrap();
    v["op"] = json!("overview");
    let o = answer(&v);
    let typed: arqlet_model::Overview = serde_json::from_value(o.clone()).unwrap();
    assert_eq!(serde_json::to_value(&typed).unwrap(), o);
}

/// Random changes of a graph, revision after revision: nodes removed (with what they hold), added, moved to another
/// container, edges rewired.
fn changed_at_random(graph: &Value, next: &mut impl FnMut(usize) -> usize, step: usize) -> Value {
    let mut nodes = graph["nodes"].as_array().unwrap().clone();
    let mut edges = graph["edges"].as_array().unwrap().clone();
    let holds = |nodes: &[Value], id: &Value| nodes.iter().any(|n| n["parentId"] == *id);
    for _ in 0..next(4) {
        let mut gone = vec![nodes[next(nodes.len())]["id"].clone()];
        let mut at = 0;
        while at < gone.len() {
            let under: Vec<Value> = nodes
                .iter()
                .filter(|n| n["parentId"] == gone[at])
                .map(|n| n["id"].clone())
                .collect();
            gone.extend(under);
            at += 1;
        }
        if gone.len() < nodes.len() / 3 {
            nodes.retain(|n| !gone.contains(&n["id"]));
            edges.retain(|e| !gone.contains(&e["source"]) && !gone.contains(&e["target"]));
        }
    }
    for k in 0..next(8) {
        let beside = nodes[next(nodes.len())].clone();
        let id = format!("s{step}-{k}");
        nodes.push(json!({"id": id, "kind": "fn", "label": id, "parentId": beside["parentId"]}));
        if next(3) > 0 {
            edges.push(
                json!({"id": format!("{id}-e"), "kind": "call", "source": id, "target": beside["id"]}),
            );
        }
    }
    // A leaf to any other node (which may turn into a container) or to the top level.
    for _ in 0..next(4) {
        let leaf = next(nodes.len());
        if holds(&nodes, &nodes[leaf]["id"].clone()) {
            continue;
        }
        let to = next(nodes.len());
        let parent = if to == leaf || next(4) == 0 {
            Value::Null
        } else {
            nodes[to]["id"].clone()
        };
        nodes[leaf]["parentId"] = parent;
    }
    for _ in 0..next(30) {
        let (e, to) = (next(edges.len()), next(nodes.len()));
        edges[e]["target"] = nodes[to]["id"].clone();
    }
    json!({"nodes": nodes, "edges": edges})
}

#[test]
fn a_graph_changed_at_random_continues_without_overlaps() {
    let mut seed = 11u64;
    let mut next = move |below: usize| {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (seed >> 33) as usize % below.max(1)
    };
    let cases = [
        (hierarchical(3, 3, 8), json!({})),
        (
            hierarchical(3, 3, 8),
            json!({"communities": 4, "satellites": 0.5}),
        ),
        (
            hierarchical(2, 4, 12),
            json!({"communities": 6, "chains": true}),
        ),
        (rings(), json!({"communities": 30})),
        (rings(), json!({"communities": 12, "satellites": 0.3})),
    ];
    for (case, (mut graph, options)) in cases.into_iter().enumerate() {
        let mut last = answer(&json!({"op": "overview", "graph": graph, "options": options}));
        for step in 0..8 {
            graph = changed_at_random(&graph, &mut next, step);
            let request = json!({"op": "overview", "graph": graph, "options": options,
                "start": {"nodes": places(&last), "settle": step % 2 == 1}, "threads": 1});
            let o = answer(&request);
            let mut parallel = request.clone();
            parallel["threads"] = json!(4);
            assert_eq!(answer(&parallel), o, "case {case}, step {step}");
            assert_nested(&o);
            let report = &o["continuation"];
            assert_eq!(
                report["continued"].as_u64().unwrap() + report["anew"].as_u64().unwrap(),
                graph["nodes"].as_array().unwrap().len() as u64,
                "case {case}, step {step}"
            );
            last = o;
        }
    }
}
