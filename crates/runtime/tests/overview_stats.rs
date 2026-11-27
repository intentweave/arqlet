//! The inner-edge share and modularity per container of an overview, on request (`containerStats`, #236, ADR 0018
//! addendum of 2026-11-28), on small graphs computed by hand.
use arqlet_runtime::dispatch;
use arqlet_runtime::engine_session::EngineSession;
use serde_json::{json, Value};

fn answer(request: &Value) -> Value {
    let response: Value = serde_json::from_str(&dispatch(&request.to_string())).unwrap();
    assert_eq!(response["ok"], true, "{response}");
    response["overview"].clone()
}

fn node(id: &str, parent: Option<&str>) -> Value {
    json!({"id": id, "kind": "n", "label": id, "parentId": parent})
}

fn edge(id: &str, source: &str, target: Option<&str>, weight: Option<u32>) -> Value {
    let mut e = json!({"id": id, "kind": "e", "source": source, "target": target});
    if let Some(w) = weight {
        e["weight"] = json!(w);
    }
    e
}

fn numbers(v: &Value) -> Vec<f64> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_f64().unwrap())
        .collect()
}

fn assert_close(actual: &Value, expected: &[f64], what: &str) {
    let actual = numbers(actual);
    assert_eq!(actual.len(), expected.len(), "{what}: {actual:?}");
    for (a, e) in actual.iter().zip(expected) {
        assert!(
            (a - e).abs() < 1e-12,
            "{what}: {actual:?} against {expected:?}"
        );
    }
}

/// Two triangles in containers `A` and `B`, joined by one bridge.
fn two_cliques() -> Value {
    let mut nodes = vec![node("A", None), node("B", None)];
    let mut edges = Vec::new();
    for c in ["a", "b"] {
        let parent = c.to_uppercase();
        for i in 1..=3 {
            nodes.push(node(&format!("{c}{i}"), Some(&parent)));
        }
        for (s, t) in [(1, 2), (2, 3), (3, 1)] {
            edges.push(edge(
                &format!("{c}{s}{t}"),
                &format!("{c}{s}"),
                Some(&format!("{c}{t}")),
                None,
            ));
        }
    }
    edges.push(edge("bridge", "a1", Some("b1"), None));
    json!({"nodes": nodes, "edges": edges})
}

#[test]
fn two_cliques_with_a_bridge() {
    let o = answer(&json!({"op": "overview", "graph": two_cliques(),
        "options": {"communities": 0}, "containerStats": true}));
    let s = &o["containerStats"];
    // m = 7; A: in 3, deg 2·3 + 1 = 7, touching 4, share 3/4, term 3/7 - (7/14)^2 = 5/28; B the same.
    assert_eq!(s["weight"], 7);
    assert_eq!(s["container"], json!([-1, 0, 1]));
    assert_eq!(s["depth"], json!([-1, 0, 0]));
    assert_eq!(s["inner"], json!([7.0, 3.0, 3.0]));
    assert_eq!(s["touching"], json!([7.0, 4.0, 4.0]));
    assert_close(&s["share"], &[1., 0.75, 0.75], "share");
    assert_close(&s["modularity"], &[0., 5. / 28., 5. / 28.], "modularity");
    assert_close(&s["depthModularity"], &[5. / 14.], "depthModularity");
}

/// `P` holds the container `C` (with `c1`, `c2`) and `x`; `y` is at the top level. Parallel edges with a weight, a
/// self-loop, an edge to the container `C` itself, one from `P` to a node below it, and an unresolved edge.
fn nested() -> Value {
    json!({
        "nodes": [node("P", None), node("C", Some("P")), node("c1", Some("C")), node("c2", Some("C")),
            node("x", Some("P")), node("y", None)],
        "edges": [
            edge("heavy", "c1", Some("c2"), Some(2)),
            edge("parallel", "c2", Some("c1"), None),
            edge("loop", "c1", Some("c1"), None),
            edge("to-container", "C", Some("x"), None),
            edge("down", "P", Some("c2"), None),
            edge("across", "x", Some("y"), None),
            edge("open", "c1", None, Some(5)),
        ],
    })
}

#[test]
fn self_loops_parallel_edges_edges_of_containers_and_nested_depths() {
    let o = answer(&json!({"op": "overview", "graph": nested(),
        "options": {"communities": 0}, "containerStats": true}));
    let s = &o["containerStats"];
    // m = 2 + 1 + 1 + 1 + 1 + 1 = 7 (the unresolved edge left out).
    // C: in = 2 + 1 + 1 (loop once) = 4; deg = 4 + 2 + 2 (loop twice) + 1 (to x) + 1 (from P) = 10; touching 6.
    // P: in = 4 + 1 (C–x) + 1 (P–c2, inside P's block) = 6; deg = 13 (x–y leaves); touching 7.
    assert_eq!(s["weight"], 7);
    assert_eq!(s["container"], json!([-1, 0, 1]));
    assert_eq!(s["depth"], json!([-1, 0, 1]));
    assert_eq!(s["inner"], json!([7.0, 6.0, 4.0]));
    assert_eq!(s["touching"], json!([7.0, 7.0, 6.0]));
    assert_close(&s["share"], &[1., 6. / 7., 4. / 6.], "share");
    // P: 6/7 - (13/14)^2 = -1/196; C: 4/7 - (10/14)^2 = 3/49.
    assert_close(&s["modularity"], &[0., -1. / 196., 3. / 49.], "modularity");
    // Depth 0: P and y (deg 1): -1/196 - 1/196. Depth 1: C (12/196), x (deg 2: -4/196), and P (own deg 1: -1/196) and
    // y (-1/196) as blocks of their own: 6/196.
    assert_close(
        &s["depthModularity"],
        &[-2. / 196., 6. / 196.],
        "depthModularity",
    );
}

#[test]
fn given_groups_are_containers_with_their_stats() {
    let graph = json!({
        "nodes": [node("n1", None), node("n2", None), node("n3", None), node("n4", None)],
        "edges": [edge("e12", "n1", Some("n2"), None), edge("e34", "n3", Some("n4"), None),
            edge("e23", "n2", Some("n3"), None)],
    });
    let o = answer(
        &json!({"op": "overview", "graph": graph, "containerStats": true,
        "options": {"communities": 0, "groups": {"n1": ["G"], "n2": ["G"], "n3": ["H"], "n4": ["H"]}}}),
    );
    let s = &o["containerStats"];
    // m = 3; each group: in 1, deg 3, touching 2, share 1/2, term 1/3 - (3/6)^2 = 1/12.
    assert_eq!(s["weight"], 3);
    let containers: Vec<i64> = s["container"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_i64().unwrap())
        .collect();
    assert_eq!(containers.len(), 3);
    assert_eq!(containers[0], -1);
    for (k, &c) in containers.iter().enumerate().skip(1) {
        let members: Vec<&str> = o["parent"]
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
            .filter(|(_, p)| p.as_i64() == Some(c))
            .map(|(i, _)| o["ids"][i].as_str().unwrap())
            .collect();
        assert_eq!(members.len(), 2, "{members:?}");
        assert_eq!(s["inner"][k], 1.0);
        assert_eq!(s["touching"][k], 2.0);
    }
    assert_close(&s["share"], &[1., 0.5, 0.5], "share");
    assert_close(&s["modularity"], &[0., 1. / 12., 1. / 12.], "modularity");
    assert_close(&s["depthModularity"], &[1. / 6.], "depthModularity");
}

#[test]
fn a_graph_without_edges_or_containers_has_zeros() {
    let graph = json!({"nodes": [node("a", None), node("b", None)], "edges": []});
    let o = answer(
        &json!({"op": "overview", "graph": graph, "containerStats": true,
        "options": {"communities": 0}}),
    );
    assert_eq!(
        o["containerStats"],
        json!({"weight": 0, "container": [-1], "depth": [-1], "inner": [0.0], "touching": [0.0],
            "share": [0.0], "modularity": [0.0], "depthModularity": []})
    );
    let refused: Value = serde_json::from_str(&dispatch(
        &json!({"op": "overview", "graph": graph, "containerStats": 1}).to_string(),
    ))
    .unwrap();
    assert_eq!(
        refused["error"],
        "Overview containerStats must be true or false"
    );
}

/// Without the flag the answer is the one it was (no `containerStats`), and with it the rest stays the same.
#[test]
fn the_stats_are_only_added_and_only_on_request() {
    let graph = nested();
    let plain = answer(&json!({"op": "overview", "graph": graph}));
    assert!(plain.get("containerStats").is_none());
    let off = answer(&json!({"op": "overview", "graph": graph, "containerStats": false}));
    assert_eq!(off, plain);
    let mut with = answer(&json!({"op": "overview", "graph": graph, "containerStats": true}));
    with.as_object_mut().unwrap().remove("containerStats");
    assert_eq!(with, plain);
    let typed: arqlet_model::Overview = serde_json::from_value(answer(
        &json!({"op": "overview", "graph": graph, "containerStats": true}),
    ))
    .expect("the contract");
    assert!(typed.container_stats.is_some());
    let request = arqlet_model::OverviewRequest {
        container_stats: Some(true),
        ..Default::default()
    };
    let mut v = serde_json::to_value(&request).unwrap();
    v["graph"] = graph.clone();
    v["op"] = json!("overview");
    assert!(answer(&v)["containerStats"].is_object());
}

/// A session answers as `dispatch` does; a positions-only continuation carries no stats while the flag is as it was,
/// and answers the whole overview when the flag changed.
#[test]
fn a_session_answers_the_stats_as_dispatch_does() {
    let graph = two_cliques();
    let request = json!({"op": "overview", "graph": graph, "containerStats": true,
        "options": {"communities": 0}});
    let mut session = EngineSession::new();
    let first = session.run_value(&request);
    assert_eq!(first["overview"], answer(&request));
    // The same flag, positions only: no stats, the host keeps them.
    let next = json!({"op": "overview", "containerStats": true, "options": {"communities": 0},
        "start": {}, "positionsOnly": true});
    let moved = session.run_value(&next);
    assert_eq!(moved["overview"]["positionsOnly"], true, "{moved}");
    assert!(moved["overview"].get("containerStats").is_none());
    // Without the flag after one with it: the whole overview, without stats.
    let without = session.run_value(
        &json!({"op": "overview", "options": {"communities": 0}, "start": {}, "positionsOnly": true}),
    );
    assert!(without["overview"]["ids"].is_array(), "{without}");
    assert!(without["overview"].get("containerStats").is_none());
    // With it again: the whole overview with the stats, as dispatch answers them.
    let again = session.run_value(&next);
    assert!(again["overview"]["ids"].is_array(), "{again}");
    assert_eq!(
        again["overview"]["containerStats"],
        first["overview"]["containerStats"]
    );
}

/// The columnar answer carries the stats' arrays as columns named `containerStats.<field>`, equal to the JSON answer's;
/// `weight` stays in the JSON.
#[test]
fn the_stats_come_as_columns() {
    use arqlet_runtime::columns::Column;
    let request = json!({"op": "overview", "graph": nested(), "containerStats": true,
        "options": {"communities": 0}});
    let whole = answer(&request);
    let mut columnar = EngineSession::new();
    let mut asked = request.clone();
    asked["columns"] = json!(true);
    let meta = columnar.run_value(&asked)["overview"].clone();
    assert_eq!(meta["containerStats"], json!({"weight": 7}));
    let columns = columnar.overview_columns().unwrap();
    let mut seen = Vec::new();
    for (name, column) in columns.columns() {
        let Some(field) = name.strip_prefix("containerStats.") else {
            continue;
        };
        seen.push(field);
        let values = match column {
            Column::F64(c) => json!(c),
            Column::I32(c) => json!(c),
            other => panic!("{name}: {:?}", other.raw().1),
        };
        assert_eq!(values, whole["containerStats"][field], "{name}");
    }
    assert_eq!(
        seen,
        [
            "container",
            "depth",
            "inner",
            "touching",
            "share",
            "modularity",
            "depthModularity"
        ]
    );
}

/// The stats are of the hierarchy alone: the same for any thread count, and with added communities they cover those
/// too.
#[test]
fn the_stats_are_the_same_for_any_thread_count_and_cover_communities() {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    for c in 0..3 {
        for i in 0..12 {
            nodes.push(node(&format!("k{c}n{i}"), None));
            for j in 0..i {
                if (i + j) % 3 != 0 {
                    edges.push(edge(
                        &format!("k{c}e{i}-{j}"),
                        &format!("k{c}n{i}"),
                        Some(&format!("k{c}n{j}")),
                        None,
                    ));
                }
            }
        }
        edges.push(edge(
            &format!("bridge{c}"),
            &format!("k{c}n0"),
            Some(&format!("k{}n1", (c + 1) % 3)),
            None,
        ));
    }
    let graph = json!({"nodes": nodes, "edges": edges});
    let ask = |threads: usize| {
        answer(
            &json!({"op": "overview", "graph": graph, "containerStats": true,
            "options": {"communities": 10}, "threads": threads}),
        )
    };
    let one = ask(1);
    assert_eq!(one, ask(4));
    let s = &one["containerStats"];
    assert!(
        one["stats"]["communities"].as_u64().unwrap() >= 3,
        "{}",
        one["stats"]
    );
    // Every community holds far more inside than it shares, and the depth-0 partition is clearly modular.
    for share in &numbers(&s["share"])[1..] {
        assert!(*share > 0.9, "{s}");
    }
    assert!(numbers(&s["depthModularity"])[0] > 0.5, "{s}");
}
