//! The counted heap peak of single requests (ADR 0024, C3): an import, edits on its snapshot (toggle, direction, patch),
//! a refinement (one and four threads) and an overview, each through `dispatch` (the request text with its snapshot, as a
//! stateless host sends it) and in a resident session (`EngineSession`, the request without a snapshot). Next to each peak:
//! the size of the view before and after, the peak of laying out the view after alone, and the engine's own estimate
//! (`estimate`) with the time it took. One JSON line per request on stdout.
//!
//! Usage: `request-peak GRAPH.json LIMIT OPTIONS_JSON [OPS]`, OPS a comma list of `import,toggle,direction,patch,refine,
//! refine4,overview` (default all). The peak counts heap bytes requested from the allocator above what was live before the
//! request text was built (so the text is part of it), not resident memory.
use serde_json::{json, Value};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
use std::time::Instant;

struct Counting;
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

fn grow(n: usize) {
    let now = LIVE.fetch_add(n, Relaxed) + n;
    PEAK.fetch_max(now, Relaxed);
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = System.alloc(layout);
        if !p.is_null() {
            grow(layout.size());
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        System.dealloc(p, layout);
        LIVE.fetch_sub(layout.size(), Relaxed);
    }
    unsafe fn realloc(&self, p: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let q = System.realloc(p, layout, size);
        if !q.is_null() {
            if size > layout.size() {
                grow(size - layout.size());
            } else {
                LIVE.fetch_sub(layout.size() - size, Relaxed);
            }
        }
        q
    }
}

#[global_allocator]
static ALLOC: Counting = Counting;

fn live() -> usize {
    LIVE.load(Relaxed)
}

/// Runs `f` and returns its result with the heap peak above what was live before, and the milliseconds it took.
fn measured<T>(f: impl FnOnce() -> T) -> (T, usize, f64) {
    let base = live();
    PEAK.store(base, Relaxed);
    let start = Instant::now();
    let value = f();
    let ms = start.elapsed().as_secs_f64() * 1000.0;
    (value, PEAK.load(Relaxed).saturating_sub(base), ms)
}

/// The size of a snapshot's view: nodes, edges, routes, containers shown open (a node that is some node's parent).
fn view_size(snapshot: &Value) -> Value {
    let nodes = snapshot["graph"]["nodes"].as_array().map_or(&[][..], Vec::as_slice);
    let parents: std::collections::BTreeSet<&str> =
        nodes.iter().filter_map(|n| n["parentId"].as_str()).collect();
    json!({
        "nodes": nodes.len(),
        "edges": snapshot["graph"]["edges"].as_array().map_or(0, Vec::len),
        "routes": snapshot["routes"].as_array().map_or(0, Vec::len),
        "open": parents.len(),
        "collapsed": snapshot["collapsedIds"].as_array().map_or(0, Vec::len),
        "source": snapshot["sourceGraph"]["nodes"].as_array().map_or(0, Vec::len),
    })
}

/// The container a toggle opens: among the collapsed ones, the one with the median number of children in the source
/// graph; with none collapsed, the shown container with the most children (the toggle closes it).
fn toggle_target(snapshot: &Value) -> Option<String> {
    let source = snapshot["sourceGraph"]["nodes"]
        .as_array()
        .or_else(|| snapshot["graph"]["nodes"].as_array())?;
    let mut children: std::collections::BTreeMap<&str, usize> = Default::default();
    for n in source {
        if let Some(p) = n["parentId"].as_str() {
            *children.entry(p).or_default() += 1;
        }
    }
    let shown: std::collections::BTreeSet<&str> = snapshot["graph"]["nodes"]
        .as_array()?
        .iter()
        .filter_map(|n| n["id"].as_str())
        .collect();
    let mut collapsed: Vec<(usize, &str)> = snapshot["collapsedIds"]
        .as_array()
        .map(|c| c.iter().filter_map(Value::as_str).collect::<Vec<_>>())
        .unwrap_or_default()
        .into_iter()
        .filter(|id| !id.starts_with("rest:") && shown.contains(id))
        .filter_map(|id| children.get(id).map(|&c| (c, id)))
        .collect();
    collapsed.sort();
    if let Some(&(_, id)) = collapsed.get(collapsed.len() / 2) {
        return Some(id.to_string());
    }
    children
        .iter()
        .filter(|(id, _)| shown.contains(*id))
        .max_by_key(|(id, c)| (**c, std::cmp::Reverse(**id)))
        .map(|(id, _)| id.to_string())
}

/// Ten new nodes beside the first shown leaf, each with an edge to it.
fn patch_of(snapshot: &Value) -> Value {
    let nodes = snapshot["graph"]["nodes"].as_array().cloned().unwrap_or_default();
    let parents: std::collections::BTreeSet<String> =
        nodes.iter().filter_map(|n| n["parentId"].as_str().map(String::from)).collect();
    let leaf = nodes
        .iter()
        .find(|n| !parents.contains(n["id"].as_str().unwrap_or("")))
        .or(nodes.first())
        .cloned()
        .unwrap_or(json!({}));
    let id = leaf["id"].as_str().unwrap_or("").to_string();
    let parent = leaf["parentId"].clone();
    let new: Vec<Value> = (0..10)
        .map(|i| json!({"id": format!("peak-new-{i}"), "kind": "symbol", "label": format!("new {i}"), "parentId": parent}))
        .collect();
    let edges: Vec<Value> = (0..10)
        .map(|i| json!({"id": format!("peak-edge-{i}"), "kind": "calls", "source": format!("peak-new-{i}"), "target": id}))
        .collect();
    json!({"nodes": new, "edges": edges})
}

/// The request of an operation, without the snapshot (a session's form).
fn op_request(op: &str, snapshot: &Value) -> Option<Value> {
    Some(match op {
        "toggle" => json!({"op": "toggle", "nodeId": toggle_target(snapshot)?}),
        "direction" => json!({"op": "direction", "direction": "lr"}),
        "patch" => json!({"op": "patch", "patch": patch_of(snapshot)}),
        "refine" => json!({"op": "refine", "threads": 1, "workBudget": 4}),
        "refine4" => json!({"op": "refine", "threads": 4, "workBudget": 4}),
        _ => return None,
    })
}

fn with_snapshot(mut request: Value, snapshot: &Value) -> Value {
    request["snapshot"] = snapshot.clone();
    request["expectedRevision"] = snapshot["revision"].clone();
    request["expectedLayoutVersion"] = snapshot["layoutVersion"].clone();
    request
}

/// The engine's estimate of a request (`estimate`), with its time and its own peak; `null` when the engine has none.
fn estimate(run: &mut dyn FnMut(&str) -> String, request: &str) -> Value {
    let text = format!("{{\"op\":\"estimate\",\"request\":{request}}}");
    let (answer, peak, ms) = measured(|| run(&text));
    match serde_json::from_str::<Value>(&answer) {
        Ok(mut v) if v["ok"] == true => {
            let mut e = v["estimate"].take();
            e["ms"] = json!(ms);
            e["peak"] = json!(peak);
            e
        }
        Ok(v) => json!({"error": v["error"]}),
        Err(e) => json!({"error": e.to_string()}),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = args.get(1).expect("usage: request-peak GRAPH.json LIMIT OPTIONS_JSON [OPS]");
    let limit: usize = args.get(2).map_or(100, |l| l.parse().expect("limit"));
    let options: Value = args.get(3).map_or(json!({}), |o| serde_json::from_str(o).expect("options JSON"));
    let ops: Vec<String> = args.get(4).map_or(
        "import,toggle,direction,patch,refine,refine4,overview".split(',').map(String::from).collect(),
        |o| o.split(',').map(String::from).collect(),
    );
    let mode = std::env::var("REQUEST_PEAK_MODE").unwrap_or_else(|_| "both".into());
    let text = std::fs::read_to_string(path).expect("graph file");
    let graph: Value = serde_json::from_str(&text).expect("graph JSON");
    let name = std::path::Path::new(path).file_stem().map_or(String::new(), |s| s.to_string_lossy().into());
    let nodes = graph["nodes"].as_array().map_or(0, Vec::len);
    let edges = graph["edges"].as_array().map_or(0, Vec::len);
    drop(graph);
    let common = json!({"graph": name, "nodes": nodes, "edges": edges, "limit": limit, "options": options,
        "pointer": std::mem::size_of::<usize>() * 8});
    let emit = |mut row: Value| {
        for (k, v) in common.as_object().unwrap() {
            row[k] = v.clone();
        }
        println!("{row}");
    };
    let mut dispatch = |t: &str| arqlet_runtime::dispatch(t);

    // The import, through `dispatch`.
    let import_text = format!(
        "{{\"op\":\"import\",\"limit\":{limit},\"options\":{options},\"graph\":{}}}",
        text.trim()
    );
    let base = live();
    PEAK.store(base, Relaxed);
    let start = Instant::now();
    let answer = arqlet_runtime::dispatch(&import_text);
    let import_ms = start.elapsed().as_secs_f64() * 1000.0;
    let import_peak = PEAK.load(Relaxed).saturating_sub(base) + import_text.len();
    let answer_bytes = answer.len();
    let imported: Value = serde_json::from_str(&answer).expect("import answer");
    drop(answer);
    assert_eq!(imported["ok"], true, "{}", imported["error"]);
    let snapshot = imported["snapshot"].clone();
    drop(imported);
    let after = view_size(&snapshot);
    // The layout of the first view alone, from its graph and options.
    let layout_alone = |snapshot: &Value| -> (usize, f64) {
        let graph: arqlet_model::Graph = serde_json::from_value(snapshot["graph"].clone()).expect("view graph");
        let options: arqlet_model::LayoutOptions =
            serde_json::from_value(snapshot["options"].clone()).unwrap_or_default();
        let (_s, peak, ms) = measured(|| arqlet_layout::compute(&graph, None, &Default::default(), &options).ok());
        (peak, ms)
    };
    if ops.iter().any(|o| o == "import") && mode != "session" {
        let (layout_peak, layout_ms) = layout_alone(&snapshot);
        let est = estimate(&mut dispatch, &import_text);
        emit(json!({"mode": "dispatch", "op": "import", "textBytes": import_text.len(), "answerBytes": answer_bytes,
            "peak": import_peak, "ms": import_ms, "viewBefore": null, "viewAfter": after,
            "layoutPeak": layout_peak, "layoutMs": layout_ms, "estimate": est}));
    }
    drop(import_text);

    if mode != "session" {
        for op in ops.iter().filter(|o| o.as_str() != "import" && o.as_str() != "overview") {
            let Some(request) = op_request(op, &snapshot) else {
                emit(json!({"mode": "dispatch", "op": op, "skipped": "no target"}));
                continue;
            };
            let full = with_snapshot(request, &snapshot);
            let base = live();
            PEAK.store(base, Relaxed);
            let request_text = full.to_string();
            drop(full);
            let start = Instant::now();
            let answer = arqlet_runtime::dispatch(&request_text);
            let ms = start.elapsed().as_secs_f64() * 1000.0;
            let peak = PEAK.load(Relaxed).saturating_sub(base);
            let answer_bytes = answer.len();
            let v: Value = serde_json::from_str(&answer).expect("answer");
            drop(answer);
            if v["ok"] != true {
                emit(json!({"mode": "dispatch", "op": op, "error": v["error"]}));
                continue;
            }
            let (layout_peak, layout_ms) = layout_alone(&v["snapshot"]);
            let est = estimate(&mut dispatch, &request_text);
            emit(json!({"mode": "dispatch", "op": op, "textBytes": request_text.len(), "answerBytes": answer_bytes,
                "peak": peak, "ms": ms, "viewBefore": after, "viewAfter": view_size(&v["snapshot"]),
                "layoutPeak": layout_peak, "layoutMs": layout_ms, "estimate": est}));
        }
    }

    if mode != "dispatch" {
        // A resident session: the import, then each operation on the session's current view.
        let mut session = arqlet_runtime::engine_session::EngineSession::new();
        let import_text = format!(
            "{{\"op\":\"import\",\"limit\":{limit},\"options\":{options},\"scene\":false,\"graph\":{}}}",
            text.trim()
        );
        let base = live();
        PEAK.store(base, Relaxed);
        let start = Instant::now();
        let answer = session.run(&import_text);
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        let peak = PEAK.load(Relaxed).saturating_sub(base) + import_text.len();
        assert!(answer.contains("\"ok\":true"), "{}", &answer[..answer.len().min(300)]);
        drop(answer);
        let est = {
            let mut fresh = arqlet_runtime::engine_session::EngineSession::new();
            estimate(&mut |t: &str| fresh.run(t), &import_text)
        };
        drop(import_text);
        if ops.iter().any(|o| o == "import") {
            emit(json!({"mode": "session", "op": "import", "peak": peak, "ms": ms, "viewAfter": after,
                "estimate": est}));
        }
        for op in ops.iter().filter(|o| o.as_str() != "import" && o.as_str() != "overview") {
            let current = serde_json::to_value(session.snapshot().expect("a snapshot")).expect("snapshot");
            let before = view_size(&current);
            let Some(mut request) = op_request(op, &current) else {
                emit(json!({"mode": "session", "op": op, "skipped": "no target"}));
                continue;
            };
            drop(current);
            request["scene"] = json!(false);
            let request_text = request.to_string();
            let est = estimate(&mut |t: &str| session.run(t), &request_text);
            let base = live();
            PEAK.store(base, Relaxed);
            let start = Instant::now();
            let answer = session.run(&request_text);
            let ms = start.elapsed().as_secs_f64() * 1000.0;
            let peak = PEAK.load(Relaxed).saturating_sub(base);
            let ok = answer.contains("\"ok\":true");
            drop(answer);
            if !ok {
                emit(json!({"mode": "session", "op": op, "error": "refused"}));
                continue;
            }
            let current = serde_json::to_value(session.snapshot().expect("a snapshot")).expect("snapshot");
            emit(json!({"mode": "session", "op": op, "peak": peak, "ms": ms, "viewBefore": before,
                "viewAfter": view_size(&current), "estimate": est}));
        }
    }

    if ops.iter().any(|o| o == "overview") && mode != "session" {
        let request_text = format!("{{\"op\":\"overview\",\"graph\":{}}}", text.trim());
        drop(text);
        let base = live();
        PEAK.store(base, Relaxed);
        let start = Instant::now();
        let answer = arqlet_runtime::dispatch(&request_text);
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        let peak = PEAK.load(Relaxed).saturating_sub(base) + request_text.len();
        let ok = answer.contains("\"ok\":true");
        let answer_bytes = answer.len();
        drop(answer);
        let est = estimate(&mut dispatch, &request_text);
        emit(json!({"mode": "dispatch", "op": "overview", "ok": ok, "textBytes": request_text.len(),
            "answerBytes": answer_bytes, "peak": peak, "ms": ms, "estimate": est}));
    }
}
