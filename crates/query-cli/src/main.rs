//! Provisional (ADR 0021, stage M5a): `arqlet-query`, an openCypher query over an Arqlet graph file, with no layout.
//!
//! ```text
//! arqlet-query GRAPH.json 'MATCH (n:symbol) RETURN n.label ORDER BY n.label'
//!     [--payload FILE.json]   properties by node and edge id: {"nodes": {id: {key: value}}, "edges": {...}}
//!     [--param NAME=JSON]     the value of $NAME (repeatable)
//!     [--max-steps N] [--max-rows N] [--max-hops N]
//!     [--json]                one JSON object {"columns": [...], "rows": [...]} instead of a table
//! ```
//!
//! Exit status: 0 for an answer, 1 when the query fails (the error is printed as `class: message`), 2 for a bad call.
use arqlet_model::Graph;
use arqlet_query::{Limits, Value};
use arqlet_query_model::{value_of, ModelStore, Payload};
use std::collections::BTreeMap;
use std::process::ExitCode;

struct Call {
    graph: String,
    query: String,
    payload: Option<String>,
    params: BTreeMap<String, Value>,
    limits: Limits,
    json: bool,
}

fn parse(args: &[String]) -> Result<Call, String> {
    let mut positional = Vec::new();
    let (mut payload, mut json) = (None, false);
    let mut params = BTreeMap::new();
    let mut limits = Limits::default();
    let mut it = args.iter();
    let number = |name: &str, value: Option<&String>| -> Result<u64, String> {
        value
            .ok_or_else(|| format!("{name} needs a value"))?
            .parse::<u64>()
            .map_err(|_| format!("{name} needs a non-negative integer"))
    };
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--payload" => payload = Some(it.next().ok_or("--payload needs a file")?.clone()),
            "--json" => json = true,
            "--param" => {
                let spec = it.next().ok_or("--param needs NAME=JSON")?;
                let (name, text) = spec.split_once('=').ok_or("--param needs NAME=JSON")?;
                let parsed: serde_json::Value = serde_json::from_str(text)
                    .map_err(|e| format!("--param {name}: not JSON: {e}"))?;
                params.insert(
                    name.to_string(),
                    value_of(&parsed).map_err(|e| format!("--param {name}: {e}"))?,
                );
            }
            "--max-steps" => limits.max_steps = number("--max-steps", it.next())?,
            "--max-rows" => limits.max_rows = number("--max-rows", it.next())? as usize,
            "--max-hops" => {
                limits.max_hops = number("--max-hops", it.next())?.min(u32::MAX as u64) as u32
            }
            flag if flag.starts_with("--") => return Err(format!("unknown option {flag}")),
            _ => positional.push(arg.clone()),
        }
    }
    let [graph, query] = <[String; 2]>::try_from(positional)
        .map_err(|_| "expected two arguments: GRAPH.json and a QUERY".to_string())?;
    Ok(Call {
        graph,
        query,
        payload,
        params,
        limits,
        json,
    })
}

fn show(store: &ModelStore, v: &Value) -> serde_json::Value {
    match v {
        Value::Null => serde_json::Value::Null,
        Value::Bool(b) => serde_json::json!(b),
        Value::Int(i) => serde_json::json!(i),
        Value::Float(f) => serde_json::json!(f),
        Value::String(s) => serde_json::json!(s),
        Value::List(items) => {
            serde_json::Value::Array(items.iter().map(|i| show(store, i)).collect())
        }
        Value::Node(n) => serde_json::json!({ "node": store.node_id(*n) }),
        Value::Edge(e) => serde_json::json!({ "edge": store.edge_id(*e) }),
    }
}

fn cell(store: &ModelStore, v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => "null".to_string(),
        other => show(store, other).to_string(),
    }
}

fn read(path: &str) -> Result<serde_json::Value, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("cannot read {path}: {e}"))?;
    serde_json::from_str(&text).map_err(|e| format!("{path} is not JSON: {e}"))
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let call = match parse(&args) {
        Ok(call) => call,
        Err(message) => {
            eprintln!("arqlet-query: {message}\nusage: arqlet-query GRAPH.json 'QUERY' [--payload FILE] [--param NAME=JSON] [--json]");
            return ExitCode::from(2);
        }
    };
    let loaded = (|| -> Result<(Graph, Payload), String> {
        let graph: Graph = serde_json::from_value(read(&call.graph)?)
            .map_err(|e| format!("{} is not an Arqlet graph: {e}", call.graph))?;
        let payload = match &call.payload {
            Some(path) => Payload::from_json(&read(path)?)?,
            None => Payload::default(),
        };
        Ok((graph, payload))
    })();
    let (graph, payload) = match loaded {
        Ok(pair) => pair,
        Err(message) => {
            eprintln!("arqlet-query: {message}");
            return ExitCode::from(2);
        }
    };
    let store = ModelStore::new(&graph, &payload);
    if store.ignored_payload() > 0 {
        eprintln!(
            "arqlet-query: {} payload entries name no node or resolved edge and were ignored",
            store.ignored_payload()
        );
    }
    match arqlet_query_cypher::run(&call.query, &store, &call.params, call.limits) {
        Ok(result) if call.json => {
            let rows: Vec<_> = result
                .rows
                .iter()
                .map(|r| serde_json::Value::Array(r.iter().map(|v| show(&store, v)).collect()))
                .collect();
            println!(
                "{}",
                serde_json::json!({ "columns": result.columns, "rows": rows })
            );
            ExitCode::SUCCESS
        }
        Ok(result) => {
            println!("{}", result.columns.join("\t"));
            for row in &result.rows {
                println!(
                    "{}",
                    row.iter()
                        .map(|v| cell(&store, v))
                        .collect::<Vec<_>>()
                        .join("\t")
                );
            }
            eprintln!("({} rows)", result.rows.len());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::from(1)
        }
    }
}
