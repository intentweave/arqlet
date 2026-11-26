//! Provisional (search): the `find` operation. The nodes of the source graph whose label or id contains a query, best
//! first, with the box of the current view that shows each, so that a host can find a node the view hides and `reveal`
//! it. Read only: the snapshot is not changed.
use crate::trace::{answered, navigable, ViewMap};
use arqlet_model::{FindAnswer, FindMatch, Snapshot};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

/// How many matches `find` lists unless the request says otherwise (`limit`).
const DEFAULT_LIMIT: usize = 20;

pub fn find_response(v: &Value) -> Value {
    match find_answer(v).and_then(|answer| answered(&answer)) {
        Ok(result) => result,
        Err(error) => json!({"ok": false, "error": error}),
    }
}

/// The matches of the request `v` (`query`, optional `limit` and `nodeKinds`) on a snapshot the caller holds.
/// `find_on` with the session's content (ADR 0023, R1): with `key`, the query is searched in that property of every node
/// instead of in labels and ids.
pub fn find_on_with(
    snapshot: &Snapshot,
    v: &Value,
    content: Option<&crate::content::Content>,
) -> Result<Value, String> {
    find_answer_on_with(snapshot, v, content).and_then(|answer| answered(&answer))
}

pub fn find_on(snapshot: &Snapshot, v: &Value) -> Result<Value, String> {
    find_answer_on(snapshot, v).and_then(|answer| answered(&answer))
}

/// The answer of a `find` request that carries its snapshot.
pub fn find_answer(v: &Value) -> Result<FindAnswer, String> {
    let snapshot: Snapshot =
        serde_json::from_value(v["snapshot"].clone()).map_err(|e| e.to_string())?;
    find_answer_on(&snapshot, v)
}

/// How well `text` (lower case) matches `query` (lower case): 0 equal, 1 prefix, 2 at a word start, 3 anywhere.
fn rank(text: &str, query: &str) -> Option<u8> {
    if text == query {
        return Some(0);
    }
    if text.starts_with(query) {
        return Some(1);
    }
    let mut best = None;
    for (at, _) in text.match_indices(query) {
        let word_start = text[..at]
            .chars()
            .next_back()
            .is_none_or(|c| !c.is_alphanumeric());
        let r = if word_start { 2 } else { 3 };
        best = Some(best.map_or(r, |b: u8| b.min(r)));
        if r == 2 {
            break;
        }
    }
    best
}

/// The typed answer of `find_on`.
pub fn find_answer_on(snapshot: &Snapshot, v: &Value) -> Result<FindAnswer, String> {
    find_answer_on_with(snapshot, v, None)
}

/// The text a property value is searched as: a string as it is, a number or a boolean as JSON writes it; lists, maps and
/// null are not searched.
fn searchable(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.to_lowercase()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

/// `find_answer_on` with the session's content.
pub fn find_answer_on_with(
    snapshot: &Snapshot,
    v: &Value,
    content: Option<&crate::content::Content>,
) -> Result<FindAnswer, String> {
    let key = v["key"].as_str();
    if key.is_some() && content.is_none() {
        return Err("Find by key needs the session's content (payload)".into());
    }
    let query = v["query"]
        .as_str()
        .ok_or("Missing query")?
        .trim()
        .to_lowercase();
    if query.is_empty() {
        return Err("Find needs a query".into());
    }
    let limit = match v.get("limit") {
        None | Some(Value::Null) => DEFAULT_LIMIT,
        Some(l) => l
            .as_u64()
            .ok_or("Find limit must be a whole number")?
            .min(usize::MAX as u64) as usize,
    };
    let kinds: Option<BTreeSet<String>> = match v.get("nodeKinds") {
        None | Some(Value::Null) => None,
        Some(k) => Some(
            serde_json::from_value(k.clone())
                .map_err(|_| "Find nodeKinds must be a list of node kinds")?,
        ),
    };
    let navigable = navigable(snapshot);
    let canonical = navigable.as_ref();
    // Ranked as (rank, label in lower case, id), so that equal matches come in a fixed order.
    let mut found: Vec<(u8, String, &arqlet_model::Node)> = canonical
        .nodes
        .iter()
        .filter(|n| kinds.as_ref().is_none_or(|k| k.contains(&n.kind)))
        .filter_map(|n| {
            let label = n.label.to_lowercase();
            if let Some(key) = key {
                // Ranked by how the property's value matches; ties by label and id as below.
                let value = content
                    .and_then(|c| c.node(&n.id))
                    .and_then(|p| p.get(key))?;
                let best = rank(&searchable(value)?, &query)?;
                return Some((best, label, n));
            }
            let best = match (rank(&label, &query), rank(&n.id.to_lowercase(), &query)) {
                (Some(a), Some(b)) => a.min(b),
                (Some(a), None) | (None, Some(a)) => a,
                (None, None) => return None,
            };
            Some((best, label, n))
        })
        .collect();
    found.sort_by(|a, b| (a.0, &a.1, &a.2.id).cmp(&(b.0, &b.1, &b.2.id)));
    let total = found.len();
    found.truncate(limit);
    let parent_of: BTreeMap<&str, &str> = canonical
        .nodes
        .iter()
        .filter_map(|n| Some((n.id.as_str(), n.parent_id.as_deref()?)))
        .collect();
    let map = ViewMap::new(snapshot, canonical)?;
    let matches = found
        .into_iter()
        .map(|(_, _, n)| {
            let mut parents = Vec::new();
            let mut at = parent_of.get(n.id.as_str()).copied();
            while let Some(p) = at {
                if parents.len() > canonical.nodes.len() {
                    break;
                }
                parents.push(p.to_string());
                at = parent_of.get(p).copied();
            }
            parents.reverse();
            FindMatch {
                id: n.id.clone(),
                label: n.label.clone(),
                kind: n.kind.clone(),
                parents,
                shown_as: map.owner(&n.id),
                value: key.and_then(|key| content?.node(&n.id)?.get(key).cloned()),
            }
        })
        .collect();
    Ok(FindAnswer {
        matches,
        total,
        content_revision: key.and(content.map(|c| c.revision)),
    })
}

/// Provisional (bookmarks): the `digest` operation (read only): the fingerprint of the snapshot's source graph
/// (`arqlet_model::source_digest`) as it is, with the shapes, directions, strategies, routing and ports the view changed.
/// A bookmark keeps it; whether a bookmark belongs to a graph is answered by `readBookmark` (`sameSource`), since another
/// digest alone can come from those changes.
pub fn digest_on(snapshot: &Snapshot) -> Value {
    let digest = snapshot.source_digest.clone().unwrap_or_else(|| {
        arqlet_model::source_digest(snapshot.source_graph.as_deref().unwrap_or(&snapshot.graph))
    });
    json!({"ok": true, "digest": digest})
}

/// The `digest` of a request that carries its snapshot.
pub fn digest_response(v: &Value) -> Value {
    match serde_json::from_value::<Snapshot>(v["snapshot"].clone()) {
        Ok(snapshot) => digest_on(&snapshot),
        Err(error) => json!({"ok": false, "error": error.to_string()}),
    }
}
