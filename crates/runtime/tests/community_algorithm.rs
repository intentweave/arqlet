//! Leiden beside Louvain (#237, provisional `communityAlgorithm` and `communityResolution`, in the overview's options and
//! in `LayoutOptions`): the defaults change nothing, Leiden is deterministic for any thread count, and a session builds its
//! hierarchy again when the algorithm changes.
use arqlet_runtime::dispatch;
use serde_json::{json, Value};

fn call(request: Value) -> Value {
    serde_json::from_str(&dispatch(&request.to_string())).unwrap()
}

/// A flat graph of 240 nodes: twelve rings of twenty with chords, and a link between neighbouring rings.
fn rings() -> Value {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    for i in 0..240 {
        let ring = i / 20;
        let id = format!("n{i:03}");
        nodes.push(json!({"id": id, "kind": "fn", "label": format!("f{i}")}));
        for (k, step) in [("r", 1), ("x", 7)] {
            edges.push(
                json!({"id": format!("{k}{i}"), "kind": "calls", "source": id,
                "target": format!("n{:03}", ring * 20 + (i + step) % 20)}),
            );
        }
        if i % 20 == 0 {
            edges.push(json!({"id": format!("b{i}"), "kind": "calls", "source": id,
                "target": format!("n{:03}", (i + 25) % 240)}));
        }
    }
    json!({"nodes": nodes, "edges": edges})
}

fn overview(options: Value, threads: u32) -> Value {
    let r =
        call(json!({"op": "overview", "graph": rings(), "options": options, "threads": threads}));
    assert_eq!(r["ok"], true, "{r}");
    r["overview"].clone()
}

#[test]
fn explicit_defaults_answer_as_the_omitted_ones() {
    let plain = overview(json!({"communities": 60}), 1);
    let explicit = overview(
        json!({"communities": 60, "communityAlgorithm": "louvain", "communityResolution": 1}),
        1,
    );
    assert_eq!(plain, explicit);
}

#[test]
fn leiden_divides_the_overview_the_same_for_any_thread_count() {
    let one = overview(
        json!({"communities": 60, "communityAlgorithm": "leiden"}),
        1,
    );
    let four = overview(
        json!({"communities": 60, "communityAlgorithm": "leiden"}),
        4,
    );
    assert_eq!(one, four);
    assert!(
        one["stats"]["communities"].as_u64().unwrap() > 0,
        "{}",
        one["stats"]
    );
    // A higher resolution gives at least as many clusters.
    let fine = overview(
        json!({"communities": 60, "communityAlgorithm": "leiden", "communityResolution": 3}),
        2,
    );
    assert!(fine["stats"]["communities"].as_u64() >= one["stats"]["communities"].as_u64());
}

#[test]
fn wrong_values_are_refused() {
    for options in [
        json!({"communityAlgorithm": "infomap"}),
        json!({"communityResolution": 0}),
        json!({"communityResolution": -1}),
        json!({"communityResolution": "high"}),
    ] {
        let r = call(json!({"op": "overview", "graph": rings(), "options": options}));
        assert_eq!(r["ok"], false, "{options}: {r}");
        assert!(r["error"].as_str().unwrap().contains("community"), "{r}");
    }
    let r = call(json!({"op": "import", "graph": rings(), "limit": 300,
        "options": {"communities": 60, "communityResolution": 0}}));
    assert_eq!(r["ok"], false, "{r}");
    assert!(
        r["error"].as_str().unwrap().contains("communityResolution"),
        "{r}"
    );
}

#[test]
fn a_session_clusters_again_when_the_algorithm_changes() {
    use arqlet_runtime::engine_session::EngineSession;
    let mut session = EngineSession::new();
    let louvain = session
        .run_value(&json!({"op": "overview", "graph": rings(), "options": {"communities": 60}}));
    assert_eq!(louvain["ok"], true, "{louvain}");
    let leiden = session.run_value(&json!({"op": "overview",
        "options": {"communities": 60, "communityAlgorithm": "leiden"}}));
    assert_eq!(leiden["ok"], true, "{leiden}");
    assert_eq!(
        leiden["overview"],
        overview(
            json!({"communities": 60, "communityAlgorithm": "leiden"}),
            1
        ),
        "the same as dispatch"
    );
}

#[test]
fn the_layout_takes_the_options_and_omits_the_defaults() {
    let plain = call(
        json!({"op": "import", "graph": rings(), "limit": 300, "options": {"communities": 60}}),
    );
    assert_eq!(plain["ok"], true, "{plain}");
    assert!(plain["snapshot"]["options"]
        .get("communityAlgorithm")
        .is_none());
    assert!(plain["snapshot"]["options"]
        .get("communityResolution")
        .is_none());
    let explicit = call(json!({"op": "import", "graph": rings(), "limit": 300,
        "options": {"communities": 60, "communityAlgorithm": "louvain"}}));
    assert_eq!(
        explicit["snapshot"], plain["snapshot"],
        "an explicit default is the default"
    );
    let leiden = call(json!({"op": "import", "graph": rings(), "limit": 300,
        "options": {"communities": 60, "communityAlgorithm": "leiden", "communityResolution": 1.5}}));
    assert_eq!(leiden["ok"], true, "{leiden}");
    assert_eq!(
        leiden["snapshot"]["options"]["communityAlgorithm"],
        "leiden"
    );
    assert_eq!(leiden["snapshot"]["options"]["communityResolution"], 1.5);
    // The communities operation hands the same view's communities to a host as groups, with the algorithm.
    let groups = call(json!({"op": "communities", "graph": rings(),
        "options": {"communities": 60, "communityAlgorithm": "leiden"}}));
    assert_eq!(groups["ok"], true, "{groups}");
    assert!(!groups["groups"].as_object().unwrap().is_empty());
}
