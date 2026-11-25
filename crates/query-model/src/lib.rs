//! Provisional (ADR 0021, stage M5a): the adapter that lets the query layer read an [`arqlet_model::Graph`] in place.
//!
//! [`ModelStore`] borrows the graph and a [`Payload`]; it copies neither. It keeps only an index: which edges are
//! resolved, their ends, and the edges of every node in the order of their ids. Reading it needs nothing of the layout
//! engine, so queries run on an imported graph before any layout exists.
//!
//! What a query sees, as the ADR fixes it (M0):
//! - A node has one label, its `kind`. Its virtual properties are `id`, `kind`, `label` and `parentId` (null for a root);
//!   any other property comes from the payload.
//! - An edge has one type, its `kind`. Its virtual properties are `id`, `kind` and `weight` (how many edges it stands for,
//!   1 when left out); any other comes from the payload.
//! - An edge whose target is missing or names no node is not in the store: unresolved edges are excluded before a query
//!   runs, and the graph itself is not changed.
//! - The buses of a graph are not nodes of the source graph and are not seen.
//! - A payload entry for an id the graph does not have is ignored and counted ([`ModelStore::ignored_payload`]); a payload
//!   value that is a map is refused, because maps are not values of the query layer yet.
//! - A payload key named like a virtual property ([`VIRTUAL_KEYS`]) is refused by [`Payload::from_json`], on nodes and on
//!   edges, as the engine's content refuses it.
#![forbid(unsafe_code)]

use arqlet_model::Graph;
use arqlet_query::{Direction, EdgeId, EdgeRef, GraphStore, NodeId, Value};
use std::collections::{BTreeMap, HashMap};

/// The keys a query reads from the model itself; a payload may not use them.
pub const VIRTUAL_KEYS: [&str; 5] = ["id", "kind", "label", "parentId", "weight"];

/// The properties a host supplies beside the graph, by node id and by edge id (ADR 0021, M0; the table of ADR 0022).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Payload {
    pub nodes: BTreeMap<String, BTreeMap<String, Value>>,
    pub edges: BTreeMap<String, BTreeMap<String, Value>>,
}

impl Payload {
    /// Reads `{"nodes": {id: {key: value}}, "edges": {id: {key: value}}}`. Either part may be left out.
    pub fn from_json(json: &serde_json::Value) -> Result<Payload, String> {
        Ok(Payload {
            nodes: table(json, "nodes")?,
            edges: table(json, "edges")?,
        })
    }
}

fn table(
    json: &serde_json::Value,
    part: &str,
) -> Result<BTreeMap<String, BTreeMap<String, Value>>, String> {
    let mut out = BTreeMap::new();
    let Some(entries) = json.get(part) else {
        return Ok(out);
    };
    let entries = entries
        .as_object()
        .ok_or_else(|| format!("payload `{part}` must be an object keyed by id"))?;
    for (id, props) in entries {
        let props = props
            .as_object()
            .ok_or_else(|| format!("payload {part} `{id}` must be an object of properties"))?;
        let mut map = BTreeMap::new();
        for (key, value) in props {
            if VIRTUAL_KEYS.contains(&key.as_str()) {
                return Err(format!(
                    "payload {part} `{id}`: the key `{key}` is a property of the model itself; use another name"
                ));
            }
            map.insert(
                key.clone(),
                value_of(value)
                    .map_err(|e| format!("payload {part} `{id}`, property `{key}`: {e}"))?,
            );
        }
        out.insert(id.clone(), map);
    }
    Ok(out)
}

/// Where the properties of nodes and edges come from. A host that keeps them in its own form (a session keeps JSON) reads
/// them through this trait instead of copying them into a [`Payload`] for every query.
pub trait PayloadSource {
    /// The property `key` of the node `id`; null when there is none.
    fn node_property(&self, id: &str, key: &str) -> Value;
    /// The property `key` of the edge `id`; null when there is none.
    fn edge_property(&self, id: &str, key: &str) -> Value;
    /// How many ids it holds properties for that `known_node` and `known_edge` do not accept.
    fn unknown(
        &self,
        known_node: &dyn Fn(&str) -> bool,
        known_edge: &dyn Fn(&str) -> bool,
    ) -> usize;
}

impl PayloadSource for Payload {
    fn node_property(&self, id: &str, key: &str) -> Value {
        payload_value(self.nodes.get(id), key)
    }
    fn edge_property(&self, id: &str, key: &str) -> Value {
        payload_value(self.edges.get(id), key)
    }
    fn unknown(
        &self,
        known_node: &dyn Fn(&str) -> bool,
        known_edge: &dyn Fn(&str) -> bool,
    ) -> usize {
        self.nodes.keys().filter(|id| !known_node(id)).count()
            + self.edges.keys().filter(|id| !known_edge(id)).count()
    }
}

/// A JSON value as a query value. A map, and an integer beyond `i64`, are refused instead of being changed.
pub fn value_of(json: &serde_json::Value) -> Result<Value, String> {
    use serde_json::Value as J;
    Ok(match json {
        J::Null => Value::Null,
        J::Bool(b) => Value::Bool(*b),
        J::Number(n) => match n.as_i64() {
            Some(i) => Value::Int(i),
            None if n.is_u64() => return Err("an integer beyond i64".into()),
            None => Value::Float(n.as_f64().ok_or("a number that is not a finite float")?),
        },
        J::String(s) => Value::String(s.clone()),
        J::Array(items) => Value::List(items.iter().map(value_of).collect::<Result<_, _>>()?),
        J::Object(_) => return Err("a map is not a value of the query layer yet".into()),
    })
}

/// A graph and its payload as a [`GraphStore`].
pub struct ModelStore<'a> {
    graph: &'a Graph,
    payload: &'a dyn PayloadSource,
    /// For each resolved edge, in graph order: its index in `graph.edges`, its source and its target (node indices).
    edges: Vec<(usize, u32, u32)>,
    /// For each node, the resolved edges that start and that end at it, ascending.
    out: Vec<Vec<u32>>,
    incoming: Vec<Vec<u32>>,
    ignored: usize,
}

impl<'a> ModelStore<'a> {
    /// Indexes the graph. Cost: one pass over the nodes and the edges.
    pub fn new(graph: &'a Graph, payload: &'a dyn PayloadSource) -> ModelStore<'a> {
        let index: HashMap<&str, u32> = graph
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id.as_str(), i as u32))
            .collect(); // only to find a node by id; never iterated
        let mut edges = Vec::new();
        let mut out = vec![Vec::new(); graph.nodes.len()];
        let mut incoming = vec![Vec::new(); graph.nodes.len()];
        let mut resolved: HashMap<&str, ()> = HashMap::new(); // only to find an edge by id; never iterated
        for (i, edge) in graph.edges.iter().enumerate() {
            let source = index.get(edge.source.as_str());
            let target = edge.target.as_deref().and_then(|t| index.get(t));
            let (Some(&source), Some(&target)) = (source, target) else {
                continue; // unresolved: not in the store
            };
            let id = edges.len() as u32;
            edges.push((i, source, target));
            resolved.insert(edge.id.as_str(), ());
            out[source as usize].push(id);
            incoming[target as usize].push(id);
        }
        let ignored = payload.unknown(&|id| index.contains_key(id), &|id| {
            resolved.contains_key(id)
        });
        ModelStore {
            graph,
            payload,
            edges,
            out,
            incoming,
            ignored,
        }
    }

    /// How many payload entries named an id the graph does not have (or an unresolved edge) and were ignored.
    pub fn ignored_payload(&self) -> usize {
        self.ignored
    }

    /// The id of a node of the graph.
    pub fn node_id(&self, node: NodeId) -> &str {
        &self.graph.nodes[node.0 as usize].id
    }

    /// The id of an edge of the graph.
    pub fn edge_id(&self, edge: EdgeId) -> &str {
        &self.graph.edges[self.edges[edge.0 as usize].0].id
    }
}

fn payload_value(map: Option<&BTreeMap<String, Value>>, key: &str) -> Value {
    map.and_then(|m| m.get(key)).cloned().unwrap_or(Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_payload_key_named_like_a_virtual_property_is_refused() {
        for part in ["nodes", "edges"] {
            for key in VIRTUAL_KEYS {
                let json = serde_json::json!({part: {"x": {"loc": 1, key: "other"}}});
                let error = Payload::from_json(&json).unwrap_err();
                assert!(error.contains(&format!("`{key}`")), "{error}");
            }
        }
        // Only these names: others, also in another case, are the host's.
        let json = serde_json::json!({"nodes": {"x": {"Label": 1, "parent": 2}}});
        assert!(Payload::from_json(&json).is_ok());
    }
}

impl GraphStore for ModelStore<'_> {
    fn node_count(&self) -> usize {
        self.graph.nodes.len()
    }

    fn node_label(&self, node: NodeId) -> &str {
        &self.graph.nodes[node.0 as usize].kind
    }

    fn node_property(&self, node: NodeId, key: &str) -> Value {
        let n = &self.graph.nodes[node.0 as usize];
        match key {
            "id" => Value::String(n.id.clone()),
            "kind" => Value::String(n.kind.clone()),
            "label" => Value::String(n.label.clone()),
            "parentId" => n.parent_id.clone().map_or(Value::Null, Value::String),
            _ => self.payload.node_property(&n.id, key),
        }
    }

    fn edge_count(&self) -> usize {
        self.edges.len()
    }

    fn edge_type(&self, edge: EdgeId) -> &str {
        &self.graph.edges[self.edges[edge.0 as usize].0].kind
    }

    fn edge_property(&self, edge: EdgeId, key: &str) -> Value {
        let e = &self.graph.edges[self.edges[edge.0 as usize].0];
        match key {
            "id" => Value::String(e.id.clone()),
            "kind" => Value::String(e.kind.clone()),
            "weight" => Value::Int(e.count() as i64),
            _ => self.payload.edge_property(&e.id, key),
        }
    }

    fn edges_of(&self, node: NodeId, direction: Direction) -> Vec<EdgeRef> {
        let n = node.0 as usize;
        let of = |ids: &[u32], outgoing: bool| -> Vec<EdgeRef> {
            ids.iter()
                .map(|&e| {
                    let (_, source, target) = self.edges[e as usize];
                    EdgeRef {
                        edge: EdgeId(e),
                        other: NodeId(if outgoing { target } else { source }),
                    }
                })
                .collect()
        };
        match direction {
            Direction::Out => of(&self.out[n], true),
            Direction::In => of(&self.incoming[n], false),
            Direction::Both => {
                // Merge the two ascending lists by edge id; a self-loop is in both and is listed once.
                let (a, b) = (&self.out[n], &self.incoming[n]);
                let (mut i, mut j, mut merged) = (0, 0, Vec::with_capacity(a.len() + b.len()));
                while i < a.len() || j < b.len() {
                    let next_a = a.get(i).copied().unwrap_or(u32::MAX);
                    let next_b = b.get(j).copied().unwrap_or(u32::MAX);
                    if next_a == next_b {
                        merged.push(EdgeRef {
                            edge: EdgeId(next_a),
                            other: node,
                        });
                        i += 1;
                        j += 1;
                    } else if next_a < next_b {
                        merged.push(of(&[next_a], true).remove(0));
                        i += 1;
                    } else {
                        merged.push(of(&[next_b], false).remove(0));
                        j += 1;
                    }
                }
                merged
            }
        }
    }
}
