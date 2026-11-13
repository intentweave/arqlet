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
            ticks: Some(50),
            groups: Some([("g0/f0".to_string(), vec!["A".to_string()])].into()),
            hubs: Some(arqlet_model::CommunityHubs {
                min_links: Some(10),
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
