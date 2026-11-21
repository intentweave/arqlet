//! The executable cases of `examples/cypher/` against the engine (ADR 0021, stages M2 to M4).
//!
//! Every case with status `supported` is lowered and run over a store built from the same graph and payload files the
//! Python oracle read, and its rows are compared with the oracle's. A case the engine says it has not built yet is
//! *pending*; a case that gets a wrong answer, or any other error, is a failure. `MIN_PASSING` is a ratchet: it goes up
//! with each stage and never down, so that a regression in a case that used to pass is seen.
use arqlet_query::{Direction, EdgeId, EdgeRef, ErrorClass, GraphStore, Limits, NodeId, Value};
use arqlet_query_cypher::run;
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

/// How many of the supported cases pass today. Raised by every stage that makes more of them pass.
const MIN_PASSING: usize = 37;

fn fixture(path: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/cypher")
        .join(path);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

fn json_value(v: &serde_json::Value) -> Value {
    match v {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Bool(*b),
        serde_json::Value::Number(n) => n
            .as_i64()
            .map_or_else(|| Value::Float(n.as_f64().unwrap_or(f64::NAN)), Value::Int),
        serde_json::Value::String(s) => Value::String(s.clone()),
        serde_json::Value::Array(items) => Value::List(items.iter().map(json_value).collect()),
        serde_json::Value::Object(_) => Value::Null, // maps are not values of this layer yet
    }
}

/// A store over the fixture files: the virtual properties of the model plus the payload table (ADR 0021, M0).
struct JsonStore {
    labels: Vec<String>,
    props: Vec<BTreeMap<String, Value>>,
    /// Resolved edges only: source, target, type, properties. An edge whose end is not a node is left out.
    edges: Vec<(u32, u32, String, BTreeMap<String, Value>)>,
}

impl JsonStore {
    fn load(graph: &str, payload: Option<&str>) -> JsonStore {
        let g: serde_json::Value =
            serde_json::from_str(&fixture(&format!("graphs/{graph}.json"))).unwrap();
        let extra: serde_json::Value = match payload {
            Some(p) => serde_json::from_str(&fixture(&format!("payloads/{p}.json"))).unwrap(),
            None => serde_json::json!({"nodes": {}, "edges": {}}),
        };
        let mut store = JsonStore {
            labels: vec![],
            props: vec![],
            edges: vec![],
        };
        let mut index: BTreeMap<String, u32> = BTreeMap::new();
        for n in g["nodes"].as_array().unwrap() {
            let id = n["id"].as_str().unwrap();
            let mut props = BTreeMap::new();
            props.insert("id".to_string(), Value::String(id.to_string()));
            props.insert("kind".to_string(), json_value(&n["kind"]));
            props.insert("label".to_string(), json_value(&n["label"]));
            if let Some(parent) = n["parentId"].as_str() {
                props.insert("parentId".to_string(), Value::String(parent.to_string()));
            }
            if let Some(map) = extra["nodes"].get(id).and_then(|m| m.as_object()) {
                for (k, v) in map {
                    props.insert(k.clone(), json_value(v));
                }
            }
            index.insert(id.to_string(), store.labels.len() as u32);
            store.labels.push(n["kind"].as_str().unwrap().to_string());
            store.props.push(props);
        }
        for e in g["edges"].as_array().into_iter().flatten() {
            let end = |k: &str| e[k].as_str().and_then(|id| index.get(id)).copied();
            let (Some(source), Some(target)) = (end("source"), end("target")) else {
                continue; // unresolved: not in the store
            };
            let id = e["id"].as_str().unwrap();
            let mut props = BTreeMap::new();
            props.insert("id".to_string(), Value::String(id.to_string()));
            props.insert("kind".to_string(), json_value(&e["kind"]));
            if let Some(w) = e.get("weight") {
                props.insert("weight".to_string(), json_value(w));
            }
            if let Some(map) = extra["edges"].get(id).and_then(|m| m.as_object()) {
                for (k, v) in map {
                    props.insert(k.clone(), json_value(v));
                }
            }
            let kind = e["kind"].as_str().unwrap().to_string();
            store.edges.push((source, target, kind, props));
        }
        store
    }
}

impl GraphStore for JsonStore {
    fn node_count(&self) -> usize {
        self.labels.len()
    }
    fn node_label(&self, node: NodeId) -> &str {
        &self.labels[node.0 as usize]
    }
    fn node_property(&self, node: NodeId, key: &str) -> Value {
        self.props[node.0 as usize]
            .get(key)
            .cloned()
            .unwrap_or(Value::Null)
    }
    fn edge_count(&self) -> usize {
        self.edges.len()
    }
    fn edge_type(&self, edge: EdgeId) -> &str {
        &self.edges[edge.0 as usize].2
    }
    fn edge_property(&self, edge: EdgeId, key: &str) -> Value {
        self.edges[edge.0 as usize]
            .3
            .get(key)
            .cloned()
            .unwrap_or(Value::Null)
    }
    fn edges_of(&self, node: NodeId, direction: Direction) -> Vec<EdgeRef> {
        let mut out = Vec::new();
        for (i, (source, target, _, _)) in self.edges.iter().enumerate() {
            let (outgoing, incoming) = (*source == node.0, *target == node.0);
            let wanted = match direction {
                Direction::Out => outgoing,
                Direction::In => incoming,
                Direction::Both => outgoing || incoming,
            };
            if wanted {
                // A self-loop is listed once; its other end is the node itself.
                let other = if outgoing && direction != Direction::In {
                    *target
                } else {
                    *source
                };
                out.push(EdgeRef {
                    edge: EdgeId(i as u32),
                    other: NodeId(other),
                });
            }
        }
        out
    }
}

fn to_json(v: &Value) -> serde_json::Value {
    match v {
        Value::Null => serde_json::Value::Null,
        Value::Bool(b) => serde_json::json!(b),
        Value::Int(i) => serde_json::json!(i),
        Value::Float(f) => serde_json::json!(f),
        Value::String(s) => serde_json::json!(s),
        Value::List(items) => serde_json::Value::Array(items.iter().map(to_json).collect()),
        Value::Node(n) => serde_json::json!(format!("node {}", n.0)),
        Value::Edge(e) => serde_json::json!(format!("edge {}", e.0)),
    }
}

fn canonical(rows: &[serde_json::Value], ordered: bool) -> Vec<String> {
    let mut v: Vec<String> = rows.iter().map(|r| r.to_string()).collect();
    if !ordered {
        v.sort();
    }
    v
}

#[test]
fn the_supported_cases_get_the_oracles_rows() {
    let mut stores: BTreeMap<(String, String), JsonStore> = BTreeMap::new();
    let (mut passing, mut pending, mut failures) = (Vec::new(), Vec::new(), Vec::new());
    for line in fixture("cases.jsonl").lines() {
        let case: serde_json::Value = serde_json::from_str(line).unwrap();
        if case["status"] != "supported" {
            continue;
        }
        let (id, query) = (
            case["id"].as_str().unwrap().to_string(),
            case["query"].as_str().unwrap(),
        );
        let graph = case["graph"].as_str().unwrap().to_string();
        let payload = case["payload"].as_str().map(str::to_string);
        let store = stores
            .entry((graph.clone(), payload.clone().unwrap_or_default()))
            .or_insert_with(|| JsonStore::load(&graph, payload.as_deref()));
        let ordered = case["ordered"].as_bool().unwrap_or(false);
        let expected = case["rows"].as_array().unwrap();
        match run(query, store, &BTreeMap::new(), Limits::default()) {
            Ok(result) => {
                let got: Vec<serde_json::Value> = result
                    .rows
                    .iter()
                    .map(|r| serde_json::Value::Array(r.iter().map(to_json).collect()))
                    .collect();
                if canonical(&got, ordered) == canonical(expected, ordered) {
                    passing.push(id);
                } else {
                    failures.push(format!(
                        "{id}: {query}\n    expected {}\n    got      {}",
                        serde_json::Value::Array(expected.clone()),
                        serde_json::Value::Array(got)
                    ));
                }
            }
            Err(e) if e.class == ErrorClass::Unsupported && e.message.contains("not built yet") => {
                pending.push(id)
            }
            Err(e) => failures.push(format!("{id}: {query}\n    error: {e}")),
        }
    }
    println!("passing {}: {}", passing.len(), passing.join(" "));
    println!("pending {}: {}", pending.len(), pending.join(" "));
    assert!(
        failures.is_empty(),
        "{} wrong answers:\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert!(
        passing.len() >= MIN_PASSING,
        "only {} cases pass, the ratchet is {MIN_PASSING}; pending: {}",
        passing.len(),
        pending.join(" ")
    );
}
