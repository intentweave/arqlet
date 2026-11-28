//! Provisional (ADR 0024, step C3): an estimate of the heap a request will need at its peak, before it runs, for a host that
//! admits work by memory (a worker pool with a memory budget). `{"op": "estimate", "request": {…}}` takes the request it
//! estimates as it would be sent (to `dispatch`, or to a resident session without its snapshot) and answers
//! `{ok, estimate: {op, target, bytes, parts, view, source, jobs, model}}`.
//!
//! How: the request is read into its types (the graph, the snapshot) as the operation reads it, and the view it will lay
//! out is found the way the operation finds it, without laying it out: an import runs its selection and projection
//! (`prepare_import`), an edit that opens containers (`toggle`, `reveal`) or changes what the view shows (`filter`,
//! `configure`, `restore`) projects the view after it, a `patch` adds what it brings, every other edit keeps the view.
//! From these sizes a model with coefficients calibrated per target (`benchmarks/results/memory-estimate-20261127.md`)
//! adds up the parts:
//!
//! - `request`: the request text, which the engine holds while it runs (copied into a WASM module's memory);
//! - `input`: reading it (the typed graph or snapshot, the JSON value the protocol reads first) and writing the answer;
//! - `structure`: the source graph's index and copies an operation makes of it;
//! - `layout`: laying out the view from scratch, once per concurrent refinement job; an edit that continues the earlier
//!   layout needs less, so the estimate is an upper bound for it.
//!
//! The estimate is meant as an upper bound within the factor recorded with the calibration, never far below the counted
//! peak. Each build estimates for its own target: type sizes differ between 64-bit builds and wasm32 (`target` in the
//! answer), so a host that admits WASM workers asks a WASM engine. Deterministic, pure and cheap next to the request: no
//! layout runs, and its own time is recorded with the calibration.
use crate::protocol::RevealMore;
use arqlet_model::{Graph, GraphPatch, Hidden, Intent, LayoutOptions, Snapshot};
use serde::Deserialize;
use serde_json::{json, Value};

/// The version of the model and its coefficients; a recalibration changes it.
pub const MODEL_VERSION: u32 = 1;

/// The margin on everything but the request text: the coefficients bound the calibration rows exactly, a request of
/// another shape may need more (`benchmarks/results/memory-estimate-20261127.md`, the graphs left out of a fit).
pub const MARGIN: f64 = 1.15;

/// The request an estimate reads: only the fields that change what the operation needs; the rest is skipped unread.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Estimated {
    op: Option<String>,
    graph: Option<Graph>,
    limit: Option<u64>,
    options: Option<Value>,
    snapshot: Option<Snapshot>,
    source: Option<Graph>,
    node_id: Option<String>,
    node_ids: Option<Vec<String>>,
    more: Option<RevealMore>,
    patch: Option<GraphPatch>,
    hide: Option<Hidden>,
    intent: Option<Box<Intent>>,
    threads: Option<u64>,
    work_budget: Option<Value>,
    scene: Option<Value>,
}

#[derive(Deserialize)]
struct EstimateText {
    op: String,
    request: Estimated,
}

/// The sizes the model reads.
#[derive(Debug, Default, Clone, Copy)]
struct Sizes {
    /// The request text in bytes.
    text: usize,
    /// The heap of the source graph (`memory::graph`), and its counts.
    source_heap: usize,
    source_nodes: usize,
    source_edges: usize,
    /// The view the operation lays out: nodes, edges, containers shown open.
    view_nodes: usize,
    view_edges: usize,
    open: usize,
    /// Layouts held at the same time (a refinement's concurrent jobs).
    jobs: usize,
}

/// What a class of requests costs: per byte of its text, per byte of its source graph, and per byte of the layout model.
#[derive(Clone, Copy)]
struct Class {
    /// Bytes per byte of request text: the text, reading it, the answer.
    text: f64,
    /// Bytes per byte of the source graph's heap: its index, selections, copies.
    source: f64,
    /// The share of a from-scratch layout of the view (`layout_bytes`) the class needs per job.
    layout: f64,
    /// Fixed bytes.
    base: f64,
}

/// The coefficients of one target: a class per kind of operation and place (`kind/place`).
struct Coefficients {
    classes: [(&'static str, Class); 11],
    /// Laying out a view from scratch: fixed, per node, per edge, per open container, per edge squared (the tables of a
    /// channel grow with the square of its segments), per edge squared over the nodes (edges gathered between few boxes),
    /// per node times open container and per open container squared (deep and wide hierarchies).
    layout: [f64; 8],
    /// The overview per node and per edge of its graph, on top of its class.
    overview_nodes: f64,
    overview_edges: f64,
}

#[cfg(target_pointer_width = "64")]
const TARGET: &str = "native64";
#[cfg(not(target_pointer_width = "64"))]
const TARGET: &str = "wasm32";

const fn class(text: f64, source: f64, layout: f64, base: f64) -> Class {
    Class {
        text,
        source,
        layout,
        base,
    }
}

// COEFFICIENTS-NATIVE-BEGIN
// Fitted by `benchmarks/memory-estimate/fit.py report` on the rows of `benchmarks/results/memory-estimate-20261127.md`.
#[cfg(target_pointer_width = "64")]
const COEFFICIENTS: Coefficients = Coefficients {
    classes: [
        ("import/dispatch", class(1.0315, 7.8400, 0.9730, 124162.3)),
        ("import/session", class(1.2776, 2.0708, 0.9907, 194393.6)),
        ("open/dispatch", class(2.7335, 1.2601, 0.6582, 0.0)),
        ("open/session", class(1.0000, 0.3113, 1.1325, 8191.3)),
        ("keep/dispatch", class(1.0000, 1.6590, 0.9007, 249141.5)),
        ("keep/session", class(1.0000, 2.9059, 1.0659, 36906.7)),
        ("patch/dispatch", class(1.1870, 1.9329, 0.6922, 1114332.9)),
        ("patch/session", class(1.0000, 3.9882, 1.1299, 0.0)),
        ("refine/dispatch", class(5.5130, 0.0390, 0.9007, 2386260.3)),
        ("refine/session", class(1.0000, 2.5180, 1.0659, 4190911.6)),
        ("overview/dispatch", class(1.0000, 0.1327, 0.0000, 507931.7)),
    ],
    layout: [0.0, 13576.0, 3581.3, 53.1, 9.9, 5409.6, 3.5, 435.7],
    overview_nodes: 1398.2,
    overview_edges: 1417.9,
};
// COEFFICIENTS-NATIVE-END

// COEFFICIENTS-WASM-BEGIN
// Fitted by `benchmarks/memory-estimate/fit.py report` on the rows of `benchmarks/results/memory-estimate-20261127.md`.
#[cfg(not(target_pointer_width = "64"))]
const COEFFICIENTS: Coefficients = Coefficients {
    classes: [
        ("import/dispatch", class(2.1703, 11.4731, 0.8964, 1508.0)),
        ("import/session", class(1.0226, 2.7846, 0.9127, 576235.4)),
        ("open/dispatch", class(18.0347, 0.0000, 0.1354, 0.0)),
        ("open/session", class(1.0000, 0.0000, 0.8392, 4249316.1)),
        ("keep/dispatch", class(17.7421, 0.0000, 0.1400, 0.0)),
        ("keep/session", class(1.0000, 2.3270, 0.7894, 419066.9)),
        ("patch/dispatch", class(16.7993, 0.0270, 0.1348, 45637.2)),
        ("patch/session", class(1.0000, 3.7750, 0.7867, 765573.4)),
        ("refine/dispatch", class(10.3952, 3.3412, 0.1400, 556059.9)),
        ("refine/session", class(1.0000, 0.0000, 0.7894, 8190215.0)),
        ("overview/dispatch", class(8.3941, 1.6319, 0.0000, 0.0)),
    ],
    layout: [0.0, 13576.0, 3581.3, 53.1, 9.9, 5409.6, 3.5, 435.7],
    overview_nodes: 1733.0,
    overview_edges: 15.2,
};
// COEFFICIENTS-WASM-END

/// The heap of laying out a view of this size from scratch.
fn layout_bytes(s: &Sizes) -> f64 {
    let [fixed, node, edge, open, square, density, nesting, open2] = COEFFICIENTS.layout;
    let (v, e, o) = (s.view_nodes as f64, s.view_edges as f64, s.open as f64);
    fixed
        + node * v
        + edge * e
        + open * o
        + square * e * e
        + density * e * e / v.max(1.0)
        + nesting * v * o
        + open2 * o * o
}

/// The coefficients of a kind of operation at a place.
fn coefficients(kind: &str, place: Place) -> Class {
    let place = match place {
        _ if kind == "overview" => "dispatch",
        Place::Dispatch => "dispatch",
        Place::Session => "session",
    };
    let key = format!("{kind}/{place}");
    COEFFICIENTS
        .classes
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, c)| *c)
        .expect("every kind has its coefficients")
}

/// Where the request runs: through `dispatch` (its snapshot in the text) or in a resident session.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Place {
    Dispatch,
    Session,
}

/// Whether a request text is an `estimate` (its own `op`, read without the rest).
pub fn is_estimate(input: &str) -> bool {
    #[derive(Deserialize)]
    struct Head {
        op: Option<String>,
    }
    input.contains("\"estimate\"")
        && serde_json::from_str::<Head>(input).is_ok_and(|h| h.op.as_deref() == Some("estimate"))
}

/// `estimate` through `dispatch`: the request in the text, read in one pass (no JSON value of a large graph).
pub fn estimate_response(input: &str) -> String {
    match serde_json::from_str::<EstimateText>(input) {
        Ok(outer) if outer.op == "estimate" => {
            let text = request_length(input);
            answer(estimate(outer.request, None, text, Place::Dispatch))
        }
        Ok(_) => answer(Err("Unsupported operation".into())),
        Err(e) => answer(Err(format!(
            "estimate needs {{op: \"estimate\", request}}: {e}"
        ))),
    }
}

/// `estimate` in a resident session: the request as the session takes it (without a snapshot), on the session's snapshot.
pub(crate) fn estimate_in_session(input: &str, current: Option<&Snapshot>) -> Value {
    match serde_json::from_str::<EstimateText>(input) {
        Ok(outer) if outer.op == "estimate" => {
            let text = request_length(input);
            estimate(outer.request, current, text, Place::Session)
                .unwrap_or_else(|e| json!({"ok": false, "error": e}))
        }
        Ok(_) => json!({"ok": false, "error": "Unsupported operation"}),
        Err(e) => {
            json!({"ok": false, "error": format!("estimate needs {{op: \"estimate\", request}}: {e}")})
        }
    }
}

fn answer(result: Result<Value, String>) -> String {
    match result {
        Ok(v) => v.to_string(),
        Err(e) => json!({"ok": false, "error": e}).to_string(),
    }
}

/// The length of the estimated request's text: the estimate's whole text (the wrapper's few bytes err on the safe side).
fn request_length(input: &str) -> usize {
    input.len()
}

/// The operations the estimate knows, by the kind of their cost: an import, an overview, an edit that opens containers or
/// changes what the view shows (`open`), one that keeps the view (`keep`), one that changes the source (`patch`), and a
/// refinement.
fn kind_of(op: &str) -> Option<&'static str> {
    Some(match op {
        "import" => "import",
        "overview" => "overview",
        "toggle" | "reveal" | "filter" | "configure" | "restore" => "open",
        "direction" | "strategy" | "routing" | "move" | "port" | "settle" | "pin" | "shape"
        | "measure" => "keep",
        "patch" | "add" => "patch",
        "refine" | "refineProgressive" => "refine",
        _ => return None,
    })
}

fn estimate(
    request: Estimated,
    current: Option<&Snapshot>,
    text: usize,
    place: Place,
) -> Result<Value, String> {
    let op = request
        .op
        .clone()
        .ok_or("estimate: the request has no op")?;
    let kind = kind_of(&op).ok_or_else(|| {
        format!("estimate knows import, overview and the edits of a view, not {op}")
    })?;
    let mut sizes = Sizes {
        text,
        jobs: 1,
        ..Sizes::default()
    };
    match kind {
        "import" => import_sizes(request, &mut sizes)?,
        "overview" => {
            let graph = request
                .graph
                .as_ref()
                .ok_or("estimate: an overview without its graph (a session's later overview) is not estimated")?;
            source_sizes(graph, &mut sizes);
        }
        _ => edit_sizes(&op, request, current, place, &mut sizes)?,
    }
    let c = &COEFFICIENTS;
    let coefficients = coefficients(kind, place);
    let class = kind;
    let request_part = sizes.text as f64;
    let input =
        MARGIN * ((coefficients.text - 1.0).max(0.0) * sizes.text as f64 + coefficients.base);
    let structure = MARGIN
        * (coefficients.source * sizes.source_heap as f64
            + if class == "overview" {
                c.overview_nodes * sizes.source_nodes as f64
                    + c.overview_edges * sizes.source_edges as f64
            } else {
                0.0
            });
    let layout = if class == "overview" {
        0.0
    } else {
        MARGIN * coefficients.layout * sizes.jobs as f64 * layout_bytes(&sizes)
    };
    let round = |b: f64| b.max(0.0).ceil() as u64;
    let parts = json!({
        "request": round(request_part),
        "input": round(input),
        "structure": round(structure),
        "layout": round(layout),
    });
    let bytes = round(request_part) + round(input) + round(structure) + round(layout);
    let mut estimate = json!({
        "op": op,
        "kind": kind,
        "target": TARGET,
        "bytes": bytes,
        "parts": parts,
        "source": {"nodes": sizes.source_nodes, "edges": sizes.source_edges, "heap": sizes.source_heap},
        "jobs": sizes.jobs,
        "model": MODEL_VERSION,
    });
    if class != "overview" {
        estimate["view"] =
            json!({"nodes": sizes.view_nodes, "edges": sizes.view_edges, "open": sizes.open});
    }
    Ok(json!({"ok": true, "estimate": estimate}))
}

fn source_sizes(graph: &Graph, sizes: &mut Sizes) {
    sizes.source_heap = crate::memory::graph(graph);
    sizes.source_nodes = graph.nodes.len();
    sizes.source_edges = graph.edges.len();
}

fn view_sizes(graph: &Graph, sizes: &mut Sizes) {
    sizes.view_nodes = graph.nodes.len();
    sizes.view_edges = graph.edges.len();
    let open: std::collections::BTreeSet<&str> = graph
        .nodes
        .iter()
        .filter_map(|n| n.parent_id.as_deref())
        .collect();
    sizes.open = open.len();
}

/// An import: the selection and projection it lays out (`prepare_import`), without the layout.
fn import_sizes(request: Estimated, sizes: &mut Sizes) -> Result<(), String> {
    let mut graph = request
        .graph
        .ok_or("estimate: an import without its graph")?;
    graph.shrink_to_fit();
    source_sizes(&graph, sizes);
    let limit = request.limit.ok_or("Missing limit")?;
    let limit = usize::try_from(limit).map_err(|e| e.to_string())?;
    let options = match &request.options {
        Some(patch) => crate::options_from_patch(patch)?,
        None => LayoutOptions::default(),
    };
    let prepared = crate::prepare_import(graph, limit, &options)?;
    view_sizes(prepared.laid_out(), sizes);
    Ok(())
}

/// An edit of a view: the view it lays out, found as the operation finds it (an upper bound where that is cheaper).
fn edit_sizes(
    op: &str,
    mut request: Estimated,
    current: Option<&Snapshot>,
    place: Place,
    sizes: &mut Sizes,
) -> Result<(), String> {
    let owned;
    let snapshot: &Snapshot = match (place, current) {
        (Place::Session, Some(s)) => s,
        (Place::Session, None) => return Err("The session has no view yet".into()),
        (Place::Dispatch, _) => {
            let mut s = request
                .snapshot
                .take()
                .ok_or("estimate: an edit without its snapshot")?;
            if s.source_graph.is_none() {
                if let Some(source) = request.source.take() {
                    s.source_graph = Some(std::sync::Arc::new(source));
                }
            }
            owned = s;
            &owned
        }
    };
    let source = snapshot.source_graph.as_deref().unwrap_or(&snapshot.graph);
    source_sizes(source, sizes);
    view_sizes(&snapshot.graph, sizes);
    // A view this edit may grow to, projected as the operation projects it.
    let project = |collapsed: Vec<String>, options: &LayoutOptions| -> Result<Graph, String> {
        let (structure, kept, validated) = crate::structure_of(snapshot, options)?;
        let index = kept
            .as_ref()
            .filter(|_| validated)
            .map(|kept| kept.index.clone());
        Ok(
            crate::project_view_on(structure, index.as_deref(), &collapsed, options, true)?
                .projected,
        )
    };
    let collapsed = || snapshot.collapsed_ids.clone();
    let mut after: Option<Graph> = None;
    match op {
        "toggle" => {
            let id = request.node_id.as_deref().ok_or("Missing nodeId")?;
            let mut options = snapshot.options.clone();
            let opened =
                if let Some(container) = id.strip_prefix(arqlet_layout::communities::REST_PREFIX) {
                    options.partial.remove(container);
                    true
                } else {
                    snapshot.collapsed_ids.iter().any(|c| c == id)
                };
            // Closing a container shrinks the view: the view before bounds it.
            if opened {
                let ids: Vec<String> = collapsed().into_iter().filter(|c| c != id).collect();
                after = Some(project(ids, &options)?);
            }
        }
        "reveal" => {
            let mut options = snapshot.options.clone();
            let (structure, _, _) = crate::structure_of(snapshot, &options)?;
            let parent: std::collections::BTreeMap<&str, Option<&str>> = structure
                .nodes
                .iter()
                .map(|n| (n.id.as_str(), n.parent_id.as_deref()))
                .collect();
            // Every container that holds a named node opened, and the paged one shown whole: at least what `reveal` opens.
            let mut open: std::collections::BTreeSet<String> = Default::default();
            for id in request.node_ids.iter().flatten() {
                let mut at = parent.get(id.as_str()).copied().flatten();
                while let Some(p) = at {
                    open.insert(p.to_string());
                    at = parent.get(p).copied().flatten();
                }
            }
            if let Some(more) = &request.more {
                options.partial.remove(&more.container);
                open.insert(more.container.clone());
                open.insert(format!(
                    "{}{}",
                    arqlet_layout::communities::REST_PREFIX,
                    more.container
                ));
            }
            drop(structure);
            let ids: Vec<String> = collapsed()
                .into_iter()
                .filter(|c| !open.contains(c))
                .collect();
            after = Some(project(ids, &options)?);
        }
        "filter" => {
            let mut options = snapshot.options.clone();
            options.hidden = request.hide.take().unwrap_or_default();
            after = Some(project(collapsed(), &options)?);
        }
        "configure" => {
            if let Some(options) = request
                .options
                .as_ref()
                .filter(|o| o.get("configSet").is_none())
                .and_then(|patch| patched(&snapshot.options, patch))
            {
                after = Some(project(collapsed(), &options)?);
            }
        }
        "restore" => {
            if let Some(intent) = &request.intent {
                after = Some(project(intent.collapsed_ids.clone(), &snapshot.options)?);
            }
        }
        "patch" => {
            if let Some(patch) = &request.patch {
                sizes.view_nodes += patch.nodes.len();
                sizes.view_edges += patch.edges.len();
            }
        }
        "add" => {
            sizes.view_nodes += 1;
            sizes.view_edges += 1;
        }
        "refine" | "refineProgressive" => {
            let threads = request
                .threads
                .unwrap_or(1)
                .clamp(1, crate::MAX_THREADS as u64) as usize;
            let work = request
                .work_budget
                .as_ref()
                .and_then(Value::as_u64)
                .map_or(usize::MAX, |w| {
                    usize::try_from(w.max(1)).unwrap_or(usize::MAX)
                });
            sizes.jobs = threads.min(work);
        }
        _ => {}
    }
    if let Some(graph) = after {
        // The larger of the views before and after: closing part of the view while opening another stays bounded.
        let mut grown = Sizes::default();
        view_sizes(&graph, &mut grown);
        sizes.view_nodes = sizes.view_nodes.max(grown.view_nodes);
        sizes.view_edges = sizes.view_edges.max(grown.view_edges);
        sizes.open = sizes.open.max(grown.open);
    }
    let _ = request.scene;
    Ok(())
}

/// The options after a `configure` patch of plain options (keys replaced, as the operation replaces them).
fn patched(options: &LayoutOptions, patch: &Value) -> Option<LayoutOptions> {
    let mut merged = serde_json::to_value(options).ok()?;
    for (key, value) in patch.as_object()? {
        merged[key] = value.clone();
    }
    serde_json::from_value(merged).ok()
}
