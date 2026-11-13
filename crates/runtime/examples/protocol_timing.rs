//! ADR 0019, step 2: where the time of a request goes in the native engine. For one graph it builds the requests a host
//! sends (import with the view collapsed from depth 1, one toggle that opens a container, reading the snapshot alone (`intent`), the scene, the overview) and
//! times, per request, reading the JSON text (`serde_json`), reading it into the typed request, handling it, and writing
//! the answer. Prints one JSON line per request with the medians of `reps` runs after one warm-up, and the sizes.
//!
//!   cargo run --release -p arqlet-runtime --example protocol_timing -- graph.json [reps]
use arqlet_runtime::protocol::{handle, Request};
use serde_json::{json, Value};
use std::time::Instant;

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

/// Times one request text: (parse JSON, typed read, handle, write), each in milliseconds, and the answer text.
fn once(text: &str) -> ([f64; 4], String) {
    let t = Instant::now();
    let value: Value = serde_json::from_str(text).expect("valid JSON");
    let parse = t.elapsed().as_secs_f64() * 1e3;
    let t = Instant::now();
    let request = Request::from_value(&value).expect("a valid request");
    let typed = t.elapsed().as_secs_f64() * 1e3;
    let t = Instant::now();
    let response = handle(request);
    let compute = t.elapsed().as_secs_f64() * 1e3;
    let t = Instant::now();
    let answer = response.to_json();
    let write = t.elapsed().as_secs_f64() * 1e3;
    ([parse, typed, compute, write], answer)
}

fn measure(name: &str, text: &str, reps: usize, nodes: usize) -> String {
    let (_, answer) = once(text);
    let mut parts: [Vec<f64>; 4] = Default::default();
    for _ in 0..reps {
        let (t, _) = once(text);
        for (i, x) in t.iter().enumerate() {
            parts[i].push(*x);
        }
    }
    let [parse, typed, compute, write] = parts.map(median);
    println!(
        "{}",
        json!({"engine": "native", "request": name, "nodes": nodes, "reps": reps,
            "requestBytes": text.len(), "answerBytes": answer.len(),
            "ms": {"parseJson": parse, "readTyped": typed, "handle": compute, "writeAnswer": write,
                   "total": parse + typed + compute + write}})
    );
    answer
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let graph: Value =
        serde_json::from_str(&std::fs::read_to_string(&args[1]).expect("graph file")).unwrap();
    let reps: usize = args.get(2).and_then(|r| r.parse().ok()).unwrap_or(5);
    let nodes = graph["nodes"].as_array().map_or(0, Vec::len);
    let limit = nodes.min(arqlet_runtime::MAX_VIEW_LIMIT);
    let import = json!({"op": "import", "graph": graph, "limit": limit,
        "options": {"levelOfDetail": {"depth": 1}}})
    .to_string();
    let answer: Value = serde_json::from_str(&measure("import", &import, reps, nodes)).unwrap();
    let snapshot = &answer["snapshot"];
    // Open the first collapsed container that is visible.
    let visible: Vec<&str> = snapshot["graph"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|n| n["id"].as_str())
        .collect();
    let container = snapshot["collapsedIds"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .find(|c| visible.contains(c))
        .expect("a collapsed container");
    let toggle = json!({"op": "toggle", "snapshot": snapshot, "nodeId": container,
        "expectedRevision": snapshot["revision"], "expectedLayoutVersion": snapshot["layoutVersion"]})
    .to_string();
    measure("toggle", &toggle, reps, nodes);
    // Reading the snapshot alone (the answer is small): what parsing a snapshot costs.
    let intent = json!({"op": "intent", "snapshot": snapshot}).to_string();
    measure("intent", &intent, reps, nodes);
    let scene = json!({"op": "scene", "snapshot": snapshot}).to_string();
    measure("scene", &scene, reps, nodes);
    let overview = json!({"op": "overview", "graph": graph}).to_string();
    measure("overview", &overview, reps, nodes);
}
