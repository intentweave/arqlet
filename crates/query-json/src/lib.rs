//! Provisional (ADR 0021): the JSON request and answer of the operation `query`, without the layout engine.
//!
//! [`run`] takes a graph, a payload source and a request and gives the answer as JSON plus the ids it names. The engine
//! (`arqlet-runtime`) adds what only it knows (the content's revision, the mapping onto the view); the query-only WASM
//! module (`arqlet-query-wasm`) answers with it as it is.
//!
//! Request: `query` (text), optional `params` (object of `$name` values), optional `maxSteps`, `maxRows`, `maxHops`
//! (the execution limits of ADR 0021 item 11). Answer: `columns`, `rowCount`, `rows` (nodes as `{"node": id}`, edges as
//! `{"edge": id}`, maps as `{"map": {key: value}}`; every object in a row is one of the three), `nodes` and `edges` (the ids once each, in the order they first appear) and `graph` (the size of what
//! the query could see). Errors read `query <class>: message`.
#![forbid(unsafe_code)]

use arqlet_model::Graph;
use arqlet_query::{GraphStore, Limits, Value as Cell};
use arqlet_query_model::{value_of, ModelStore, PayloadSource};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

/// The answer and the ids it names.
pub struct Outcome {
    /// `{ok: true, columns, rowCount, rows, nodes, edges, graph}`, and `ignoredContent` when the payload named ids the
    /// graph does not have.
    pub answer: Value,
    /// The node ids in the rows, once each, in the order they first appear.
    pub nodes: Vec<String>,
    /// The edge ids in the rows, once each, in the order they first appear.
    pub edges: Vec<String>,
}

/// Runs the request `v` over `graph` with the properties of `payload`.
pub fn run(graph: &Graph, payload: &dyn PayloadSource, v: &Value) -> Result<Outcome, String> {
    let text = v["query"]
        .as_str()
        .ok_or("A query request needs the query text in `query`")?;
    let mut params = BTreeMap::new();
    match v.get("params") {
        None | Some(Value::Null) => {}
        Some(Value::Object(map)) => {
            for (name, value) in map {
                params.insert(
                    name.clone(),
                    value_of(value).map_err(|e| format!("query parameter ${name}: {e}"))?,
                );
            }
        }
        Some(_) => return Err("query `params` must be an object of values".into()),
    }
    let mut limits = Limits::default();
    let limit = |key: &str| -> Result<Option<u64>, String> {
        match v.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(n) => n
                .as_u64()
                .map(Some)
                .ok_or_else(|| format!("query {key} must be a whole number")),
        }
    };
    if let Some(n) = limit("maxSteps")? {
        limits.max_steps = n;
    }
    if let Some(n) = limit("maxRows")? {
        limits.max_rows = n.min(usize::MAX as u64) as usize;
    }
    if let Some(n) = limit("maxHops")? {
        limits.max_hops = n.min(u32::MAX as u64) as u32;
    }

    let store = ModelStore::new(graph, payload);
    let result = arqlet_query_cypher::run(text, &store, &params, limits)
        .map_err(|e| format!("query {}: {}", e.class.as_str(), e.message))?;

    /// The ids an answer names, each once, in the order they first appear.
    #[derive(Default)]
    struct Seen {
        node_set: BTreeSet<String>,
        nodes: Vec<String>,
        edge_set: BTreeSet<String>,
        edges: Vec<String>,
    }
    fn show(store: &ModelStore, v: &Cell, seen: &mut Seen) -> Value {
        match v {
            Cell::Null => Value::Null,
            Cell::Bool(b) => json!(b),
            Cell::Int(i) => json!(i),
            Cell::Float(f) => json!(f),
            Cell::String(s) => json!(s),
            Cell::List(items) => Value::Array(items.iter().map(|i| show(store, i, seen)).collect()),
            // Tagged like a node and an edge: a map with the key `node` is not mistaken for one.
            Cell::Map(entries) => json!({ "map": entries
                .iter()
                .map(|(k, v)| (k.clone(), show(store, v, seen)))
                .collect::<serde_json::Map<_, _>>() }),
            Cell::Node(n) => {
                let id = store.node_id(*n);
                if seen.node_set.insert(id.to_string()) {
                    seen.nodes.push(id.to_string());
                }
                json!({ "node": id })
            }
            Cell::Edge(e) => {
                let id = store.edge_id(*e);
                if seen.edge_set.insert(id.to_string()) {
                    seen.edges.push(id.to_string());
                }
                json!({ "edge": id })
            }
        }
    }
    let mut seen = Seen::default();
    let rows: Vec<Value> = result
        .rows
        .iter()
        .map(|row| Value::Array(row.iter().map(|c| show(&store, c, &mut seen)).collect()))
        .collect();

    let mut answer = json!({
        "ok": true,
        "columns": result.columns,
        "rowCount": rows.len(),
        "rows": rows,
        "nodes": seen.nodes,
        "edges": seen.edges,
        // What the query could see: the graph it was given (resolved edges only).
        "graph": {"nodes": graph.nodes.len(), "edges": store.edge_count()},
    });
    if store.ignored_payload() > 0 {
        answer["ignoredContent"] = json!(store.ignored_payload());
    }
    Ok(Outcome {
        answer,
        nodes: seen.nodes,
        edges: seen.edges,
    })
}
