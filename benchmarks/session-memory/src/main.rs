//! Measures the heap of a resident engine session (`EngineSession`) after an import: the bytes it keeps, the peak during
//! the import, and the parts the kept bytes consist of. Usage: `session-memory GRAPH.json [LIMIT] [OPTIONS_JSON]`.
//! Counts heap bytes requested from the allocator (live and peak), not resident memory of the process.
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

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

#[cfg(not(feature = "profile"))]
#[global_allocator]
static ALLOC: Counting = Counting;

#[cfg(feature = "profile")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

fn live() -> usize {
    LIVE.load(Relaxed)
}

/// The heap bytes that `make` leaves alive (the value it returns is kept until the measurement ends).
fn kept<T>(make: impl FnOnce() -> T) -> (T, usize) {
    let before = live();
    let value = make();
    (value, live().saturating_sub(before))
}

fn mb(bytes: usize) -> String {
    format!("{:.1} MB", bytes as f64 / 1_048_576.0)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = args.get(1).expect("usage: session-memory GRAPH.json [LIMIT] [OPTIONS_JSON]");
    let limit: usize = args.get(2).map_or(100, |l| l.parse().expect("limit"));
    let options: serde_json::Value = args
        .get(3)
        .map_or(serde_json::json!({}), |o| serde_json::from_str(o).expect("options JSON"));
    #[cfg(feature = "profile")]
    {
        // Only the import from a typed graph, so the profile's peak is the import's.
        let text = std::fs::read_to_string(path).expect("graph file");
        let typed: arqlet_model::Graph = serde_json::from_str(&text).expect("graph");
        drop(text);
        let options = arqlet_runtime::options_from_patch(&options).expect("options");
        let _profiler = dhat::Profiler::new_heap();
        drop(arqlet_runtime::import_graph_with(typed, limit, &options).expect("import"));
        return;
    }
    #[allow(unreachable_code)]
    let text = std::fs::read_to_string(path).expect("graph file");
    let (graph, graph_bytes) = kept(|| serde_json::from_str::<serde_json::Value>(&text).expect("graph JSON"));
    let nodes = graph["nodes"].as_array().map_or(0, Vec::len);
    let edges = graph["edges"].as_array().map_or(0, Vec::len);
    drop(graph);
    // The request as a host sends it to a resident session (the browser: one JSON text through `lw_session_run`).
    let request = format!(
        "{{\"op\":\"import\",\"limit\":{limit},\"options\":{options},\"scene\":false,\"graph\":{}}}",
        text.trim()
    );
    drop(text);

    let mut session = arqlet_runtime::engine_session::EngineSession::new();
    let base = live();
    PEAK.store(base, Relaxed);
    // `--value` (env SESSION_MEMORY_VALUE=1): the request read as a JSON value first and then run, the way a session
    // read every request before the one-pass import.
    let answer = if std::env::var_os("SESSION_MEMORY_VALUE").is_some() {
        let v: serde_json::Value = serde_json::from_str(&request).expect("request JSON");
        session.run_value(&v).to_string()
    } else {
        session.run(&request)
    };
    assert!(answer.contains("\"ok\":true"), "{}", &answer[..answer.len().min(300)]);
    drop(answer);
    let peak = PEAK.load(Relaxed) - base;
    let session_kept = live().saturating_sub(base);
    let request_bytes = request.len();
    drop(request);

    // Content (ADR 0023, R1): with SESSION_MEMORY_KEYS=n, n properties per node (a string, an integer, a float, …) set in
    // one payload; the heap the session keeps for them.
    let keys: usize = std::env::var("SESSION_MEMORY_KEYS").ok().and_then(|k| k.parse().ok()).unwrap_or(0);
    let mut content_ids = 0usize;
    let content_kept = if keys > 0 {
        let ids: Vec<String> = session.snapshot().expect("a snapshot").source_graph.as_deref()
            .map(|g| g.nodes.iter().map(|n| n.id.clone()).collect()).unwrap_or_default();
        content_ids = ids.len();
        let mut table = serde_json::Map::new();
        for (i, id) in ids.iter().enumerate() {
            let mut properties = serde_json::Map::new();
            for k in 0..keys {
                let value = match k % 3 {
                    0 => serde_json::json!(format!("owner-{}", i % 97)),
                    1 => serde_json::json!(i as i64 * 7 + k as i64),
                    _ => serde_json::json!(i as f64 / 3.0),
                };
                properties.insert(format!("key{k}"), value);
            }
            table.insert(id.clone(), serde_json::Value::Object(properties));
        }
        let request = serde_json::json!({"op": "payload", "set": {"nodes": table}});
        let before = live();
        let answer = session.run_value(&request);
        assert_eq!(answer["ok"], true, "{answer}");
        drop(answer);
        let kept = live().saturating_sub(before);
        drop(request);
        Some(kept)
    } else {
        None
    };
    // The parts, each measured as the heap a copy of it takes (a shared Arc counted once).
    let s = session.snapshot().expect("a snapshot");
    let (_a, source) = kept(|| s.source_graph.as_deref().cloned());
    let (_b, view_graph) = kept(|| s.graph.clone());
    let (_c, boxes) = kept(|| s.boxes.clone());
    let (_d, routes) = kept(|| s.routes.clone());
    let (_e, options_bytes) = kept(|| s.options.clone());
    let structure = s.structure.as_deref();
    let shares_source = structure.is_some_and(|st| {
        s.source_graph.as_ref().is_some_and(|src| std::sync::Arc::ptr_eq(src, &st.view))
    });
    let (_f, structure_view) = kept(|| structure.map(|st| (*st.view).clone()));
    let (_g, index) = kept(|| structure.map(|st| (*st.index).clone()));
    let key = structure.map_or(0, |st| st.key.capacity());
    let (_h, rest) = kept(|| {
        let mut copy = s.clone();
        copy.source_graph = None;
        copy.structure = None;
        copy.graph.nodes = Vec::new();
        copy.graph.edges = Vec::new();
        copy.graph.buses = Vec::new();
        copy.boxes = Default::default();
        copy.routes = Default::default();
        copy.options = Default::default();
        copy
    });
    let per = |b: usize| format!("{:.0} B/node", b as f64 / nodes.max(1) as f64);
    // The peaks of the import's phases, each above what it starts from (the graph text, then the typed graph).
    let phase = |f: &dyn Fn()| {
        let base = live();
        PEAK.store(base, Relaxed);
        f();
        PEAK.load(Relaxed) - base
    };
    let text = std::fs::read_to_string(path).expect("graph file");
    let typed_options: arqlet_model::LayoutOptions =
        arqlet_runtime::options_from_patch(&options).expect("options");
    let parse = phase(&|| drop(serde_json::from_str::<arqlet_model::Graph>(&text).expect("graph")));
    let typed: arqlet_model::Graph = serde_json::from_str(&text).expect("graph");
    let validate = phase(&|| arqlet_model::validate(&typed).expect("valid"));
    let index = phase(&|| drop(arqlet_model::GraphIndex::of(&typed)));
    let import = phase(&|| drop(arqlet_runtime::import_graph_with(typed.clone(), limit, &typed_options).expect("import")));
    let graph_copy = phase(&|| drop(typed.clone()));
    println!("graph {path}: {nodes} nodes, {edges} edges; limit {limit}; options {options}");
    println!("view: {} nodes, {} routes", s.graph.nodes.len(), s.routes.len());
    println!("request text: {} ({})", mb(request_bytes), per(request_bytes));
    println!("the graph as a serde_json::Value (for comparison): {} ({})", mb(graph_bytes), per(graph_bytes));
    println!("session kept after the import: {} ({})", mb(session_kept), per(session_kept));
    println!("peak during the import, above the request text: {} ({})", mb(peak), per(peak));
    println!("phase peaks: parse {} · validate {} · index {} · import (with a copy of the graph, {}) {}",
        mb(parse), mb(validate), mb(index), mb(graph_copy), mb(import));
    if let Some(bytes) = content_kept {
        println!(
            "content of {keys} properties per node kept: {} ({:.0} B per node with properties; {content_ids} such nodes)",
            mb(bytes),
            bytes as f64 / content_ids.max(1) as f64
        );
    }
    println!("parts (heap of a copy):");
    println!("  source graph          {:>10}  {}", mb(source), per(source));
    if shares_source {
        println!("  structure view        shares the source graph (no copy)");
    } else {
        println!("  structure view        {:>10}  {}", mb(structure_view), per(structure_view));
    }
    println!("  structure index       {:>10}  {}", mb(index), per(index));
    println!("  structure key         {:>10}", mb(key));
    println!("  view graph (bounded)  {:>10}", mb(view_graph));
    println!("  boxes                 {:>10}", mb(boxes));
    println!("  routes                {:>10}", mb(routes));
    println!("  options               {:>10}", mb(options_bytes));
    println!("  rest of the snapshot  {:>10}", mb(rest));
}
