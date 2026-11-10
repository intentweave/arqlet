//! Curated reference document, schema version 1 (provisional; see docs/curated-references.md sections 6, 7 and 15).
//!
//! A document embeds its baseline snapshot and its curated geometry in absolute world coordinates. `seal` writes the
//! payload hash; `validate` rejects structural corruption with an explicit error, recomputes the geometric metrics with
//! `scene::metrics`, and reports hard violations separately. Nothing here reads files, the network or the clock.

use std::collections::{BTreeMap, BTreeSet};

use arqlet_model::{Edge, Graph, LayoutOptions, Point, PortSide, Rect, Snapshot};
use arqlet_scene::metrics::{measure, DrawEdge, DrawNode, Drawing, Metrics, DEFAULT_TRACK_SPACING};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::canonical::{canonical_json, sha256_hex};
use crate::ports;

pub const FORMAT: &str = "layerweave-curated-reference";
pub const SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Draft,
    Accepted,
    Retired,
}

/// The layout input: what the engine laid out, with the view and options it used.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReferenceInput {
    pub graph: Graph,
    pub collapsed_ids: Vec<String>,
    pub options: LayoutOptions,
}

/// Explicit curated geometry in absolute world coordinates.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CuratedScene {
    /// Node bounds keyed by node id.
    pub nodes: BTreeMap<String, Rect>,
    /// Route polylines keyed by drawn edge id. Bundle members have no route of their own.
    pub routes: BTreeMap<String, Vec<Point>>,
    /// Ports keyed by port id: where each route end attaches to a node side.
    #[serde(default)]
    pub ports: BTreeMap<String, PortRecord>,
    /// Routes whose node moved and that are not repaired yet. Drawn as provisional connectors; a draft with pending
    /// routes is never valid. Saved with the document, so a reopened draft shows the same state.
    #[serde(default)]
    pub pending_routes: Vec<String>,
    /// Routes the curator has locked: the editor keeps their geometry and never marks them pending. Omitted when empty,
    /// so documents without locks keep their hashes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub locked_routes: Vec<String>,
    /// Node sizes the curator has pinned, keyed by node id. The editor keeps a pinned size through resizing. The engine
    /// does not honour size pins yet. Omitted when empty.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub size_pins: BTreeMap<String, SizePin>,
    /// Label positions the curator has moved, keyed by the baseline label id: the top-left corner in world units. The
    /// label keeps its measured size. Label boxes are not measured by the metrics yet. Omitted when empty.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub labels: BTreeMap<String, Point>,
}

/// A pinned node size, in world units.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SizePin {
    pub width: f64,
    pub height: f64,
}

/// One port: the attachment of a route end to a node side. `offset` is the fraction of the side length, as the
/// engine places ports. `position` is derived from the node bounds, the side and the offset.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PortRecord {
    pub node_id: String,
    pub side: PortSide,
    pub edge_id: String,
    /// "source" or "target": which end of the route this port serves.
    pub endpoint: String,
    pub offset: f64,
    pub position: Point,
    /// Explicit ports come from the input configuration and cannot be changed by curation.
    pub explicit: bool,
}

/// Provisional annotation shape; the annotation model of section 5 is not implemented yet.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Annotation {
    pub id: String,
    pub target: String,
    pub text: String,
    /// What the note is about (section 5 of the design). Optional; one of `ANNOTATION_CATEGORIES`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// How much the note matters. Optional; one of `ANNOTATION_SEVERITIES`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub severity: Option<String>,
}

/// The categories a note may name (section 5 of the curated-references design).
pub const ANNOTATION_CATEGORIES: [&str; 11] = [
    "overlap",
    "node-intersection",
    "clearance",
    "port-side-or-order",
    "channel-space",
    "crossing",
    "bend",
    "label",
    "hierarchy",
    "readability",
    "other",
];

/// The severities a note may carry.
pub const ANNOTATION_SEVERITIES: [&str; 3] = ["info", "warning", "error"];

/// Metrics of the curated geometry, computed by the shared validator.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ValidationRecord {
    pub metrics: Metrics,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Integrity {
    /// SHA-256 of the canonical JSON of the document without the `integrity` member.
    pub payload_hash: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReferenceDocument {
    pub format: String,
    pub schema_version: u32,
    pub reference_id: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub revision: u64,
    pub status: Status,
    pub input: ReferenceInput,
    /// SHA-256 of the canonical JSON of `input`.
    pub input_hash: String,
    pub baseline: Snapshot,
    pub curated: CuratedScene,
    pub annotations: Vec<Annotation>,
    pub validation: Option<ValidationRecord>,
    pub integrity: Integrity,
}

/// The outcome of validating a document that passed structural checks.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct Report {
    /// No hard geometric violation, and the stored validation matches the current geometry.
    pub valid: bool,
    /// The stored validation record was computed for the current geometry.
    pub current_validation: bool,
    /// Overlaps, route-through-node hits and foreign-container traversals.
    pub hard_violations: usize,
    /// Routes still waiting for repair after a node moved.
    pub pending_routes: usize,
    pub payload_hash: String,
    pub input_hash: String,
    pub metrics: Metrics,
}

/// Creates an unedited draft from an engine snapshot: revision 1, curated geometry equal to the baseline.
pub fn create(snapshot: &Snapshot, reference_id: &str) -> Result<ReferenceDocument, String> {
    if reference_id.trim().is_empty() {
        return Err("referenceId must not be empty".into());
    }
    // The curated geometry holds the corners of every route (the arcs of a rounded style are drawn by the export), so the metrics
    // count bends at corners, not at the samples of an arc.
    let scene = arqlet_scene::build_corner_scene(snapshot)?;
    let nodes: BTreeMap<String, Rect> = scene
        .nodes
        .iter()
        .map(|node| (node.node.id.clone(), node.bounds.clone()))
        .collect();
    let routes = scene
        .routes
        .iter()
        .map(|route| (route.id.clone(), route.points.clone()))
        .collect();
    let input = ReferenceInput {
        graph: snapshot.graph.clone(),
        collapsed_ids: snapshot.collapsed_ids.clone(),
        options: snapshot.options.clone(),
    };
    let ports = ports::from_snapshot(&snapshot.resolved_ports, &nodes)?;
    let curated = CuratedScene {
        nodes,
        routes,
        ports,
        pending_routes: Vec::new(),
        locked_routes: Vec::new(),
        size_pins: BTreeMap::new(),
        labels: BTreeMap::new(),
    };
    let metrics = measure(&drawing(&input.graph, &curated)?)?;
    let mut document = ReferenceDocument {
        format: FORMAT.into(),
        schema_version: SCHEMA_VERSION,
        reference_id: reference_id.into(),
        revision: 1,
        status: Status::Draft,
        input_hash: input_hash(&input)?,
        input,
        baseline: snapshot.clone(),
        curated,
        annotations: Vec::new(),
        validation: Some(ValidationRecord { metrics }),
        integrity: Integrity {
            payload_hash: String::new(),
        },
    };
    seal(&mut document)?;
    Ok(document)
}

/// The hash of the canonical input.
pub fn input_hash(input: &ReferenceInput) -> Result<String, String> {
    let value = serde_json::to_value(input).map_err(|e| e.to_string())?;
    Ok(sha256_hex(canonical_json(&value)?.as_bytes()))
}

/// The hash of the canonical document without its integrity member.
pub fn payload_hash(document: &ReferenceDocument) -> Result<String, String> {
    let mut value = serde_json::to_value(document).map_err(|e| e.to_string())?;
    if let Some(object) = value.as_object_mut() {
        object.remove("integrity");
    }
    Ok(sha256_hex(canonical_json(&value)?.as_bytes()))
}

/// Writes the payload hash. Call after every change to the document.
pub fn seal(document: &mut ReferenceDocument) -> Result<(), String> {
    document.integrity.payload_hash = payload_hash(document)?;
    Ok(())
}

/// Parses a document with the envelope checks first, so an unsupported format or schema gets its own error.
fn parse(value: &Value) -> Result<ReferenceDocument, String> {
    match value.get("format").and_then(Value::as_str) {
        Some(FORMAT) => {}
        other => {
            return Err(format!(
                "Unsupported reference format: {}",
                other.unwrap_or("missing")
            ))
        }
    }
    match value.get("schemaVersion").and_then(Value::as_u64) {
        Some(version) if version == u64::from(SCHEMA_VERSION) => {}
        Some(version) => return Err(format!("Unsupported reference schema version {version}")),
        None => return Err("Missing reference schema version".into()),
    }
    serde_json::from_value(value.clone()).map_err(|e| format!("Invalid reference: {e}"))
}

/// Recomputes the validation record of an edited draft and seals it. The input must be intact (its hash and its
/// node and route identities are checked), but the payload hash is not: an edit changes it on purpose. The caller
/// sets the revision; this function does not change it.
pub fn refresh(value: &Value) -> Result<ReferenceDocument, String> {
    let mut document = parse(value)?;
    if document.reference_id.trim().is_empty() {
        return Err("referenceId must not be empty".into());
    }
    if document.revision == 0 {
        return Err("revision must be at least 1".into());
    }
    if input_hash(&document.input)? != document.input_hash {
        return Err("Input hash mismatch: the input does not match its hash".into());
    }
    finish(&mut document)?;
    Ok(document)
}

/// Re-searches the pending routes of a draft, then refreshes and seals it. A draft without pending routes is
/// refused: there is nothing to re-route. The caller sets the revision.
pub fn reroute(value: &Value) -> Result<ReferenceDocument, String> {
    let mut document = parse(value)?;
    if document.curated.pending_routes.is_empty() {
        return Err("There are no pending routes to re-route".into());
    }
    if input_hash(&document.input)? != document.input_hash {
        return Err("Input hash mismatch: the input does not match its hash".into());
    }
    check_identities(&document.input.graph, &document.curated)?;
    ports::sync(&mut document.curated)?;
    ports::reroute(&mut document.curated, &document.input.graph)?;
    finish(&mut document)?;
    Ok(document)
}

/// Syncs the derived port positions, checks the ports and the identities, and recomputes the validation.
/// The caller seals nothing: this function seals.
fn finish(document: &mut ReferenceDocument) -> Result<(), String> {
    check_labels_and_notes(document)?;
    check_identities(&document.input.graph, &document.curated)?;
    ports::sync(&mut document.curated)?;
    ports::check(&document.curated, &document.baseline)?;
    let metrics = measure(&drawing(&document.input.graph, &document.curated)?)?;
    document.validation = Some(ValidationRecord { metrics });
    seal(document)
}

/// Validates a document given as JSON. `Err` means structural rejection: the document must not be stored or
/// accepted. `Ok` reports geometric validity; a hard violation makes the report invalid but still readable.
pub fn validate(value: &Value) -> Result<Report, String> {
    let document = parse(value)?;
    check_labels_and_notes(&document)?;

    if document.reference_id.trim().is_empty() {
        return Err("referenceId must not be empty".into());
    }
    if document.revision == 0 {
        return Err("revision must be at least 1".into());
    }
    if payload_hash(&document)? != document.integrity.payload_hash {
        return Err("Integrity mismatch: the document changed after it was sealed".into());
    }
    if input_hash(&document.input)? != document.input_hash {
        return Err("Input hash mismatch: the input does not match its hash".into());
    }
    if canonical_json(&serde_json::to_value(&document.baseline.graph).map_err(|e| e.to_string())?)?
        != canonical_json(&serde_json::to_value(&document.input.graph).map_err(|e| e.to_string())?)?
    {
        return Err("The baseline graph differs from the input graph".into());
    }
    check_identities(&document.input.graph, &document.curated)?;
    ports::check(&document.curated, &document.baseline)?;

    let metrics = measure(&drawing(&document.input.graph, &document.curated)?)?;
    let current_validation = document
        .validation
        .as_ref()
        .is_some_and(|record| metrics_equal(&record.metrics, &metrics));
    let hard_violations = metrics.sibling_overlaps
        + metrics.cross_branch_overlaps
        + metrics.line_node_hits
        + metrics.container_traversals;
    if matches!(document.status, Status::Accepted) && hard_violations > 0 {
        return Err(format!(
            "An accepted reference cannot have hard violations ({hard_violations})"
        ));
    }
    Ok(Report {
        valid: hard_violations == 0
            && current_validation
            && document.curated.pending_routes.is_empty(),
        pending_routes: document.curated.pending_routes.len(),
        current_validation,
        hard_violations,
        payload_hash: document.integrity.payload_hash.clone(),
        input_hash: document.input_hash.clone(),
        metrics,
    })
}

/// The canonical text of a document: validated first, so only well-formed documents get a saved form.
pub fn canonical_text(value: &Value) -> Result<String, String> {
    validate(value)?;
    canonical_json(value)
}

/// Checks the labels and the notes of a document. A note is curation evidence: it never changes geometry. Its target
/// must be a node, a route or a baseline label of the reference.
fn check_labels_and_notes(document: &ReferenceDocument) -> Result<(), String> {
    let baseline_labels: BTreeSet<&str> = document
        .baseline
        .labels
        .iter()
        .map(|label| label.id.as_str())
        .collect();
    for (id, point) in &document.curated.labels {
        if !baseline_labels.contains(id.as_str()) {
            return Err(format!("Label {id} names no label of the baseline"));
        }
        if !(point.x.is_finite() && point.y.is_finite()) {
            return Err(format!("Label {id} has a position that is not finite"));
        }
    }
    let mut ids = BTreeSet::new();
    for note in &document.annotations {
        if !ids.insert(note.id.as_str()) {
            return Err(format!("Annotation {} is listed twice", note.id));
        }
        if note.text.trim().is_empty() {
            return Err(format!("Annotation {} has no text", note.id));
        }
        if let Some(category) = &note.category {
            if !ANNOTATION_CATEGORIES.contains(&category.as_str()) {
                return Err(format!(
                    "Annotation {} names an unknown category {category}",
                    note.id
                ));
            }
        }
        if let Some(severity) = &note.severity {
            if !ANNOTATION_SEVERITIES.contains(&severity.as_str()) {
                return Err(format!(
                    "Annotation {} names an unknown severity {severity}",
                    note.id
                ));
            }
        }
        let target = note.target.as_str();
        let known = document.curated.nodes.contains_key(target)
            || document.curated.routes.contains_key(target)
            || baseline_labels.contains(target);
        if !known {
            return Err(format!(
                "Annotation {} targets {target}, which is not in the reference",
                note.id
            ));
        }
    }
    Ok(())
}

fn check_identities(graph: &Graph, curated: &CuratedScene) -> Result<(), String> {
    let mut node_ids = BTreeSet::new();
    for node in &graph.nodes {
        if !node_ids.insert(node.id.as_str()) {
            return Err(format!("Duplicate node id {}", node.id));
        }
    }
    let mut edge_ids = BTreeSet::new();
    for edge in &graph.edges {
        if !edge_ids.insert(edge.id.as_str()) {
            return Err(format!("Duplicate edge id {}", edge.id));
        }
    }
    let curated_nodes: BTreeSet<&str> = curated.nodes.keys().map(String::as_str).collect();
    if curated_nodes != node_ids {
        return Err("Curated node set does not match the input graph".into());
    }
    for route in curated.routes.keys() {
        match graph.edges.iter().find(|edge| edge.id == *route) {
            Some(Edge {
                target: Some(_), ..
            }) => {}
            Some(_) => return Err(format!("Route {route} belongs to an unresolved edge")),
            None => return Err(format!("Route {route} names no edge in the input graph")),
        }
    }
    for pending in &curated.pending_routes {
        if !curated.routes.contains_key(pending) {
            return Err(format!("Pending route {pending} has no route"));
        }
    }
    let mut locked = BTreeSet::new();
    for route in &curated.locked_routes {
        if !curated.routes.contains_key(route) {
            return Err(format!("Locked route {route} has no route"));
        }
        if !locked.insert(route.as_str()) {
            return Err(format!("Route {route} is locked twice"));
        }
        if curated.pending_routes.contains(route) {
            return Err(format!(
                "Locked route {route} is pending: unlock it before its node moves"
            ));
        }
    }
    for (node, pin) in &curated.size_pins {
        let Some(rect) = curated.nodes.get(node) else {
            return Err(format!("Size pin on {node} names no node"));
        };
        if !(pin.width.is_finite() && pin.height.is_finite() && pin.width > 0. && pin.height > 0.) {
            return Err(format!("Size pin on {node} must be finite and positive"));
        }
        if (rect.width - pin.width).abs() > 1e-9 || (rect.height - pin.height).abs() > 1e-9 {
            return Err(format!("Size pin on {node} disagrees with its node"));
        }
    }
    Ok(())
}

fn drawing(graph: &Graph, curated: &CuratedScene) -> Result<Drawing, String> {
    let parents: BTreeSet<&str> = graph
        .nodes
        .iter()
        .filter_map(|node| node.parent_id.as_deref())
        .collect();
    let mut nodes = Vec::with_capacity(graph.nodes.len());
    for node in &graph.nodes {
        let bounds = curated
            .nodes
            .get(&node.id)
            .ok_or_else(|| format!("Node {} has no curated geometry", node.id))?;
        nodes.push(DrawNode {
            id: node.id.clone(),
            parent_id: node.parent_id.clone(),
            container: parents.contains(node.id.as_str()),
            x: bounds.x,
            y: bounds.y,
            width: bounds.width,
            height: bounds.height,
        });
    }
    let mut edges = Vec::with_capacity(curated.routes.len());
    for (id, points) in &curated.routes {
        let edge = graph
            .edges
            .iter()
            .find(|edge| edge.id == *id)
            .ok_or_else(|| format!("Route {id} names no edge"))?;
        let target = edge
            .target
            .clone()
            .ok_or_else(|| format!("Route {id} belongs to an unresolved edge"))?;
        let ports = graph.edge_ports.iter().find(|ports| ports.edge_id == *id);
        edges.push(DrawEdge {
            id: id.clone(),
            source: edge.source.clone(),
            target,
            points: points.clone(),
            source_port: ports.and_then(|ports| ports.source_port.clone()),
            target_port: ports.and_then(|ports| ports.target_port.clone()),
            bundle_id: None,
        });
    }
    Ok(Drawing {
        nodes,
        edges,
        track_spacing: DEFAULT_TRACK_SPACING,
        buses: Vec::new(),
    })
}

fn metrics_equal(stored: &Metrics, current: &Metrics) -> bool {
    serde_json::to_value(stored).ok() == serde_json::to_value(current).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn snapshot() -> Snapshot {
        serde_json::from_str(include_str!(
            "../../../packages/viewer/fixtures/state-tcp.snapshot.json"
        ))
        .unwrap()
    }

    fn created() -> ReferenceDocument {
        create(&snapshot(), "state-tcp").unwrap()
    }

    fn value_of(document: &ReferenceDocument) -> Value {
        serde_json::to_value(document).unwrap()
    }

    /// Edits the typed document and writes a fresh seal, so only the intended rule can fail.
    fn resealed(edit: impl FnOnce(&mut ReferenceDocument)) -> Value {
        let mut document = created();
        edit(&mut document);
        seal(&mut document).unwrap();
        value_of(&document)
    }

    #[test]
    fn unedited_snapshot_round_trips_through_canonical_text() {
        let value = value_of(&created());
        let report = validate(&value).unwrap();
        let text = canonical_text(&value).unwrap();
        let reparsed: Value = serde_json::from_str(&text).unwrap();
        let again = validate(&reparsed).unwrap();
        assert_eq!(again.payload_hash, report.payload_hash);
        assert_eq!(again.input_hash, report.input_hash);
        assert_eq!(canonical_text(&reparsed).unwrap(), text);
    }

    #[test]
    fn creation_from_the_same_snapshot_is_deterministic() {
        let first = canonical_text(&value_of(&created())).unwrap();
        let second = canonical_text(&value_of(&created())).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn a_stored_validation_matches_the_geometry_it_was_computed_for() {
        let report = validate(&value_of(&created())).unwrap();
        assert!(report.current_validation);
    }

    #[test]
    fn changed_bytes_fail_the_integrity_check() {
        let mut value = value_of(&created());
        let node = value["curated"]["nodes"]
            .as_object_mut()
            .unwrap()
            .values_mut()
            .next()
            .unwrap();
        node["x"] = json!(node["x"].as_f64().unwrap() + 1.0);
        let error = validate(&value).unwrap_err();
        assert!(error.starts_with("Integrity mismatch"), "{error}");
    }

    #[test]
    fn a_resealed_route_change_leaves_the_validation_stale() {
        let value = resealed(|document| {
            // An interior point: the endpoints are port attachments and may not move on their own.
            let route = document
                .curated
                .routes
                .values_mut()
                .find(|route| {
                    (1..route.len().saturating_sub(2)).any(|i| route[i].y == route[i + 1].y)
                })
                .unwrap();
            // A whole interior horizontal segment moves, so the route stays orthogonal.
            let index = (1..route.len() - 2)
                .find(|&i| route[i].y == route[i + 1].y)
                .unwrap();
            route[index].y += 50.0;
            route[index + 1].y += 50.0;
        });
        let report = validate(&value).unwrap();
        assert!(!report.current_validation);
        assert!(!report.valid);
    }

    #[test]
    fn an_unsupported_schema_version_is_rejected_explicitly() {
        let mut value = value_of(&created());
        value["schemaVersion"] = json!(2);
        assert_eq!(
            validate(&value).unwrap_err(),
            "Unsupported reference schema version 2"
        );
    }

    #[test]
    fn a_foreign_format_is_rejected_explicitly() {
        let mut value = value_of(&created());
        value["format"] = json!("something-else");
        assert_eq!(
            validate(&value).unwrap_err(),
            "Unsupported reference format: something-else"
        );
    }

    #[test]
    fn unknown_members_are_rejected() {
        let mut value = value_of(&created());
        value["surprise"] = json!(true);
        assert!(validate(&value)
            .unwrap_err()
            .starts_with("Invalid reference"));
    }

    #[test]
    fn a_curated_node_set_that_differs_from_the_input_is_rejected() {
        let value = resealed(|document| {
            let first = document.curated.nodes.keys().next().unwrap().clone();
            document.curated.nodes.remove(&first);
        });
        assert_eq!(
            validate(&value).unwrap_err(),
            "Curated node set does not match the input graph"
        );
    }

    #[test]
    fn a_route_for_an_unknown_edge_is_rejected() {
        let value = resealed(|document| {
            let points = document.curated.routes.values().next().unwrap().clone();
            document
                .curated
                .routes
                .insert("no-such-edge".into(), points);
        });
        assert_eq!(
            validate(&value).unwrap_err(),
            "Route no-such-edge names no edge in the input graph"
        );
    }

    #[test]
    fn a_changed_input_without_a_new_input_hash_is_rejected() {
        let value = resealed(|document| {
            document.input.collapsed_ids.push("extra".into());
        });
        assert!(validate(&value)
            .unwrap_err()
            .starts_with("Input hash mismatch"));
    }

    #[test]
    fn canonical_text_refuses_structurally_invalid_documents() {
        let mut value = value_of(&created());
        value["revision"] = json!(0);
        assert_eq!(
            canonical_text(&value).unwrap_err(),
            "revision must be at least 1"
        );
    }

    /// Two leaves with one parent, moved onto the same rectangle: a sibling overlap, a hard violation.
    fn overlapping(document: &mut ReferenceDocument) {
        let parent_of: BTreeMap<&str, Option<&str>> = document
            .input
            .graph
            .nodes
            .iter()
            .map(|node| (node.id.as_str(), node.parent_id.as_deref()))
            .collect();
        let containers: BTreeSet<&str> = parent_of.values().flatten().copied().collect();
        let leaves: Vec<&str> = parent_of
            .keys()
            .copied()
            .filter(|id| !containers.contains(id))
            .collect();
        let (first, second) = leaves
            .iter()
            .flat_map(|a| leaves.iter().map(move |b| (*a, *b)))
            .find(|(a, b)| a != b && parent_of[a] == parent_of[b])
            .expect("fixture has two leaves with the same parent");
        let rect = document.curated.nodes[first].clone();
        document.curated.nodes.insert(second.into(), rect);
        // What the editor does when a node moves: its routes become pending and its ports follow it.
        let touching: Vec<String> = document
            .input
            .graph
            .edges
            .iter()
            .filter(|edge| edge.source == second || edge.target.as_deref() == Some(second))
            .map(|edge| edge.id.clone())
            .filter(|id| document.curated.routes.contains_key(id))
            .collect();
        document.curated.pending_routes.extend(touching);
        ports::sync(&mut document.curated).unwrap();
    }

    #[test]
    fn a_draft_with_hard_violations_is_reported_not_rejected() {
        let value = resealed(overlapping);
        let report = validate(&value).unwrap();
        assert!(report.hard_violations > 0);
        assert!(!report.valid);
    }

    #[test]
    fn an_accepted_reference_cannot_carry_hard_violations() {
        let value = resealed(|document| {
            overlapping(document);
            document.status = Status::Accepted;
        });
        assert!(validate(&value)
            .unwrap_err()
            .starts_with("An accepted reference cannot have hard violations"));
    }

    #[test]
    fn refresh_reseals_an_edited_draft_with_current_validation() {
        let mut value = value_of(&created());
        let route = value["curated"]["routes"]
            .as_object_mut()
            .unwrap()
            .values_mut()
            .find(|route| {
                let points = route.as_array().unwrap();
                (1..points.len().saturating_sub(2)).any(|i| points[i]["y"] == points[i + 1]["y"])
            })
            .unwrap();
        let count = route.as_array().unwrap().len();
        let index = (1..count - 2)
            .find(|&i| route[i]["y"] == route[i + 1]["y"])
            .unwrap();
        for at in [index, index + 1] {
            route[at]["y"] = json!(route[at]["y"].as_f64().unwrap() + 50.0);
        }
        assert!(validate(&value)
            .unwrap_err()
            .starts_with("Integrity mismatch"));
        let edited = value_of(&refresh(&value).unwrap());
        assert!(validate(&edited).unwrap().current_validation);
    }

    #[test]
    fn refresh_still_checks_the_input_hash() {
        let mut value = value_of(&created());
        value["input"]["collapsedIds"] = json!(["changed"]);
        assert!(refresh(&value)
            .unwrap_err()
            .starts_with("Input hash mismatch"));
    }

    #[test]
    fn refresh_still_checks_the_node_set() {
        let mut value = value_of(&created());
        let nodes = value["curated"]["nodes"].as_object_mut().unwrap();
        let first = nodes.keys().next().unwrap().clone();
        nodes.remove(&first);
        assert_eq!(
            refresh(&value).unwrap_err(),
            "Curated node set does not match the input graph"
        );
    }

    #[test]
    fn a_pending_route_keeps_the_draft_invalid_but_readable() {
        let value = resealed(|document| {
            let route = document.curated.routes.keys().next().unwrap().clone();
            document.curated.pending_routes.push(route);
        });
        let report = validate(&value).unwrap();
        assert_eq!(report.pending_routes, 1);
        assert!(!report.valid);
    }

    #[test]
    fn a_pending_route_must_name_a_route() {
        let value = resealed(|document| {
            document.curated.pending_routes.push("no-such-route".into());
        });
        assert_eq!(
            validate(&value).unwrap_err(),
            "Pending route no-such-route has no route"
        );
    }
}
