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
    /// Show the given nodes by opening the containers that hold them (`partial`, default true: only as far as needed).
    Reveal {
        #[serde(flatten)]
        on: Edit,
        node_ids: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        partial: Option<bool>,
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
    /// The cluster overview of a whole graph.
    Overview {
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
    /// The JSON answer of an operation whose answer is not typed yet.
    Encoded(String),
}

impl Response {
    /// The JSON text of the answer, as `dispatch` returns it.
    pub fn to_json(&self) -> String {
        match self {
            Response::Snapshot(s) => serde_json::json!({"ok": true, "snapshot": s}).to_string(),
            Response::Error(e) => serde_json::json!({"ok": false, "error": e}).to_string(),
            Response::Encoded(text) => text.clone(),
        }
    }
}

fn body_of(v: &Value) -> Map<String, Value> {
    let mut body = v.as_object().cloned().unwrap_or_default();
    body.remove("op");
    body
}

fn typed<T: serde::de::DeserializeOwned>(v: &Value) -> Result<T, String> {
    serde_json::from_value(v.clone()).map_err(|e| e.to_string())
}

impl Request {
    /// Reads a request from its JSON text, with the same errors the engine has always given.
    pub fn from_json(input: &str) -> Result<Request, String> {
        let v: Value = serde_json::from_str(input).map_err(|e| e.to_string())?;
        Request::from_value(&v)
    }

    /// Reads a request from its JSON value. The checks run in the engine's historical order, so that a malformed request
    /// gets the same error as before the protocol was typed.
    pub fn from_value(v: &Value) -> Result<Request, String> {
        let body = || body_of(v);
        match v["op"].as_str() {
            Some("optionDefaults") => return Ok(Request::OptionDefaults),
            Some("scene") => return Ok(Request::Scene { body: body() }),
            Some("trace") => return Ok(Request::Trace { body: body() }),
            Some("componentRefine") => return Ok(Request::ComponentRefine { body: body() }),
            Some("elkLayout") => return Ok(Request::ElkLayout { body: body() }),
            Some("overview") => return Ok(Request::Overview { body: body() }),
            Some("containerRefine") => return Ok(Request::ContainerRefine { body: body() }),
            Some("structuredRefine") => return Ok(Request::StructuredRefine { body: body() }),
            Some("metrics") => return Ok(Request::Metrics { body: body() }),
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
                    | "configure"
                    | "port"
                    | "settle"
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
            "reveal" => Request::Reveal {
                on,
                node_ids: serde_json::from_value(v["nodeIds"].clone())
                    .map_err(|_| "Missing nodeIds")?,
                partial: v["partial"].as_bool(),
            },
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
pub fn handle(request: Request) -> Response {
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
        Request::Reveal {
            on,
            node_ids,
            partial,
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
        Request::Intent { snapshot: s } => Response::Encoded(
            serde_json::json!({"ok": true, "intent": history::intent_of(&s)}).to_string(),
        ),
        Request::Scene { body } => Response::Encoded(scene_response(&with_op("scene", body))),
        Request::Trace { body } => {
            Response::Encoded(trace::trace_response(&with_op("trace", body)).to_string())
        }
        Request::Overview { body } => {
            let v = with_op("overview", body);
            let n = threads(v["threads"].as_u64());
            Response::Encoded(overview::overview_response(&v, n).to_string())
        }
        Request::ElkLayout { body } => Response::Encoded(elk_response(&with_op("elkLayout", body))),
        Request::Metrics { body } => Response::Encoded(metrics_response(&with_op("metrics", body))),
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
            json!({"op": "import", "graph": graph, "limit": 100, "options": {"communities": 2, "initialCollapse": {"depth": 0}}}),
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
