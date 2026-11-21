//! Provisional (ADR 0023, step R1): the content of a graph, its properties per node and per edge, kept by a session beside
//! the structure. Layout never reads it, and the snapshot does not carry it, so a serialized snapshot is unchanged with or
//! without content. `details` answers the properties, `find` can search a named key, and the query layer of ADR 0021 reads
//! them later. Content has its own revision: every change that changes something advances it.
//!
//! Ids the structure does not have are **kept and counted**, never refused or dropped (ADR 0023 §1): they take effect when
//! a later change adds the node or edge. A key named like a property the model already has (`id`, `kind`, `label`,
//! `parentId`, `weight`) is refused, not silently overridden (ADR 0021, M0 item 1).
use arqlet_model::Graph;
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};

/// The keys a query sees from the model itself; content may not use them.
pub const VIRTUAL_KEYS: [&str; 5] = ["id", "kind", "label", "parentId", "weight"];

/// Properties by id: id → key → value. Values are JSON (null, boolean, integer, float, string, list, map); an integer stays
/// an integer.
pub type Properties = BTreeMap<String, BTreeMap<String, Value>>;

/// The content of a session: properties of nodes and of edges, and the revision of the last change.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Content {
    pub revision: u64,
    pub nodes: Properties,
    pub edges: Properties,
}

impl Content {
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty() && self.edges.is_empty()
    }

    /// The properties of a node, if it has any.
    pub fn node(&self, id: &str) -> Option<&BTreeMap<String, Value>> {
        self.nodes.get(id)
    }

    /// The properties of an edge, if it has any.
    pub fn edge(&self, id: &str) -> Option<&BTreeMap<String, Value>> {
        self.edges.get(id)
    }

    /// Applies a `payload` request (`set`, `unset`, `clear`, optional `expectedContentRevision`) as one change: all of it
    /// or, on an error, nothing. Answers the new revision and the counts against `graph` (the session's source graph).
    pub fn apply(&mut self, v: &Value, graph: Option<&Graph>) -> Result<Value, String> {
        if let Some(expected) = v.get("expectedContentRevision").and_then(Value::as_u64) {
            if expected != self.revision {
                return Err(format!(
                    "Stale content: expected revision {expected}, the content is at {}",
                    self.revision
                ));
            }
        }
        let mut next = self.clone();
        let mut changed = false;
        if v.get("clear").and_then(Value::as_bool) == Some(true) {
            changed |= !next.is_empty();
            next.nodes.clear();
            next.edges.clear();
        }
        for (part, table) in [("nodes", &mut next.nodes), ("edges", &mut next.edges)] {
            if let Some(set) = v.get("set").and_then(|s| s.get(part)) {
                let set = set
                    .as_object()
                    .ok_or_else(|| format!("payload set.{part} must map ids to properties"))?;
                for (id, properties) in set {
                    let properties = properties.as_object().ok_or_else(|| {
                        format!("payload set.{part}.{id} must map keys to values")
                    })?;
                    for (key, value) in properties {
                        if VIRTUAL_KEYS.contains(&key.as_str()) {
                            return Err(format!(
                                "payload key {key} is a property of the model itself (on {id}); use another name"
                            ));
                        }
                        let entry = table.entry(id.clone()).or_default();
                        if entry.get(key) != Some(value) {
                            entry.insert(key.clone(), value.clone());
                            changed = true;
                        }
                    }
                }
            }
            if let Some(unset) = v.get("unset").and_then(|s| s.get(part)) {
                let unset = unset
                    .as_object()
                    .ok_or_else(|| format!("payload unset.{part} must map ids to lists of keys"))?;
                for (id, keys) in unset {
                    let keys = keys.as_array().ok_or_else(|| {
                        format!("payload unset.{part}.{id} must be a list of keys")
                    })?;
                    if let Some(entry) = table.get_mut(id) {
                        for key in keys {
                            let key = key.as_str().ok_or("payload keys are strings")?;
                            changed |= entry.remove(key).is_some();
                        }
                        if entry.is_empty() {
                            table.remove(id);
                        }
                    }
                }
            }
        }
        if changed {
            next.revision = self
                .revision
                .checked_add(1)
                .ok_or("Content revision overflow")?;
            *self = next;
        }
        Ok(self.summary(graph))
    }

    /// The revision and the counts: ids with properties, keys, and ids the structure does not have (kept, counted).
    pub fn summary(&self, graph: Option<&Graph>) -> Value {
        let node_ids: Option<BTreeSet<&str>> =
            graph.map(|g| g.nodes.iter().map(|n| n.id.as_str()).collect());
        let edge_ids: Option<BTreeSet<&str>> =
            graph.map(|g| g.edges.iter().map(|e| e.id.as_str()).collect());
        let unknown = |table: &Properties, ids: &Option<BTreeSet<&str>>| {
            ids.as_ref().map_or(0, |ids| {
                table.keys().filter(|id| !ids.contains(id.as_str())).count()
            })
        };
        let keys: BTreeSet<&str> = self
            .nodes
            .values()
            .chain(self.edges.values())
            .flat_map(|p| p.keys().map(String::as_str))
            .collect();
        json!({
            "ok": true,
            "contentRevision": self.revision,
            "nodes": self.nodes.len(),
            "edges": self.edges.len(),
            "keys": keys.into_iter().collect::<Vec<_>>(),
            "unknownNodes": unknown(&self.nodes, &node_ids),
            "unknownEdges": unknown(&self.edges, &edge_ids),
        })
    }

    /// The properties of `id` as a JSON map, for an answer (empty map when it has none).
    pub fn as_map(properties: Option<&BTreeMap<String, Value>>) -> Map<String, Value> {
        properties
            .map(|p| p.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default()
    }
}
