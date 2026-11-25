//! Provisional (ADR 0021, stage M5b): the operation `query`, an openCypher query over the session's source graph and its
//! content (ADR 0023, R1).
//!
//! The query reads what the export contains (no synthetic bus nodes, unresolved edges left out) and the session's content
//! as its properties, in place: nothing is copied for it. The answer names ids and values. It maps them onto the current
//! view only on request (`"view": true`), because `ViewMap` is built over the whole source graph (ADR 0021, item 10): then
//! `shown` tells for each node id in the answer the box that shows it, and for each edge id the drawn line that carries it.
//!
//! A query sees the session's source graph. For an `import` with a `limit` that is the selection the engine kept (at most
//! 10 000 nodes and their ancestors), not the whole export; the answer says how large it was (`graph`).
//!
//! The operation exists only with the cargo feature `query`; without it every request is refused with that reason.
//!
//! Request: `query` (text), optional `params` (object of `$name` values), optional `maxSteps`, `maxRows`, `maxHops`
//! (the execution limits of ADR 0021 item 11), optional `view`. A stateless `dispatch` request also carries the `snapshot`
//! and may carry `content` (`{"nodes": {id: {key: value}}, "edges": {…}}`); a session uses its own.
use crate::content::{Content, VIRTUAL_KEYS};
use arqlet_model::Snapshot;
use serde_json::{json, Value};

/// A request that carries its snapshot (and, optionally, content): the answer or an error.
pub fn query_response(v: &Value) -> Value {
    let result = serde_json::from_value::<Snapshot>(v["snapshot"].clone())
        .map_err(|e| e.to_string())
        .and_then(|snapshot| {
            let content = match v.get("content") {
                None | Some(Value::Null) => None,
                Some(c) => Some(content_of(c)?),
            };
            query_on(&snapshot, content.as_ref(), v)
        });
    result.unwrap_or_else(|error| json!({"ok": false, "error": error}))
}

/// The content a stateless request brings; it may not use the model's own keys, as a session's content may not.
fn content_of(v: &Value) -> Result<Content, String> {
    let table = |part: &str| -> Result<crate::content::Properties, String> {
        match v.get(part) {
            None | Some(Value::Null) => Ok(Default::default()),
            Some(t) => serde_json::from_value(t.clone())
                .map_err(|e| format!("content.{part} must map ids to properties: {e}"))
                .and_then(|table: crate::content::Properties| {
                    for (id, properties) in &table {
                        if let Some(key) = VIRTUAL_KEYS
                            .iter()
                            .find(|key| properties.contains_key(**key))
                        {
                            return Err(format!(
                                "content key {key} is a property of the model itself (on {id}); use another name"
                            ));
                        }
                    }
                    Ok(table)
                }),
        }
    };
    Ok(Content {
        revision: 0,
        nodes: table("nodes")?,
        edges: table("edges")?,
    })
}

#[cfg(not(feature = "query"))]
pub fn query_on(_: &Snapshot, _: Option<&Content>, _: &Value) -> Result<Value, String> {
    Err("query is not built into this engine (cargo feature `query` of arqlet-runtime)".into())
}

/// The answer to a `query` request on a snapshot the caller holds (a session's), with its content.
#[cfg(feature = "query")]
pub fn query_on(
    snapshot: &Snapshot,
    content: Option<&Content>,
    v: &Value,
) -> Result<Value, String> {
    use arqlet_model::Graph;
    use arqlet_query::Value as Cell;
    use arqlet_query_model::{value_of, PayloadSource};
    use std::cell::Cell as Counter;
    use std::collections::BTreeMap;

    /// The session's JSON content as a payload; a value the query layer has no type for (an integer beyond `i64`) reads
    /// as null and is counted, so the answer says that something was left out instead of changing it silently.
    struct ContentSource<'a> {
        content: Option<&'a Content>,
        skipped: Counter<usize>,
    }
    impl ContentSource<'_> {
        fn read(&self, properties: Option<&BTreeMap<String, Value>>, key: &str) -> Cell {
            match properties.and_then(|p| p.get(key)) {
                None => Cell::Null,
                Some(json) => value_of(json).unwrap_or_else(|_| {
                    self.skipped.set(self.skipped.get() + 1);
                    Cell::Null
                }),
            }
        }
    }
    impl PayloadSource for ContentSource<'_> {
        fn node_property(&self, id: &str, key: &str) -> Cell {
            self.read(self.content.and_then(|c| c.node(id)), key)
        }
        fn edge_property(&self, id: &str, key: &str) -> Cell {
            self.read(self.content.and_then(|c| c.edge(id)), key)
        }
        fn unknown(
            &self,
            known_node: &dyn Fn(&str) -> bool,
            known_edge: &dyn Fn(&str) -> bool,
        ) -> usize {
            self.content.map_or(0, |c| {
                c.nodes.keys().filter(|id| !known_node(id)).count()
                    + c.edges.keys().filter(|id| !known_edge(id)).count()
            })
        }
    }

    // The source graph as the export has it, the same one `payload` counts against.
    let graph: &Graph = snapshot.source_graph.as_deref().unwrap_or(&snapshot.graph);
    let source = ContentSource {
        content,
        skipped: Counter::new(0),
    };
    let outcome = arqlet_query_json::run(graph, &source, v)?;
    let mut answer = outcome.answer;
    if let Some(c) = content.filter(|c| !c.is_empty()) {
        answer["contentRevision"] = json!(c.revision);
    }
    if source.skipped.get() > 0 {
        answer["skippedValues"] = json!(source.skipped.get());
    }
    if v.get("view").and_then(Value::as_bool) == Some(true) {
        let navigable = crate::trace::navigable(snapshot);
        let map = crate::trace::ViewMap::new(snapshot, navigable.as_ref())?;
        let shown_nodes: BTreeMap<&String, Option<String>> =
            outcome.nodes.iter().map(|id| (id, map.owner(id))).collect();
        let shown_edges: BTreeMap<&String, Option<String>> = outcome
            .edges
            .iter()
            .map(|id| (id, map.route_of(id, &None)))
            .collect();
        answer["shown"] = json!({ "nodes": shown_nodes, "edges": shown_edges });
    }
    Ok(answer)
}
