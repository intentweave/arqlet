//! Provisional (ADR 0021, stage M5c): a query-only WASM module. It holds one graph and its payload and answers openCypher
//! queries over them, with the same JSON answer as the engine's `query` operation, and carries no layout engine.
//!
//! Requests (JSON, one per `lw_run`):
//! - `{"op": "load", "graph": {...}, "payload": {"nodes": {...}, "edges": {...}}}` keeps the graph (an `arqlet-model`
//!   graph) and its optional payload, replacing what was loaded; answers `{ok, nodes, edges, unresolvedEdges}`: `edges` counts
//!   the edges of the graph as given, `unresolvedEdges` those a query does not see (no target, or an end that is no node), so
//!   a query's `graph.edges` is `edges - unresolvedEdges`. The graph is read straight from the request text, not through a
//!   JSON value of the whole request first.
//! - `{"op": "query", "query": "...", "params": {...}, "maxSteps": n, "maxRows": n, "maxHops": n}` answers as the engine's
//!   `query` (see `arqlet-query-json`), without the mapping onto a view: there is no view here.
//!
//! Everything is synchronous and deterministic; WASM runs on one thread, so a thread-local holds the loaded graph.
use arqlet_model::Graph;
use arqlet_query_model::Payload;
use serde_json::{json, Value};
use std::collections::HashSet;

/// A `load` request read straight into its types.
#[derive(serde::Deserialize)]
struct LoadRequest {
    op: String,
    graph: Graph,
    #[serde(default)]
    payload: Option<Value>,
}

/// A loaded graph with its payload.
#[derive(Default)]
pub struct QueryModule {
    loaded: Option<(Graph, Payload)>,
}

impl QueryModule {
    /// Runs one request given as JSON text and answers as JSON text.
    pub fn run(&mut self, input: &str) -> String {
        // A `load` read in one pass into its types: no JSON value of the whole graph beside the graph itself (which holds
        // it twice for a moment and, in WASM, leaves the linear memory at that high-water mark). Anything this reading
        // refuses goes the general way below, with the errors it has always given.
        if input.contains("\"load\"") {
            if let Ok(request) = serde_json::from_str::<LoadRequest>(input) {
                if request.op == "load" {
                    return match self.load(request.graph, request.payload.as_ref()) {
                        Ok(answer) => answer,
                        Err(e) => json!({"ok": false, "error": e}),
                    }
                    .to_string();
                }
            }
        }
        let answer = match serde_json::from_str::<Value>(input) {
            Ok(v) => self
                .answer(&v)
                .unwrap_or_else(|e| json!({"ok": false, "error": e})),
            Err(e) => json!({"ok": false, "error": e.to_string()}),
        };
        answer.to_string()
    }

    /// Keeps `graph` and the payload, and answers what was loaded. A payload that is refused keeps what was loaded before.
    fn load(&mut self, graph: Graph, payload: Option<&Value>) -> Result<Value, String> {
        let payload = match payload {
            None | Some(Value::Null) => Payload::default(),
            Some(p) => Payload::from_json(p)?,
        };
        // Only to look up; never iterated.
        let ids: HashSet<&str> = graph.nodes.iter().map(|n| n.id.as_str()).collect();
        let unresolved = graph
            .edges
            .iter()
            .filter(|e| {
                !ids.contains(e.source.as_str())
                    || !e.target.as_deref().is_some_and(|t| ids.contains(t))
            })
            .count();
        let answer = json!({
            "ok": true,
            "nodes": graph.nodes.len(),
            "edges": graph.edges.len(),
            "unresolvedEdges": unresolved,
        });
        self.loaded = Some((graph, payload));
        Ok(answer)
    }

    fn answer(&mut self, v: &Value) -> Result<Value, String> {
        match v["op"].as_str() {
            Some("load") => {
                let graph: Graph = serde_json::from_value(v["graph"].clone())
                    .map_err(|e| format!("The graph is not an Arqlet graph: {e}"))?;
                self.load(graph, v.get("payload"))
            }
            Some("query") => {
                let (graph, payload) = self
                    .loaded
                    .as_ref()
                    .ok_or("Nothing is loaded: send `load` with a graph first.")?;
                Ok(arqlet_query_json::run(graph, payload, v)?.answer)
            }
            Some(other) => Err(format!("Unknown operation: {other}")),
            None => Err("A request needs an `op`".into()),
        }
    }
}

thread_local! {
    static MODULE: std::cell::RefCell<QueryModule> = std::cell::RefCell::new(QueryModule::default());
}

/// Minimal owned-buffer ABI, as the engine's: the host passes only live allocations from this module.
#[no_mangle]
pub extern "C" fn lw_alloc(len: usize) -> *mut u8 {
    Box::into_raw(vec![0u8; len].into_boxed_slice()) as *mut u8
}

/// # Safety
/// `ptr,len` must describe a live buffer returned by `lw_alloc` or `lw_run`, freed once.
#[no_mangle]
pub unsafe extern "C" fn lw_free(ptr: *mut u8, len: usize) {
    drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(ptr, len)));
}

/// # Safety
/// Input must be a valid live allocation of `len` bytes. The caller retains ownership. Returns the output pointer (low 32
/// bits) and length (high 32 bits).
#[no_mangle]
pub unsafe extern "C" fn lw_run(ptr: *const u8, len: usize) -> u64 {
    let bytes = std::slice::from_raw_parts(ptr, len);
    let result = match std::str::from_utf8(bytes) {
        Ok(s) => MODULE.with(|m| m.borrow_mut().run(s)),
        Err(_) => "{\"ok\":false,\"error\":\"Invalid UTF-8\"}".into(),
    };
    let out = result.into_bytes().into_boxed_slice();
    let size = out.len() as u64;
    let address = Box::into_raw(out) as *mut u8 as u64;
    (size << 32) | (address & 0xffffffff)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graph() -> Value {
        json!({
            "nodes": [
                {"id": "a", "kind": "fn", "label": "parse"},
                {"id": "b", "kind": "fn", "label": "render"},
            ],
            "edges": [
                {"id": "ab", "kind": "calls", "source": "a", "target": "b"},
                {"id": "lost", "kind": "calls", "source": "a", "target": null},
            ],
        })
    }

    fn ask(m: &mut QueryModule, v: Value) -> Value {
        serde_json::from_str(&m.run(&v.to_string())).unwrap()
    }

    #[test]
    fn a_loaded_graph_answers_queries_with_the_engines_answer() {
        let mut m = QueryModule::default();
        let nothing = ask(
            &mut m,
            json!({"op": "query", "query": "MATCH (n) RETURN n"}),
        );
        assert_eq!(nothing["ok"], false);
        let load = ask(
            &mut m,
            json!({"op": "load", "graph": graph(), "payload": {"nodes": {"a": {"loc": 7}}}}),
        );
        assert_eq!(
            load,
            json!({"ok": true, "nodes": 2, "edges": 2, "unresolvedEdges": 1})
        );
        let q = ask(
            &mut m,
            json!({"op": "query", "query": "MATCH (a)-[r]->(b) WHERE a.loc > $min RETURN a, r, b.label", "params": {"min": 1}}),
        );
        assert_eq!(
            q["rows"],
            json!([[{"node": "a"}, {"edge": "ab"}, "render"]])
        );
        assert_eq!(
            q["graph"],
            json!({"nodes": 2, "edges": 1}),
            "the unresolved edge is left out"
        );
        assert_eq!(q["nodes"], json!(["a"]));
    }

    #[test]
    fn a_load_read_in_one_pass_is_the_same_as_the_general_way() {
        let request =
            json!({"op": "load", "graph": graph(), "payload": {"nodes": {"a": {"loc": 7}}}});
        let mut one_pass = QueryModule::default();
        let mut general = QueryModule::default();
        // `run` reads a `load` straight into its types; `answer` takes the JSON value, as every other request does.
        let a = ask(&mut one_pass, request.clone());
        let b = general.answer(&request).unwrap();
        assert_eq!(a, b);
        let question = json!({"op": "query", "query": "MATCH (n) WHERE n.loc = 7 RETURN n.label"});
        assert_eq!(
            ask(&mut one_pass, question.clone()),
            ask(&mut general, question)
        );
        // A payload that is refused is refused the same way, and keeps what was loaded.
        let bad =
            json!({"op": "load", "graph": graph(), "payload": {"nodes": {"a": {"m": {"k": 1}}}}});
        let (x, y) = (ask(&mut one_pass, bad.clone()), ask(&mut general, bad));
        assert_eq!(x["ok"], false);
        assert_eq!(x, y);
    }

    #[test]
    fn errors_and_bad_input_are_answers_not_panics() {
        let mut m = QueryModule::default();
        assert_eq!(ask(&mut m, json!({"op": "nope"}))["ok"], false);
        assert_eq!(
            serde_json::from_str::<Value>(&m.run("{not json")).unwrap()["ok"],
            false
        );
        ask(&mut m, json!({"op": "load", "graph": graph()}));
        let syntax = ask(&mut m, json!({"op": "query", "query": "MATCH (n RETURN n"}));
        assert!(syntax["error"]
            .as_str()
            .unwrap()
            .starts_with("query syntax:"));
        let bad = ask(&mut m, json!({"op": "load", "graph": 3}));
        assert_eq!(bad["ok"], false);
        // A failed load keeps what was loaded.
        assert_eq!(
            ask(
                &mut m,
                json!({"op": "query", "query": "MATCH (n) RETURN count(*)"})
            )["rows"],
            json!([[2]])
        );
    }
}
