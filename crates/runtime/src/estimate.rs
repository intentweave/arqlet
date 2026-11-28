//! Provisional (ADR 0024, step C3): an estimate of the heap a request will need at its peak, before it runs, for a host that
//! admits work by memory (a worker pool with a memory budget). `{"op": "estimate", "request": {…}}` takes the request it
//! estimates as it would be sent (to `dispatch`, or to a resident session without its snapshot) and answers
//! `{ok, estimate: {op, kind, target, bytes, parts, view, source, jobs, model}}`.
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
//!   layout needs less;
//! - `content`: the content a `change` brings with its payload, as the session keeps it;
//! - `floor`: what the structural floor adds when it is above the rest: the cost of reading the request and handling the
//!   whole source graph (index, structure, copies), which a large source with a small view has whatever its view.
//!
//! The estimate does not validate the request: a stale revision or an unknown node id is estimated all the same (the
//! operation refuses it later). Operations it does not know are refused with `heavy`: `true` for those that lay out or
//! refine (`elkLayout`, `structuredRefine`, the reference operations, …), which a host runs alone, `false` for reads.
//!
//! A calibrated estimate, not a guarantee. On the calibration rows it was at or above every measured peak, with a margin
//! of 1.15 (`MARGIN`); above 4 MB its median was 1.15 to 2.0 times the peak for imports, edits and overviews; on graph
//! shapes left out of the fit it can fall below (the leave-one-shape-out check in the results file). Natively it estimates
//! the counted heap: the resident memory of the process can be up to 3 times that on macOS's allocator. Any change of the
//! layout code changes the peaks: recalibrate with `benchmarks/memory-estimate/fit.py` (README there) and raise
//! `MODEL_VERSION`. Each build estimates for its own target: type sizes differ between 64-bit builds and wasm32
//! (`target` in the answer), so a host that admits WASM workers asks a WASM engine. Deterministic and pure: no layout
//! runs; its own cost is recorded with the calibration.
use crate::protocol::RevealMore;
use arqlet_model::{Graph, GraphPatch, Hidden, Intent, LayoutOptions, Snapshot};
use serde::Deserialize;
use serde_json::{json, Value};

/// The version of the model and its coefficients; a recalibration changes it.
pub const MODEL_VERSION: u32 = 1;

/// The heap a session keeps per byte of a payload's JSON text, for a `change`: about 750 B per node with a few properties
/// against about 50 B of JSON (`benchmarks/results/content-memory-20261122.md`); not calibrated with `change` itself.
const CONTENT_PER_PAYLOAD_BYTE: f64 = 16.0;

/// The margin on everything but the request text: the coefficients reach the calibration rows exactly, a request of
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
    payload: Option<Value>,
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
    /// The JSON text of a `change`'s payload, in bytes.
    payload: usize,
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
    /// The structural floor: at least this many bytes per byte of the source graph's heap, whatever the view (reading
    /// the request and handling the whole source: index, structure, copies). Fitted as the lowest such ratio over the
    /// class's calibration rows above 4 MB, so it raises none of them.
    floor: f64,
    /// The same per byte of request text, through `dispatch` (the text is read, a snapshot request into a JSON value
    /// first); 0 in a session, whose request text is a few bytes.
    floor_text: f64,
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

const fn class(
    text: f64,
    source: f64,
    layout: f64,
    base: f64,
    floor: f64,
    floor_text: f64,
) -> Class {
    Class {
        text,
        source,
        layout,
        base,
        floor,
        floor_text,
    }
}

// COEFFICIENTS-NATIVE-BEGIN
// Generated by `python3 benchmarks/memory-estimate/fit.py report` (Python 3.13.2) from the rows of
// `benchmarks/results/memory-estimate-20261127-*.jsonl`; columns: text, source, layout share, base, floor, text floor.
#[cfg(target_pointer_width = "64")]
const COEFFICIENTS: Coefficients = Coefficients {
    classes: [
        (
            "import/dispatch",
            class(1.0315, 7.8400, 0.9730, 124162.3, 2.6708, 4.9569),
        ),
        (
            "import/session",
            class(1.2776, 2.0708, 0.9907, 194393.6, 2.1195, 0.0000),
        ),
        (
            "open/dispatch",
            class(2.7335, 1.2601, 0.6582, 0.0, 2.0138, 3.0716),
        ),
        (
            "open/session",
            class(1.0000, 0.3113, 1.1325, 8191.3, 0.5981, 0.0000),
        ),
        (
            "keep/dispatch",
            class(1.0000, 1.6590, 0.9007, 249141.5, 1.4389, 2.9627),
        ),
        (
            "keep/session",
            class(1.0000, 2.9059, 1.0659, 36906.7, 2.6385, 0.0000),
        ),
        (
            "patch/dispatch",
            class(1.1870, 1.9329, 0.6922, 1114332.9, 2.0996, 3.1198),
        ),
        (
            "patch/session",
            class(1.0000, 3.9882, 1.1299, 0.0, 4.2598, 0.0000),
        ),
        (
            "refine/dispatch",
            class(5.5130, 0.0390, 0.9007, 2386260.3, 1.7026, 4.0558),
        ),
        (
            "refine/session",
            class(1.0000, 2.5180, 1.0659, 4190911.6, 0.5142, 0.0000),
        ),
        (
            "overview/dispatch",
            class(1.0000, 0.1327, 0.0000, 507931.7, 7.6494, 15.2846),
        ),
    ],
    layout: [0.0, 13576.0, 3581.3, 53.1, 9.9, 5409.6, 3.5, 435.7],
    overview_nodes: 1397.8,
    overview_edges: 1417.9,
};
// COEFFICIENTS-NATIVE-END

// COEFFICIENTS-WASM-BEGIN
// Generated by `python3 benchmarks/memory-estimate/fit.py report` (Python 3.13.2) from the rows of
// `benchmarks/results/memory-estimate-20261127-*.jsonl`; columns: text, source, layout share, base, floor, text floor.
#[cfg(not(target_pointer_width = "64"))]
const COEFFICIENTS: Coefficients = Coefficients {
    classes: [
        (
            "import/dispatch",
            class(2.1703, 11.4731, 0.8964, 1508.0, 6.7015, 6.0276),
        ),
        (
            "import/session",
            class(1.0226, 2.7846, 0.9127, 576235.4, 3.0847, 0.0000),
        ),
        (
            "open/dispatch",
            class(18.0347, 0.0000, 0.1354, 0.0, 9.5860, 12.4375),
        ),
        (
            "open/session",
            class(1.0000, 0.0000, 0.8392, 4249316.1, 0.9390, 0.0000),
        ),
        (
            "keep/dispatch",
            class(17.7421, 0.0000, 0.1400, 0.0, 8.9154, 11.5027),
        ),
        (
            "keep/session",
            class(1.0000, 2.3270, 0.7894, 419066.9, 2.5296, 0.0000),
        ),
        (
            "patch/dispatch",
            class(16.7993, 0.0270, 0.1348, 45637.2, 9.2244, 12.3572),
        ),
        (
            "patch/session",
            class(1.0000, 3.7750, 0.7867, 765573.4, 4.0106, 0.0000),
        ),
        (
            "refine/dispatch",
            class(10.3952, 3.3412, 0.1400, 556059.9, 8.9154, 9.4187),
        ),
        (
            "refine/session",
            class(1.0000, 0.0000, 0.7894, 8190215.0, 1.1096, 0.0000),
        ),
        (
            "overview/dispatch",
            class(8.3941, 1.6319, 0.0000, 0.0, 12.6273, 14.0611),
        ),
    ],
    layout: [0.0, 13576.0, 3581.3, 53.1, 9.9, 5409.6, 3.5, 435.7],
    overview_nodes: 1732.8,
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

/// The operations that lay out or refine but that the estimate does not know: refused with `heavy: true`, so that a host
/// runs them alone instead of taking the refusal for a light request.
const HEAVY_UNESTIMATED: &[&str] = &[
    "structuredRefine",
    "containerRefine",
    "componentRefine",
    "elkLayout",
    "load",
    "referenceCreate",
    "referenceRefresh",
    "referenceReroute",
    "referenceValidate",
    "referenceCanonical",
    "referenceSvg",
    "referenceFromSvg",
    "referenceReproduce",
    "referenceRepair",
];

/// The estimated request's `op`, read first without the rest (another operation's graph need not be a model graph).
fn inner_op(input: &str) -> Result<String, String> {
    #[derive(Deserialize)]
    struct Inner {
        op: Option<String>,
    }
    #[derive(Deserialize)]
    struct Outer {
        request: Inner,
    }
    let outer: Outer = serde_json::from_str(input)
        .map_err(|e| format!("estimate needs {{op: \"estimate\", request}}: {e}"))?;
    outer
        .request
        .op
        .ok_or_else(|| "estimate: the request has no op".to_string())
}

/// A refusal: `heavy` says whether the request lays out (run it alone) or is a light read (run it beside the rest).
fn refusal(error: String, heavy: bool) -> Value {
    json!({"ok": false, "error": error, "heavy": heavy})
}

/// Reads and estimates the request, or refuses with whether it is heavy.
fn estimate_text(input: &str, current: Option<&Snapshot>, place: Place) -> Value {
    let op = match inner_op(input) {
        Ok(op) => op,
        Err(e) => return json!({"ok": false, "error": e}),
    };
    if kind_of(&op).is_none() {
        let heavy = HEAVY_UNESTIMATED.contains(&op.as_str());
        let reason = if heavy {
            "it lays out or refines: run it alone"
        } else {
            "it reads the view and needs little memory"
        };
        return refusal(format!("estimate does not know {op}: {reason}"), heavy);
    }
    let request = match serde_json::from_str::<EstimateText>(input) {
        Ok(outer) if outer.op == "estimate" => outer.request,
        Ok(_) => return json!({"ok": false, "error": "Unsupported operation"}),
        Err(e) => {
            return json!({"ok": false, "error": format!("estimate needs {{op: \"estimate\", request}}: {e}")})
        }
    };
    let text = request_length(input);
    estimate(request, current, text, place).unwrap_or_else(|e| {
        // A request the estimate cannot read the way the operation would (a session's overview without its graph):
        // heavy, since it is one of the operations that lay out.
        refusal(e, true)
    })
}

/// `estimate` through `dispatch`: the request in the text, read in one pass (no JSON value of a large graph).
pub fn estimate_response(input: &str) -> String {
    estimate_text(input, None, Place::Dispatch).to_string()
}

/// `estimate` in a resident session: the request as the session takes it (without a snapshot), on the session's snapshot.
pub(crate) fn estimate_in_session(input: &str, current: Option<&Snapshot>) -> Value {
    estimate_text(input, current, Place::Session)
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
        "patch" | "add" | "change" => "patch",
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
    // Content a `change` brings (its payload), as the session keeps it.
    let content = MARGIN * CONTENT_PER_PAYLOAD_BYTE * sizes.payload as f64;
    // The structural floor: a large source with a small view costs more than the view alone says.
    let at_least = MARGIN
        * (coefficients.floor * sizes.source_heap as f64)
            .max(coefficients.floor_text * sizes.text as f64);
    let floor = (at_least - (input + structure + layout + content)).max(0.0);
    let round = |b: f64| b.max(0.0).ceil() as u64;
    let parts = json!({
        "request": round(request_part),
        "input": round(input),
        "structure": round(structure),
        "layout": round(layout),
        "content": round(content),
        "floor": round(floor),
    });
    let bytes = round(request_part)
        + round(input)
        + round(structure)
        + round(layout)
        + round(content)
        + round(floor);
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

/// An edit of a view: the view it lays out, found as the operation finds it (the larger view where that is simpler to find).
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
            // Closing a container shrinks the view: the view before stands for it.
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
                open.insert(more.container.clone());
            }
            // A container shown in part is estimated shown whole (what `partial: false` and paging to the end show).
            for id in open.clone() {
                if options.partial.remove(&id).is_some() {
                    open.insert(format!("{}{}", arqlet_layout::communities::REST_PREFIX, id));
                }
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
        "patch" | "change" => {
            if let Some(payload) = &request.payload {
                sizes.payload = serde_json::to_string(payload).map_or(0, |t| t.len());
            }
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
        // The larger of the views before and after: closing part of the view while opening another stays covered.
        let mut grown = Sizes::default();
        view_sizes(&graph, &mut grown);
        sizes.view_nodes = sizes.view_nodes.max(grown.view_nodes);
        sizes.view_edges = sizes.view_edges.max(grown.view_edges);
        sizes.open = sizes.open.max(grown.open);
    }
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
