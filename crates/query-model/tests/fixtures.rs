//! The adapter against the independent oracle (ADR 0021, M5a): every supported case of `examples/cypher/` is run over a
//! `ModelStore` built from the real `arqlet_model::Graph`, and its rows are compared with the oracle's. The same cases run
//! in `arqlet-query-cypher` over a store made of the raw JSON; passing here too shows that the adapter agrees with it.
use arqlet_model::Graph;
use arqlet_query::{Limits, Value};
use arqlet_query_cypher::run;
use arqlet_query_model::{ModelStore, Payload};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

const MIN_PASSING: usize = 40;

fn fixture(path: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/cypher")
        .join(path);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

fn to_json(store: &ModelStore, v: &Value) -> serde_json::Value {
    match v {
        Value::Null => serde_json::Value::Null,
        Value::Bool(b) => serde_json::json!(b),
        Value::Int(i) => serde_json::json!(i),
        Value::Float(f) => serde_json::json!(f),
        Value::String(s) => serde_json::json!(s),
        Value::List(items) => {
            serde_json::Value::Array(items.iter().map(|i| to_json(store, i)).collect())
        }
        Value::Node(n) => serde_json::json!({ "node": store.node_id(*n) }),
        Value::Edge(e) => serde_json::json!({ "edge": store.edge_id(*e) }),
    }
}

fn canonical(rows: &[serde_json::Value], ordered: bool) -> Vec<String> {
    let mut v: Vec<String> = rows.iter().map(|r| r.to_string()).collect();
    if !ordered {
        v.sort();
    }
    v
}

fn graph(name: &str) -> Graph {
    serde_json::from_str(&fixture(&format!("graphs/{name}.json")))
        .unwrap_or_else(|e| panic!("graph {name} is not an arqlet-model graph: {e}"))
}

fn payload(name: &str) -> Payload {
    let json: serde_json::Value =
        serde_json::from_str(&fixture(&format!("payloads/{name}.json"))).unwrap();
    Payload::from_json(&json).unwrap()
}

#[test]
fn the_supported_cases_get_the_oracles_rows_over_the_real_model() {
    let mut passing = 0;
    let mut failures = Vec::new();
    for line in fixture("cases.jsonl").lines() {
        let case: serde_json::Value = serde_json::from_str(line).unwrap();
        if case["status"] != "supported" {
            continue;
        }
        let (id, query) = (
            case["id"].as_str().unwrap(),
            case["query"].as_str().unwrap(),
        );
        let g = graph(case["graph"].as_str().unwrap());
        let p = case["payload"].as_str().map(payload).unwrap_or_default();
        let store = ModelStore::new(&g, &p);
        let ordered = case["ordered"].as_bool().unwrap_or(false);
        let expected = case["rows"].as_array().unwrap();
        // Several runs give the same rows in the same order: nothing depends on a hash order or on the run.
        let first = run(query, &store, &BTreeMap::new(), Limits::default());
        for _ in 0..4 {
            assert_eq!(
                format!("{first:?}"),
                format!(
                    "{:?}",
                    run(query, &store, &BTreeMap::new(), Limits::default())
                ),
                "{id} is not deterministic"
            );
        }
        match first {
            Ok(result) => {
                let got: Vec<serde_json::Value> = result
                    .rows
                    .iter()
                    .map(|r| {
                        serde_json::Value::Array(r.iter().map(|v| to_json(&store, v)).collect())
                    })
                    .collect();
                if canonical(&got, ordered) == canonical(expected, ordered) {
                    passing += 1;
                } else {
                    failures.push(format!(
                        "{id}: {query}\n    expected {expected:?}\n    got      {got:?}"
                    ));
                }
            }
            Err(e) => failures.push(format!("{id}: {query}\n    error: {e}")),
        }
    }
    assert!(
        failures.is_empty(),
        "{} wrong answers:\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert!(
        passing >= MIN_PASSING,
        "only {passing} cases pass, the ratchet is {MIN_PASSING}"
    );
}

#[test]
fn unresolved_edges_are_not_in_the_store_and_the_graph_is_not_changed() {
    let g = graph("small");
    let p = Payload::default();
    let store = ModelStore::new(&g, &p);
    let source_edges = g.edges.len();
    // The graph has one edge with no target; the store leaves it out.
    use arqlet_query::GraphStore;
    assert_eq!(store.edge_count(), source_edges - 1);
    assert_eq!(g.edges.len(), source_edges);
    let result = run(
        "MATCH ()-[r]->() RETURN count(*)",
        &store,
        &BTreeMap::new(),
        Limits::default(),
    )
    .unwrap();
    assert_eq!(
        result.rows,
        vec![vec![Value::Int((source_edges - 1) as i64)]]
    );
}

#[test]
fn payload_for_unknown_ids_is_ignored_and_counted_and_a_map_is_refused() {
    let g = graph("small");
    let json = serde_json::json!({
        "nodes": { "a": { "tag": "x" }, "nobody": { "tag": "y" } },
        "edges": { "unknown": { "tag": "z" }, "ab": { "since": 1 } }
    });
    let p = Payload::from_json(&json).unwrap();
    let store = ModelStore::new(&g, &p);
    // `nobody` is no node and `unknown` is an unresolved edge: two entries ignored.
    assert_eq!(store.ignored_payload(), 2);
    let map = serde_json::json!({ "nodes": { "a": { "nested": { "k": 1 } } } });
    let e = Payload::from_json(&map).unwrap_err();
    assert!(e.contains("nested") && e.contains("map"), "{e}");
}
