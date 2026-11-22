//! Provisional (ADR 0019, step 1): the engine's protocol as types. Every operation is a variant of [`Request`]; JSON is
//! its serialization (`{"op": "toggle", "snapshot": …, "nodeId": …}`), unchanged from before the types existed.
//! [`dispatch`](crate::dispatch) parses a request, [`handle`]s it, and writes the [`Response`].
//!
//! The operations on a snapshot and the import are typed field by field. The read-only and tool operations (scene, trace,
//! overview, ELK, metrics, refinement of parts, references) still carry their fields as a JSON object (`body`) that their
//! handlers read; they are typed in a later step without changing their JSON.
use arqlet_model::{
    DirectionConstraint, Graph, Intent, Measurement, MoveStep, NodeShape, NodeSize, PortSide,
    RoutingConstraint, Snapshot, StrategyConstraint,
};
use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::BTreeMap;

/// One request to the engine. Serializes to the JSON that `dispatch` reads; [`Request::from_json`] reads it back.
/// Provisional (paging, `reveal` with `more`): the next `count` children of `container` from its rest.
#[derive(Clone, Debug, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RevealMore {
    pub container: String,
    pub count: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "op", rename_all = "camelCase", rename_all_fields = "camelCase")]
#[non_exhaustive]
pub enum Request {
    /// The engine's default options.
    OptionDefaults,
    /// Lay out a graph: at most `limit` nodes, with `options` as a patch over the defaults.
    Import {
        graph: Box<Graph>,
        limit: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        options: Option<Value>,
    },
    /// Set (or clear, `None`) the direction of a container (`None`: the root).
    Direction {
        #[serde(flatten)]
        on: Edit,
        node_id: Option<String>,
        direction: Option<DirectionConstraint>,
    },
    /// Set (or clear) the layout strategy of a container.
    Strategy {
        #[serde(flatten)]
        on: Edit,
        node_id: Option<String>,
        strategy: Option<StrategyConstraint>,
    },
    /// Set (or clear) the routing of a container.
    Routing {
        #[serde(flatten)]
        on: Edit,
        node_id: Option<String>,
        routing: Option<RoutingConstraint>,
    },
    /// One step of a move gesture.
    Move {
        #[serde(flatten)]
        on: Edit,
        #[serde(flatten)]
        step: Box<MoveStep>,
    },
    /// Add a node connected to `source_id` (no layout version: an add is a source edit).
    Add {
        snapshot: Box<Snapshot>,
        expected_revision: u64,
        source_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        label: Option<String>,
    },
    /// Collapse or expand a container.
    Toggle {
        #[serde(flatten)]
        on: Edit,
        node_id: String,
    },
    /// Hide nodes by kind or id and edges by kind, keeping the place of the rest (`hide` replaces what was hidden).
    Filter {
        #[serde(flatten)]
        on: Edit,
        hide: arqlet_model::Hidden,
    },
    /// Change the source graph without a new import: add nodes and edges, remove nodes (with what they hold) and edges.
    Patch {
        #[serde(flatten)]
        on: Edit,
        patch: arqlet_model::GraphPatch,
    },
    /// Show the given nodes by opening the containers that hold them (`partial`, default true: only as far as needed).
    Reveal {
        #[serde(flatten)]
        on: Edit,
        node_ids: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        partial: Option<bool>,
        /// Provisional (paging): instead of `node_ids`, the next `count` children of a container shown in part.
        #[serde(skip_serializing_if = "Option::is_none")]
        more: Option<RevealMore>,
    },
    /// Apply an options patch.
    Configure {
        #[serde(flatten)]
        on: Edit,
        options: Value,
    },
    /// Place (or release, `side: None`) the port of an edge end.
    Port {
        #[serde(flatten)]
        on: Edit,
        edge_id: String,
        endpoint: String,
        side: Option<PortSide>,
        #[serde(skip_serializing_if = "Option::is_none")]
        offset: Option<f64>,
    },
    /// Let force-directed areas settle.
    Settle {
        #[serde(flatten)]
        on: Edit,
    },
    /// Provisional (drag API): pin a node at a position in its parent's frame (default: where it is), or release its pin.
    Pin {
        #[serde(flatten)]
        on: Edit,
        node_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        position: Option<arqlet_model::Point>,
        #[serde(skip_serializing_if = "std::ops::Not::not")]
        unpin: bool,
    },
    /// Restore the decisions of an earlier snapshot (undo).
    Restore {
        #[serde(flatten)]
        on: Edit,
        intent: Box<Intent>,
    },
    /// Change the shape of a node.
    Shape {
        #[serde(flatten)]
        on: Edit,
        node_id: String,
        shape: NodeShape,
    },
    /// Report measured sizes (`sizes`) or outlines (`measurements`) of nodes.
    Measure {
        #[serde(flatten)]
        on: Edit,
        #[serde(flatten)]
        what: Measured,
    },
    /// Refine the layout, within a time (`budgetMs`) or work (`workBudget`) budget.
    Refine {
        #[serde(flatten)]
        on: Edit,
        #[serde(flatten)]
        budget: RefineBudget,
    },
    /// `Refine` that publishes improvements while it runs (where the host listens; else a plain refinement).
    RefineProgressive {
        #[serde(flatten)]
        on: Edit,
        #[serde(flatten)]
        budget: RefineBudget,
    },
    /// The decisions a snapshot carries, for an undo stack.
    Intent { snapshot: Box<Snapshot> },
    /// The scene (drawing) of a snapshot.
    Scene {
        #[serde(flatten)]
        body: Map<String, Value>,
    },
    /// The paths between two nodes (read only).
    Trace {
        #[serde(flatten)]
        body: Map<String, Value>,
    },
    /// The nodes within some hops of nodes (read only, provisional).
    Neighbourhood {
        #[serde(flatten)]
        body: Map<String, Value>,
    },
    /// What the engine knows about a node or an edge of the view (read only, provisional).
    Details {
        #[serde(flatten)]
        body: Map<String, Value>,
    },
    /// The nodes whose label or id contains a query (read only, provisional).
    Find {
        #[serde(flatten)]
        body: Map<String, Value>,
    },
    /// An openCypher query over the source graph and the content (read only, provisional; cargo feature `query`).
    Query {
        #[serde(flatten)]
        body: Map<String, Value>,
    },
    /// A look into a collapsed container without opening it (read only, provisional).
    Peek {
        #[serde(flatten)]
        body: Map<String, Value>,
    },
    /// The fingerprint of the snapshot's source graph (read only, provisional).
    Digest {
        #[serde(flatten)]
        body: Map<String, Value>,
    },
    /// The cluster overview of a whole graph.
    Overview {
        #[serde(flatten)]
        body: Map<String, Value>,
    },
    /// The paths between two nodes of a graph, without a view (for the overview).
    OverviewTrace {
        #[serde(flatten)]
        body: Map<String, Value>,
    },
    /// The nodes within some hops of nodes of a graph, without a view (for the overview).
    OverviewNeighbourhood {
        #[serde(flatten)]
        body: Map<String, Value>,
    },
    /// An ELK-style one-shot layout.
    ElkLayout {
        #[serde(flatten)]
        body: Map<String, Value>,
    },
    /// Quality metrics of a drawing.
    Metrics {
        #[serde(flatten)]
        body: Map<String, Value>,
    },
    /// Provisional: the groups and communities the view of `graph` adds with `options`, as the `groups` option takes them
    /// (`crate::community_groups`), answered as `{groups}`.
    Communities {
        #[serde(flatten)]
        body: Map<String, Value>,
    },
    /// The conflicts of one route in a drawing.
    RouteConflicts {
        #[serde(flatten)]
        body: Map<String, Value>,
    },
    /// Refine the components of a graph.
    ComponentRefine {
        #[serde(flatten)]
        body: Map<String, Value>,
    },
    /// Refine the containers of a graph.
    ContainerRefine {
        #[serde(flatten)]
        body: Map<String, Value>,
    },
    /// Structured refinement of a snapshot.
    StructuredRefine {
        #[serde(flatten)]
        body: Map<String, Value>,
    },
    /// Reference documents (create, refresh, reroute, validate, canonical form, SVG, reproduce, repair).
    ReferenceCreate {
        #[serde(flatten)]
        body: Map<String, Value>,
    },
    ReferenceRefresh {
        #[serde(flatten)]
        body: Map<String, Value>,
    },
    ReferenceReroute {
        #[serde(flatten)]
        body: Map<String, Value>,
    },
    ReferenceValidate {
        #[serde(flatten)]
        body: Map<String, Value>,
    },
    ReferenceCanonical {
        #[serde(flatten)]
        body: Map<String, Value>,
    },
    ReferenceSvg {
        #[serde(flatten)]
        body: Map<String, Value>,
    },
    ReferenceFromSvg {
        #[serde(flatten)]
        body: Map<String, Value>,
    },
    ReferenceReproduce {
        #[serde(flatten)]
        body: Map<String, Value>,
    },
    ReferenceRepair {
        #[serde(flatten)]
        body: Map<String, Value>,
    },
}

/// What every view edit names: the snapshot it applies to and the revision and layout version it expects. An answer to
/// another version is refused, never applied silently.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Edit {
    pub snapshot: Box<Snapshot>,
    pub expected_revision: u64,
    pub expected_layout_version: u64,
}

/// What a `measure` reports: typed outlines, or plain sizes.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub enum Measured {
    Measurements(BTreeMap<String, Measurement>),
    Sizes(BTreeMap<String, NodeSize>),
}

/// The budget of a refinement: worker threads (native only), a time budget and a deterministic work budget.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefineBudget {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threads: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub budget_ms: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub work_budget: Option<Value>,
}

/// The answer to a request.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum Response {
    /// The new snapshot of an operation that changes one (or an import).
    Snapshot(Box<Snapshot>),
    /// A refused or failed request.
    Error(String),
    /// The scene of a snapshot (`scene`).
    Scene(Box<arqlet_scene::Scene>),
    /// The paths between two nodes (`trace`).
    Trace(Box<arqlet_model::TraceAnswer>),
    /// The nodes within some hops of nodes (`neighbourhood`).
    Neighbourhood(Box<arqlet_model::NeighbourhoodAnswer>),
    /// A node or an edge of the view (`details`).
    Details(Box<arqlet_model::DetailsAnswer>),
    /// The nodes a query finds (`find`).
    Find(Box<arqlet_model::FindAnswer>),
    /// The cluster overview of a graph (`overview`).
    Overview(Box<arqlet_model::Overview>),
    /// The decisions a snapshot carries (`intent`).
    Intent(Box<arqlet_model::Intent>),
    /// The metrics of a drawing (`metrics`).
    Metrics(Box<arqlet_scene::metrics::Metrics>),
    /// The JSON answer of an operation whose answer is not typed yet.
    Encoded(String),
}

impl Response {
    /// The JSON text of the answer, as `dispatch` returns it.
    pub fn to_json(&self) -> String {
        match self {
            Response::Snapshot(s) => serde_json::json!({"ok": true, "snapshot": s}).to_string(),
            Response::Error(e) => serde_json::json!({"ok": false, "error": e}).to_string(),
            Response::Scene(s) => serde_json::json!({"ok": true, "scene": s}).to_string(),
            Response::Trace(a) => answered(a),
            Response::Neighbourhood(a) => answered(a),
            Response::Details(a) => answered(a),
            Response::Find(a) => answered(a),
            Response::Overview(o) => serde_json::json!({"ok": true, "overview": o}).to_string(),
            Response::Intent(i) => serde_json::json!({"ok": true, "intent": i}).to_string(),
            Response::Metrics(m) => serde_json::json!({"ok": true, "metrics": m}).to_string(),
            Response::Encoded(text) => text.clone(),
        }
    }
}

/// A typed answer whose fields stand beside `ok` (`trace`, `neighbourhood`).
fn answered<T: serde::Serialize>(answer: &T) -> String {
    match crate::trace::answered(answer) {
        Ok(v) => v.to_string(),
        Err(e) => serde_json::json!({"ok": false, "error": e}).to_string(),
    }
}

/// A typed result, or the error as the answer.
fn typed_answer<T>(result: Result<T, String>, wrap: impl FnOnce(T) -> Response) -> Response {
    match result {
        Ok(value) => wrap(value),
        Err(e) => Response::Error(e),
    }
}

fn body_of(v: &Value) -> Map<String, Value> {
    let mut body = v.as_object().cloned().unwrap_or_default();
    body.remove("op");
    body
}

/// A typed field of a request, read from the value in place (a copy of the value first doubled the work for a large
/// graph or snapshot).
fn typed<T: serde::de::DeserializeOwned>(v: &Value) -> Result<T, String> {
    T::deserialize(v).map_err(|e| e.to_string())
}

/// An `import` request read straight from its text (`Request::from_json`): one pass instead of a JSON value and then
/// types. `options` stays a value, `Some` whenever the field is there (also as `null`), as `v.get("options")` gives it.
#[derive(serde::Deserialize)]
struct ImportText {
    op: String,
    graph: Graph,
    #[serde(default)]
    limit: Value,
    #[serde(default, deserialize_with = "present")]
    options: Option<Value>,
    #[serde(default, rename = "omitSource")]
    omit_source: bool,
}

/// Provisional (ADR 0019): how the source graph travels with a request and its answer, beside the request itself
/// (`Request::from_json_delivered`). The host keeps a large source graph and the snapshots leave it out.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Delivery {
    /// `omitSource` on an import: the answer leaves the source graph out and carries its `sourceDigest`, when the view
    /// kept every node of the graph (`input_nodes`); otherwise the host could not hand back the graph the snapshot keeps.
    pub omit_source: bool,
    pub input_nodes: usize,
    /// The request's snapshot left its source out (`sourceDigest`) and the request brought it as `source`, matching:
    /// the answer leaves it out again, with this digest.
    pub digest: Option<String>,
}

impl Delivery {
    /// Applies the delivery to an answer: a snapshot in it leaves its source graph out and carries the digest.
    pub fn deliver(&self, response: Response) -> Response {
        match response {
            Response::Snapshot(mut snapshot) if self.omit_source || self.digest.is_some() => {
                let Some(source) = snapshot.source_graph.as_deref() else {
                    return Response::Snapshot(snapshot);
                };
                let now = arqlet_model::source_digest(source);
                // An operation that changed the source graph (an added node, a patch) answers with the new graph whole: the
                // host's copy is out of date then.
                if self.digest.as_ref().is_some_and(|before| *before != now)
                    || (self.omit_source && source.nodes.len() != self.input_nodes)
                {
                    snapshot.source_digest = None;
                } else {
                    snapshot.source_graph = None;
                    snapshot.source_digest = Some(now);
                }
                Response::Snapshot(snapshot)
            }
            other => other,
        }
    }
}

/// A snapshot that left its source out (`sourceDigest`, no `sourceGraph`) gets it back from the request's `source`, after
/// the fingerprint is checked; returns the digest, or `None` when the snapshot is whole (or there is none).
fn attach_source(v: &mut Value) -> Result<Option<(String, Graph)>, String> {
    let Some(snapshot) = v.get("snapshot").filter(|s| s.is_object()) else {
        return Ok(None);
    };
    let Some(digest) = snapshot
        .get("sourceDigest")
        .and_then(Value::as_str)
        .map(String::from)
    else {
        return Ok(None);
    };
    if snapshot.get("sourceGraph").is_some_and(|g| !g.is_null()) {
        return Ok(None);
    }
    let source = v.as_object_mut().and_then(|o| o.remove("source")).ok_or(
        "The snapshot left its source graph out (sourceDigest): send that graph as source",
    )?;
    // The graph as the import kept it: the host keeps the graph it sent.
    let mut graph: Graph = typed(&source)?;
    crate::resolved_source(&mut graph);
    if arqlet_model::source_digest(&graph) != digest {
        return Err("The source does not match the snapshot's sourceDigest".into());
    }
    Ok(Some((digest, graph)))
}

fn present<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Value>, D::Error> {
    <Value as serde::Deserialize>::deserialize(d).map(Some)
}

impl Request {
    /// The snapshot a request operates on, when it carries one as a typed value (not in a JSON body).
    pub fn snapshot_mut(&mut self) -> Option<&mut Snapshot> {
        match self {
            Request::Direction { on, .. }
            | Request::Strategy { on, .. }
            | Request::Routing { on, .. }
            | Request::Move { on, .. }
            | Request::Toggle { on, .. }
            | Request::Filter { on, .. }
            | Request::Patch { on, .. }
            | Request::Reveal { on, .. }
            | Request::Configure { on, .. }
            | Request::Port { on, .. }
            | Request::Settle { on, .. }
            | Request::Pin { on, .. }
            | Request::Restore { on, .. }
            | Request::Shape { on, .. }
            | Request::Measure { on, .. }
            | Request::Refine { on, .. }
            | Request::RefineProgressive { on, .. } => Some(&mut on.snapshot),
            Request::Add { snapshot, .. } | Request::Intent { snapshot } => Some(snapshot),
            _ => None,
        }
    }

    /// Reads a request from its JSON text, with the same errors the engine has always given.
    pub fn from_json(input: &str) -> Result<Request, String> {
        Request::from_json_delivered(input).map(|(request, _)| request)
    }

    /// `from_json`, with how the source graph travels (`Delivery`): `omitSource` on an import, and a snapshot whose source
    /// was left out gets it back from `source` (checked against its `sourceDigest`).
    pub fn from_json_delivered(input: &str) -> Result<(Request, Delivery), String> {
        // An import, read in one pass when it is well formed; anything else (and any import the fast reading refuses) goes
        // the general way, which gives the errors the engine has always given.
        if input.contains("\"import\"") {
            if let Ok(mut fast) = serde_json::from_str::<ImportText>(input) {
                // Read from text, a list grows by doubling and keeps up to twice the room it needs; the source graph is
                // kept by the snapshot (and a session) for its lifetime, so it is trimmed to its length.
                fast.graph.shrink_to_fit();
                let limit = fast.limit.as_u64();
                if fast.op == "import" && limit.is_some_and(|l| usize::try_from(l).is_ok()) {
                    let delivery = Delivery {
                        omit_source: fast.omit_source,
                        input_nodes: fast.graph.nodes.len(),
                        digest: None,
                    };
                    return Ok((
                        Request::Import {
                            graph: Box::new(fast.graph),
                            limit: limit.expect("checked"),
                            options: fast.options,
                        },
                        delivery,
                    ));
                }
            }
        }
        let mut v: Value = serde_json::from_str(input).map_err(|e| e.to_string())?;
        let attached = attach_source(&mut v)?;
        // The source goes into a typed snapshot as it is; a request that keeps its snapshot as JSON gets it as JSON.
        let (digest, request) = match attached {
            None => (None, Request::from_value(&v)?),
            Some((digest, graph)) => {
                let mut request = Request::from_value(&v)?;
                match request.snapshot_mut() {
                    Some(snapshot) => snapshot.source_graph = Some(std::sync::Arc::new(graph)),
                    None => {
                        v["snapshot"]["sourceGraph"] =
                            serde_json::to_value(&graph).map_err(|e| e.to_string())?;
                        request = Request::from_value(&v)?;
                    }
                }
                (Some(digest), request)
            }
        };
        let delivery = Delivery {
            omit_source: v["op"].as_str() == Some("import")
                && v["omitSource"].as_bool() == Some(true),
            input_nodes: v["graph"]["nodes"].as_array().map_or(0, Vec::len),
            digest,
        };
        Ok((request, delivery))
    }

    /// Reads a request from its JSON value. The checks run in the engine's historical order, so that a malformed request
    /// gets the same error as before the protocol was typed.
    pub fn from_value(v: &Value) -> Result<Request, String> {
        let body = || body_of(v);
        match v["op"].as_str() {
            Some("optionDefaults") => return Ok(Request::OptionDefaults),
            Some("scene") => return Ok(Request::Scene { body: body() }),
            Some("trace") => return Ok(Request::Trace { body: body() }),
            Some("neighbourhood") => return Ok(Request::Neighbourhood { body: body() }),
            Some("details") => return Ok(Request::Details { body: body() }),
            Some("find") => return Ok(Request::Find { body: body() }),
            Some("digest") => return Ok(Request::Digest { body: body() }),
            Some("peek") => return Ok(Request::Peek { body: body() }),
            Some("query") => return Ok(Request::Query { body: body() }),
            Some("payload") => {
                return Err(
                    "payload needs a resident session (the content is kept by the session)".into(),
                )
            }
            Some("componentRefine") => return Ok(Request::ComponentRefine { body: body() }),
            Some("elkLayout") => return Ok(Request::ElkLayout { body: body() }),
            Some("overview") => return Ok(Request::Overview { body: body() }),
            Some("overviewTrace") => return Ok(Request::OverviewTrace { body: body() }),
            Some("overviewNeighbourhood") => {
                return Ok(Request::OverviewNeighbourhood { body: body() })
            }
            Some("containerRefine") => return Ok(Request::ContainerRefine { body: body() }),
            Some("structuredRefine") => return Ok(Request::StructuredRefine { body: body() }),
            Some("metrics") => return Ok(Request::Metrics { body: body() }),
            Some("communities") => return Ok(Request::Communities { body: body() }),
            Some("routeConflicts") => return Ok(Request::RouteConflicts { body: body() }),
            Some("referenceCreate") => return Ok(Request::ReferenceCreate { body: body() }),
            Some("referenceRefresh") => return Ok(Request::ReferenceRefresh { body: body() }),
            Some("referenceReroute") => return Ok(Request::ReferenceReroute { body: body() }),
            Some("referenceValidate") => return Ok(Request::ReferenceValidate { body: body() }),
            Some("referenceCanonical") => return Ok(Request::ReferenceCanonical { body: body() }),
            Some("referenceSvg") => return Ok(Request::ReferenceSvg { body: body() }),
            Some("referenceFromSvg") => return Ok(Request::ReferenceFromSvg { body: body() }),
            Some("referenceReproduce") => return Ok(Request::ReferenceReproduce { body: body() }),
            Some("referenceRepair") => return Ok(Request::ReferenceRepair { body: body() }),
            Some("intent") => {
                return Ok(Request::Intent {
                    snapshot: Box::new(typed(&v["snapshot"])?),
                })
            }
            Some("import") => {
                let graph: Graph = typed(&v["graph"])?;
                let limit = v["limit"].as_u64().ok_or("Missing limit")?;
                usize::try_from(limit).map_err(|e| e.to_string())?;
                return Ok(Request::Import {
                    graph: Box::new(graph),
                    limit,
                    options: v.get("options").cloned(),
                });
            }
            _ => {}
        }
        // Every other operation changes a snapshot: the snapshot and the expected revision come first.
        let snapshot: Snapshot = typed(&v["snapshot"])?;
        Request::edit_on(v, Box::new(snapshot))
    }

    /// Reads an operation on a snapshot that the caller already holds (a session's), instead of one sent in the request
    /// (ADR 0019, step 3). Any `snapshot` field of the request is ignored.
    pub fn from_value_on(v: &Value, snapshot: Box<Snapshot>) -> Result<Request, String> {
        Request::edit_on(v, snapshot)
    }

    fn edit_on(v: &Value, snapshot: Box<Snapshot>) -> Result<Request, String> {
        let expected_revision = v["expectedRevision"]
            .as_u64()
            .ok_or("Missing expectedRevision")?;
        if v["op"].as_str() == Some("add") {
            return Ok(Request::Add {
                snapshot,
                expected_revision,
                source_id: v["sourceId"].as_str().ok_or("Missing sourceId")?.into(),
                label: v["label"].as_str().map(String::from),
            });
        }
        let known = matches!(
            v["op"].as_str(),
            Some(
                "direction"
                    | "strategy"
                    | "routing"
                    | "move"
                    | "toggle"
                    | "reveal"
                    | "filter"
                    | "patch"
                    | "configure"
                    | "port"
                    | "settle"
                    | "pin"
                    | "restore"
                    | "shape"
                    | "measure"
                    | "refine"
                    | "refineProgressive"
            )
        );
        if !known {
            return Err("Unsupported operation".into());
        }
        let on = Edit {
            snapshot,
            expected_revision,
            expected_layout_version: v["expectedLayoutVersion"]
                .as_u64()
                .ok_or("Missing expectedLayoutVersion")?,
        };
        let optional = |field: &str| -> Option<String> { v[field].as_str().map(String::from) };
        Ok(match v["op"].as_str().unwrap_or_default() {
            "direction" => Request::Direction {
                on,
                node_id: optional("nodeId"),
                direction: if v["direction"].is_null() {
                    None
                } else {
                    Some(typed(&v["direction"])?)
                },
            },
            "strategy" => Request::Strategy {
                on,
                node_id: optional("nodeId"),
                strategy: if v["strategy"].is_null() {
                    None
                } else {
                    Some(arqlet_model::parse_strategy(&v["strategy"]).map_err(|e| e.to_string())?)
                },
            },
            "routing" => Request::Routing {
                on,
                node_id: optional("nodeId"),
                routing: if v["routing"].is_null() {
                    None
                } else {
                    Some(arqlet_model::parse_routing(&v["routing"]).map_err(|e| e.to_string())?)
                },
            },
            "move" => Request::Move {
                on,
                step: Box::new(typed(v)?),
            },
            "toggle" => Request::Toggle {
                on,
                node_id: v["nodeId"].as_str().ok_or("Missing nodeId")?.into(),
            },
            "patch" => Request::Patch {
                on,
                patch: serde_json::from_value(v["patch"].clone()).map_err(|e| {
                    format!("patch must be {{nodes?, edges?, removeNodes?, removeEdges?}}: {e}")
                })?,
            },
            "filter" => Request::Filter {
                on,
                hide: if v["hide"].is_null() {
                    arqlet_model::Hidden::default()
                } else {
                    serde_json::from_value(v["hide"].clone()).map_err(|e| {
                        format!("filter: hide must be {{nodeKinds?, nodes?, edgeKinds?}}: {e}")
                    })?
                },
            },
            "reveal" => {
                let more: Option<RevealMore> = match v.get("more") {
                    None => None,
                    Some(m) => Some(
                        serde_json::from_value(m.clone())
                            .map_err(|_| "reveal more needs {container, count}")?,
                    ),
                };
                Request::Reveal {
                    on,
                    node_ids: if more.is_some() && v.get("nodeIds").is_none() {
                        Vec::new()
                    } else {
                        serde_json::from_value(v["nodeIds"].clone())
                            .map_err(|_| "Missing nodeIds")?
                    },
                    partial: v["partial"].as_bool(),
                    more,
                }
            }
            "configure" => Request::Configure {
                on,
                options: v["options"].clone(),
            },
            "port" => {
                let edge_id = v["edgeId"].as_str().ok_or("Missing edgeId")?.to_string();
                let endpoint = v["endpoint"]
                    .as_str()
                    .ok_or("Missing endpoint")?
                    .to_string();
                let (side, offset) = if v["side"].is_null() {
                    (None, None)
                } else {
                    let side: PortSide = typed(&v["side"])?;
                    (
                        Some(side),
                        Some(v["offset"].as_f64().ok_or("Missing offset")?),
                    )
                };
                Request::Port {
                    on,
                    edge_id,
                    endpoint,
                    side,
                    offset,
                }
            }
            "settle" => Request::Settle { on },
            "pin" => Request::Pin {
                on,
                node_id: v["nodeId"].as_str().ok_or("Missing nodeId")?.into(),
                position: match v.get("position") {
                    None | Some(Value::Null) => None,
                    Some(p) => Some(
                        serde_json::from_value(p.clone())
                            .map_err(|_| "pin: position must be {x, y}")?,
                    ),
                },
                unpin: v["unpin"].as_bool().unwrap_or(false),
            },
            "restore" => Request::Restore {
                on,
                intent: Box::new(typed(&v["intent"])?),
            },
            "shape" => Request::Shape {
                node_id: v["nodeId"].as_str().ok_or("Missing nodeId")?.into(),
                shape: typed(&v["shape"])?,
                on,
            },
            "measure" => Request::Measure {
                on,
                what: if v.get("measurements").is_some() {
                    Measured::Measurements(typed(&v["measurements"])?)
                } else {
                    Measured::Sizes(typed(&v["sizes"])?)
                },
            },
            op => {
                let budget = RefineBudget {
                    threads: v["threads"].as_u64(),
                    budget_ms: v.get("budgetMs").cloned(),
                    work_budget: v.get("workBudget").cloned(),
                };
                if op == "refine" {
                    Request::Refine { on, budget }
                } else {
                    Request::RefineProgressive { on, budget }
                }
            }
        })
    }

    /// The JSON value of the request, as `dispatch` reads it.
    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).expect("a request serializes")
    }
}

/// Handles one request.
pub fn handle(mut request: Request) -> Response {
    // Provisional (bus lines, ADR 0019 view model): an operation continues the layout of the graph with the bus nodes, and
    // every snapshot it answers is the view again, with its bars and taps drawn.
    if let Some(snapshot) = request.snapshot_mut() {
        arqlet_layout::expand_bus_lines(snapshot);
    }
    match handle_request(request) {
        Response::Snapshot(mut snapshot) => {
            arqlet_layout::apply_bus_lines(&mut snapshot);
            Response::Snapshot(snapshot)
        }
        other => other,
    }
}

fn handle_request(request: Request) -> Response {
    use crate::*;
    let snapshot = |r: Result<Snapshot, String>| match r {
        Ok(s) => Response::Snapshot(Box::new(s)),
        Err(e) => Response::Error(e),
    };
    // The handlers that still read their fields from JSON get the request as it came.
    let with_op = |op: &str, body: Map<String, Value>| {
        let mut v = Value::Object(body);
        v["op"] = Value::String(op.into());
        v
    };
    let threads = |n: Option<u64>| n.unwrap_or(1).clamp(1, MAX_THREADS as u64) as usize;
    match request {
        Request::OptionDefaults => {
            // The engine defaults. The UI reads them instead of copying values; absent keys are defaults too.
            let mut options =
                serde_json::to_value(arqlet_model::LayoutOptions::default()).unwrap_or_default();
            options["layeredConstruction"] = serde_json::json!(true);
            options["hierarchicalOrdering"] = serde_json::json!(true);
            Response::Encoded(serde_json::json!({"ok": true, "options": options}).to_string())
        }
        Request::Import {
            graph,
            limit,
            options,
        } => snapshot((|| {
            let options = match options {
                Some(patch) => options_from_patch(&patch)?,
                None => LayoutOptions::default(),
            };
            import_graph_with(*graph, limit as usize, &options)
        })()),
        Request::Direction {
            on,
            node_id,
            direction,
        } => snapshot(set_direction(
            &on.snapshot,
            on.expected_revision,
            on.expected_layout_version,
            node_id.as_deref(),
            direction,
        )),
        Request::Strategy {
            on,
            node_id,
            strategy,
        } => snapshot(set_strategy(
            &on.snapshot,
            on.expected_revision,
            on.expected_layout_version,
            node_id.as_deref(),
            strategy,
        )),
        Request::Routing {
            on,
            node_id,
            routing,
        } => snapshot(set_routing(
            &on.snapshot,
            on.expected_revision,
            on.expected_layout_version,
            node_id.as_deref(),
            routing,
        )),
        Request::Move { on, step } => snapshot(move_node(
            &on.snapshot,
            on.expected_revision,
            on.expected_layout_version,
            &step,
        )),
        Request::Add {
            snapshot: s,
            expected_revision,
            source_id,
            label,
        } => snapshot(add_node(
            &s,
            expected_revision,
            &source_id,
            label.as_deref().unwrap_or("New node"),
        )),
        Request::Toggle { on, node_id } => snapshot(toggle_container(
            &on.snapshot,
            on.expected_revision,
            on.expected_layout_version,
            &node_id,
        )),
        Request::Patch { on, patch } => snapshot(crate::patch::patch_snapshot(
            &on.snapshot,
            on.expected_revision,
            on.expected_layout_version,
            &patch,
        )),
        Request::Filter { on, hide } => snapshot(crate::filter::filter_view(
            &on.snapshot,
            on.expected_revision,
            on.expected_layout_version,
            hide,
        )),
        Request::Reveal {
            on,
            more: Some(more),
            ..
        } => snapshot(reveal_more(
            &on.snapshot,
            on.expected_revision,
            on.expected_layout_version,
            &more.container,
            more.count,
        )),
        Request::Reveal {
            on,
            node_ids,
            partial,
            more: None,
        } => snapshot(reveal_nodes(
            &on.snapshot,
            on.expected_revision,
            on.expected_layout_version,
            &node_ids,
            partial.unwrap_or(true),
        )),
        Request::Configure { on, options } => snapshot(configure_snapshot(
            &on.snapshot,
            on.expected_revision,
            on.expected_layout_version,
            &options,
        )),
        Request::Port {
            on,
            edge_id,
            endpoint,
            side,
            offset,
        } => snapshot(port_edit::set_port(
            &on.snapshot,
            on.expected_revision,
            on.expected_layout_version,
            &edge_id,
            &endpoint,
            side.zip(offset),
        )),
        Request::Pin {
            on,
            node_id,
            position,
            unpin,
        } => snapshot(crate::pin_node(
            &on.snapshot,
            on.expected_revision,
            on.expected_layout_version,
            &node_id,
            position,
            unpin,
        )),
        Request::Settle { on } => snapshot(settle::settle_snapshot(
            &on.snapshot,
            on.expected_revision,
            on.expected_layout_version,
        )),
        Request::Restore { on, intent } => snapshot(history::restore_intent(
            &on.snapshot,
            on.expected_revision,
            on.expected_layout_version,
            &intent,
        )),
        Request::Shape { on, node_id, shape } => snapshot(set_shape(
            &on.snapshot,
            on.expected_revision,
            on.expected_layout_version,
            &node_id,
            shape,
        )),
        Request::Measure { on, what } => snapshot(match what {
            Measured::Measurements(m) => measure_contours(
                &on.snapshot,
                on.expected_revision,
                on.expected_layout_version,
                m,
            ),
            Measured::Sizes(sizes) => measure_snapshot(
                &on.snapshot,
                on.expected_revision,
                on.expected_layout_version,
                sizes,
            ),
        }),
        // Without a publication callback the progressive operation is a plain refinement with the same result.
        Request::Refine { on, budget } | Request::RefineProgressive { on, budget } => {
            snapshot(refine_request(
                &on.snapshot,
                on.expected_revision,
                on.expected_layout_version,
                threads(budget.threads),
                budget.budget_ms.as_ref(),
                budget.work_budget.as_ref(),
            ))
        }
        Request::Intent { snapshot: s } => Response::Intent(Box::new(history::intent_of(&s))),
        Request::Scene { body } => {
            let v = with_op("scene", body);
            typed_answer(
                serde_json::from_value::<Snapshot>(v["snapshot"].clone())
                    .map_err(|e| e.to_string())
                    .and_then(|s| crate::filter::scene_of(&s)),
                |s| Response::Scene(Box::new(s)),
            )
        }
        Request::Trace { body } => {
            typed_answer(trace::trace_answer(&with_op("trace", body)), |a| {
                Response::Trace(Box::new(a))
            })
        }
        Request::Neighbourhood { body } => typed_answer(
            trace::neighbourhood_answer(&with_op("neighbourhood", body)),
            |a| Response::Neighbourhood(Box::new(a)),
        ),
        Request::Details { body } => typed_answer(
            crate::details::details_answer(&with_op("details", body)),
            |a| Response::Details(Box::new(a)),
        ),
        Request::Query { body } => {
            Response::Encoded(crate::query::query_response(&with_op("query", body)).to_string())
        }
        Request::Peek { body } => {
            Response::Encoded(crate::peek::peek_response(&with_op("peek", body)).to_string())
        }
        Request::Digest { body } => {
            Response::Encoded(crate::find::digest_response(&with_op("digest", body)).to_string())
        }
        Request::Find { body } => {
            typed_answer(crate::find::find_answer(&with_op("find", body)), |a| {
                Response::Find(Box::new(a))
            })
        }
        Request::Overview { body } => {
            let v = with_op("overview", body);
            let n = threads(v["threads"].as_u64());
            typed_answer(overview::overview_of(&v, n), |o| {
                Response::Overview(Box::new(o))
            })
        }
        Request::OverviewTrace { body } => {
            let v = with_op("overviewTrace", body);
            Response::Encoded(
                crate::trace::graph_read_response(overview::graph_of(&v), &v).to_string(),
            )
        }
        Request::OverviewNeighbourhood { body } => {
            let v = with_op("overviewNeighbourhood", body);
            Response::Encoded(
                crate::trace::graph_read_response(overview::graph_of(&v), &v).to_string(),
            )
        }
        Request::ElkLayout { body } => Response::Encoded(elk_response(&with_op("elkLayout", body))),
        Request::Communities { body } => {
            let v = with_op("communities", body);
            let answer = typed::<Graph>(&v["graph"]).and_then(|graph| {
                let options: arqlet_model::LayoutOptions = if v["options"].is_null() {
                    Default::default()
                } else {
                    typed(&v["options"])?
                };
                crate::community_groups(&graph, &options)
            });
            match answer {
                Ok(groups) => {
                    Response::Encoded(serde_json::json!({"ok": true, "groups": groups}).to_string())
                }
                Err(e) => Response::Error(e),
            }
        }
        Request::Metrics { body } => {
            let v = with_op("metrics", body);
            typed_answer(
                serde_json::from_value::<arqlet_scene::metrics::Drawing>(v["drawing"].clone())
                    .map_err(|e| e.to_string())
                    .and_then(|drawing| arqlet_scene::metrics::measure(&drawing)),
                |m| Response::Metrics(Box::new(m)),
            )
        }
        Request::RouteConflicts { body } => {
            Response::Encoded(route_conflicts_response(&with_op("routeConflicts", body)))
        }
        Request::ComponentRefine { body } => {
            Response::Encoded(component_response(&with_op("componentRefine", body)))
        }
        Request::ContainerRefine { body } => {
            Response::Encoded(container_response(&with_op("containerRefine", body)))
        }
        Request::StructuredRefine { body } => {
            Response::Encoded(structured_response(&with_op("structuredRefine", body)))
        }
        Request::ReferenceCreate { body } => reference("referenceCreate", body),
        Request::ReferenceRefresh { body } => reference("referenceRefresh", body),
        Request::ReferenceReroute { body } => reference("referenceReroute", body),
        Request::ReferenceValidate { body } => reference("referenceValidate", body),
        Request::ReferenceCanonical { body } => reference("referenceCanonical", body),
        Request::ReferenceSvg { body } => reference("referenceSvg", body),
        Request::ReferenceFromSvg { body } => reference("referenceFromSvg", body),
        Request::ReferenceReproduce { body } => reference("referenceReproduce", body),
        Request::ReferenceRepair { body } => reference("referenceRepair", body),
    }
}

fn reference(op: &str, body: Map<String, Value>) -> Response {
    let mut v = Value::Object(body);
    v["op"] = Value::String(op.into());
    Response::Encoded(crate::reference_response(op, &v))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{dispatch, dispatch_legacy};
    use serde_json::json;

    fn fixture() -> Value {
        serde_json::from_str(include_str!("../../../examples/fixtures/small.json")).unwrap()
    }

    /// Requests of every kind, valid and malformed, built on a snapshot of the small fixture.
    fn corpus() -> Vec<String> {
        let graph = fixture();
        let imported: Value = serde_json::from_str(&dispatch_legacy(
            &json!({"op": "import", "graph": graph, "limit": 100}).to_string(),
        ))
        .unwrap();
        let s = imported["snapshot"].clone();
        let (rev, lay) = (s["revision"].clone(), s["layoutVersion"].clone());
        let edit = |op: &str, extra: Value| {
            let mut v = json!({"op": op, "snapshot": s, "expectedRevision": rev, "expectedLayoutVersion": lay});
            for (k, x) in extra.as_object().unwrap() {
                v[k] = x.clone();
            }
            v
        };
        let intent: Value = serde_json::from_str(&dispatch_legacy(
            &json!({"op": "intent", "snapshot": s}).to_string(),
        ))
        .unwrap();
        let mut requests = vec![
            json!({"op": "optionDefaults"}),
            json!({"op": "import", "graph": graph, "limit": 100}),
            json!({"op": "import", "graph": graph, "limit": 100, "options": {"communities": 2, "levelOfDetail": {"depth": 0}}}),
            json!({"op": "import", "graph": graph}),
            json!({"op": "import", "graph": graph, "limit": 0}),
            json!({"op": "import", "graph": {"nodes": 3}, "limit": 10}),
            json!({"op": "import", "graph": graph, "limit": 10, "options": {"nodeSpacing": "wide"}}),
            json!({}),
            json!([]),
            json!({"op": "bogus", "snapshot": s, "expectedRevision": rev}),
            json!({"op": "toggle", "snapshot": s}),
            json!({"op": "toggle", "snapshot": 3, "expectedRevision": rev}),
            edit("toggle", json!({"nodeId": "root"})),
            edit("toggle", json!({})),
            edit("toggle", json!({"nodeId": "nope"})),
            edit("toggle", json!({"nodeId": "root", "expectedRevision": 99})),
            json!({"op": "toggle", "snapshot": s, "expectedRevision": rev, "nodeId": "root"}),
            edit("reveal", json!({"nodeIds": ["a"]})),
            edit("reveal", json!({"nodeIds": ["a"], "partial": false})),
            edit("reveal", json!({"nodeIds": "a"})),
            edit("direction", json!({"nodeId": null, "direction": "tb"})),
            edit("direction", json!({"nodeId": "root", "direction": null})),
            edit("direction", json!({"direction": "sideways"})),
            edit("strategy", json!({"nodeId": "root", "strategy": "tree"})),
            edit("strategy", json!({"nodeId": "root", "strategy": "nonsense"})),
            edit("routing", json!({"nodeId": null, "routing": "straight"})),
            edit("routing", json!({"routing": "zigzag"})),
            json!({"op": "add", "snapshot": s, "expectedRevision": rev, "sourceId": "a", "label": "x"}),
            json!({"op": "add", "snapshot": s, "expectedRevision": rev, "sourceId": "a"}),
            json!({"op": "add", "snapshot": s, "expectedRevision": rev}),
            edit("configure", json!({"options": {"nodeSpacing": 30}})),
            edit("configure", json!({"options": {"nodeSpacing": "wide"}})),
            edit("port", json!({"edgeId": "ab", "endpoint": "source", "side": "top", "offset": 0.5})),
            edit("port", json!({"edgeId": "ab", "endpoint": "source", "side": null})),
            edit("port", json!({"edgeId": "ab", "endpoint": "source", "side": "top"})),
            edit("port", json!({"edgeId": "ab"})),
            edit("settle", json!({})),
            edit("pin", json!({"nodeId": "a"})),
            edit("pin", json!({"nodeId": "a", "position": {"x": 10, "y": 20}})),
            edit("pin", json!({"nodeId": "a", "unpin": true})),
            edit("pin", json!({"nodeId": "nope"})),
            edit("pin", json!({"nodeId": "a", "position": 3})),
            edit("restore", json!({"intent": intent["intent"]})),
            edit("restore", json!({"intent": 5})),
            edit("shape", json!({"nodeId": "a", "shape": "circle"})),
            edit("shape", json!({"nodeId": "a", "shape": "blob"})),
            edit("measure", json!({"sizes": {"a": {"width": 120, "height": 40}}})),
            edit("measure", json!({"measurements": 7})),
            edit("measure", json!({})),
            edit("refine", json!({"workBudget": 1})),
            edit("refineProgressive", json!({"workBudget": 1})),
            edit("refine", json!({"workBudget": -1})),
            edit("move", json!({"gestureId": "g", "seq": 1, "phase": "begin", "node": "a", "position": {"x": 10, "y": 10}})),
            edit("move", json!({"seq": 1})),
            json!({"op": "intent", "snapshot": s}),
            json!({"op": "intent", "snapshot": null}),
            json!({"op": "scene", "snapshot": s}),
            json!({"op": "scene"}),
            json!({"op": "trace", "snapshot": s, "from": "a", "to": "b"}),
            json!({"op": "trace", "snapshot": s, "from": "a"}),
            json!({"op": "neighbourhood", "snapshot": s, "nodeId": "a", "hops": 2}),
            json!({"op": "neighbourhood", "snapshot": s, "nodeId": "a", "direction": "sideways"}),
            json!({"op": "overview", "graph": graph}),
            json!({"op": "overview", "graph": graph, "threads": 2, "options": {"communities": 0}}),
            json!({"op": "elkLayout", "graph": {"id": "root", "children": [{"id": "a", "width": 40, "height": 30}, {"id": "b", "width": 40, "height": 30}], "edges": [{"id": "e", "sources": ["a"], "targets": ["b"]}]}}),
            json!({"op": "elkLayout"}),
            json!({"op": "metrics", "snapshot": s}),
            json!({"op": "routeConflicts"}),
            json!({"op": "componentRefine"}),
            json!({"op": "containerRefine"}),
            json!({"op": "structuredRefine", "snapshot": s, "expectedRevision": rev, "expectedLayoutVersion": lay}),
            json!({"op": "referenceValidate"}),
            json!({"op": "referenceCanonical", "reference": 1}),
        ]
        .into_iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>();
        requests.push("{".into());
        requests.push("not json".into());
        requests
    }

    /// ADR 0019, step 1: the typed protocol answers every request exactly as the dispatcher before it, byte for byte,
    /// errors included.
    #[test]
    fn the_typed_protocol_answers_like_the_dispatcher_before_it() {
        let (mut ok, mut refused) = (0, 0);
        for request in corpus() {
            let (now, before) = (dispatch(&request), dispatch_legacy(&request));
            assert_eq!(
                now,
                before,
                "request: {}",
                &request[..request.len().min(200)]
            );
            if now.contains(r#""ok":true"#) {
                ok += 1;
            } else {
                refused += 1;
            }
        }
        // Both kinds are covered: answers with results and refusals.
        eprintln!("{ok} answered, {refused} refused");
        assert!(
            ok >= 25 && refused >= 25,
            "{ok} answered, {refused} refused"
        );
    }

    /// A request read into its type and written back is the same request: it gets the same answer.
    #[test]
    fn a_typed_request_written_back_gets_the_same_answer() {
        let mut typed = 0;
        for request in corpus() {
            let Ok(parsed) = Request::from_json(&request) else {
                continue;
            };
            typed += 1;
            let written = serde_json::to_string(&parsed).unwrap();
            assert_eq!(
                dispatch(&written),
                dispatch(&request),
                "request: {}",
                &request[..request.len().min(200)]
            );
            // Reading the written request again gives the same request.
            let again = Request::from_json(&written).unwrap();
            assert_eq!(again.to_value(), parsed.to_value());
        }
        assert!(typed > 40, "{typed} requests parsed");
    }

    /// A host in Rust builds requests as values, without JSON.
    #[test]
    fn a_request_built_in_rust_runs() {
        let graph: Graph = serde_json::from_value(fixture()).unwrap();
        let Response::Snapshot(snapshot) = handle(Request::Import {
            graph: Box::new(graph),
            limit: 100,
            options: None,
        }) else {
            panic!("import failed");
        };
        let edit = Edit {
            expected_revision: snapshot.revision,
            expected_layout_version: snapshot.layout_version,
            snapshot,
        };
        match handle(Request::Toggle {
            on: edit,
            node_id: "root".into(),
        }) {
            Response::Snapshot(s) => assert_eq!(s.collapsed_ids, ["root"]),
            other => panic!("{other:?}"),
        }
    }
}
