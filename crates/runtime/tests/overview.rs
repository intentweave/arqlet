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
