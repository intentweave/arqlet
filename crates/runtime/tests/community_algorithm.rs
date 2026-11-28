//! Leiden beside Louvain (#237, provisional `communityAlgorithm` and `communityResolution`, in the overview's options and
//! in `LayoutOptions`): the defaults change nothing, both options change the clusters, the same for any thread count, and a
//! session builds its hierarchy again when either changes.
use arqlet_runtime::dispatch;
use serde_json::{json, Value};

fn call(request: Value) -> Value {
    serde_json::from_str(&dispatch(&request.to_string())).unwrap()
}

/// A flat graph of 300 nodes with six planted groups (70 % of the links inside a group), from a fixed xorshift stream:
/// on it Louvain and Leiden, and Leiden at different resolutions, give different clusters (asserted below), so that the
/// tests can tell the options apart.
fn planted() -> Value {
    let mut state: u64 = 0x2545_f491_4f6c_dd1d;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let (k, groups) = (300u64, 6u64);
    let nodes: Vec<Value> = (0..k)
        .map(|i| json!({"id": format!("n{i}"), "kind": "fn", "label": format!("f{i}")}))
        .collect();
    let mut seen = std::collections::BTreeSet::new();
    let mut edges = Vec::new();
    for e in 0..3 * k {
        let a = next() % k;
        let b = if next() % 10 < 7 {
            ((next() % (k / groups)) * groups + a % groups).min(k - 1)
        } else {
            next() % k
        };
        if a != b && seen.insert((a.min(b), a.max(b))) {
            edges.push(
                json!({"id": format!("e{e}"), "kind": "calls", "source": format!("n{a}"),
                "target": format!("n{b}")}),
            );
        }
    }
    json!({"nodes": nodes, "edges": edges})
}

fn overview(options: Value, threads: u32) -> Value {
    let r =
        call(json!({"op": "overview", "graph": planted(), "options": options, "threads": threads}));
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
fn the_algorithm_and_the_resolution_change_the_clusters_the_same_for_any_thread_count() {
    let louvain = overview(json!({"communities": 60}), 1);
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
    assert_ne!(
        one["parent"], louvain["parent"],
        "Leiden and Louvain cluster this graph differently"
    );
    for (options, other) in [
        (
            json!({"communities": 60, "communityAlgorithm": "leiden", "communityResolution": 0.3}),
            &one,
        ),
        (
            json!({"communities": 60, "communityAlgorithm": "leiden", "communityResolution": 3}),
            &one,
        ),
        (
            json!({"communities": 60, "communityResolution": 3}),
            &louvain,
        ),
    ] {
        let at = overview(options.clone(), 2);
        assert_ne!(
            at["parent"], other["parent"],
            "{options} changes the clusters"
        );
        assert_eq!(at, overview(options, 1), "the same for any thread count");
    }
}

#[test]
fn wrong_values_are_refused() {
    for options in [
        json!({"communityAlgorithm": "infomap"}),
        json!({"communityResolution": 0}),
        json!({"communityResolution": -1}),
        json!({"communityResolution": "high"}),
    ] {
        let r = call(json!({"op": "overview", "graph": planted(), "options": options}));
        assert_eq!(r["ok"], false, "{options}: {r}");
        assert!(r["error"].as_str().unwrap().contains("community"), "{r}");
    }
    // In the layout options, refused when the options are read, with or without communities, at import, at configure
    // and in the communities operation.
    for options in [
        json!({"communities": 60, "communityResolution": 0}),
        json!({"communityResolution": -1}),
        json!({"communityResolution": 0}),
    ] {
        let r = call(json!({"op": "import", "graph": planted(), "limit": 300, "options": options}));
        assert_eq!(r["ok"], false, "{options}: {r}");
        assert!(
            r["error"].as_str().unwrap().contains("communityResolution"),
            "{r}"
        );
        let r = call(json!({"op": "communities", "graph": planted(), "options": options}));
        assert_eq!(r["ok"], false, "{options}: {r}");
    }
    let s = call(json!({"op": "import", "graph": planted(), "limit": 300}));
    let r = call(
        json!({"op": "configure", "snapshot": s["snapshot"], "expectedRevision": s["snapshot"]["revision"],
        "expectedLayoutVersion": s["snapshot"]["layoutVersion"], "options": {"communityResolution": -1}}),
    );
    assert_eq!(r["ok"], false, "{r}");
    assert!(
        r["error"].as_str().unwrap().contains("communityResolution"),
        "{r}"
    );
}

#[test]
fn a_session_clusters_again_when_the_algorithm_or_the_resolution_changes() {
    use arqlet_runtime::engine_session::EngineSession;
    let mut session = EngineSession::new();
    let mut first = true;
    for options in [
        json!({"communities": 60}),
        json!({"communities": 60, "communityAlgorithm": "leiden"}),
        json!({"communities": 60, "communityAlgorithm": "leiden", "communityResolution": 3}),
        json!({"communities": 60, "communityResolution": 3}),
        json!({"communities": 60}),
    ] {
        // `start: {}` continues from the session's last layout while the cluster options stay the same; with other
        // options it must cluster again, and then answer as a fresh overview with those options.
        let request = if first {
            json!({"op": "overview", "graph": planted(), "options": options})
        } else {
            json!({"op": "overview", "options": options})
        };
        first = false;
        let answer = session.run_value(&request);
        assert_eq!(answer["ok"], true, "{answer}");
        assert_eq!(
            answer["overview"],
            overview(options.clone(), 1),
            "{options}: the same as dispatch"
        );
    }
}

#[test]
fn the_layout_takes_the_options_and_omits_the_defaults() {
    let plain = call(
        json!({"op": "import", "graph": planted(), "limit": 300, "options": {"communities": 60}}),
    );
    assert_eq!(plain["ok"], true, "{plain}");
    assert!(plain["snapshot"]["options"]
        .get("communityAlgorithm")
        .is_none());
    assert!(plain["snapshot"]["options"]
        .get("communityResolution")
        .is_none());
    let explicit = call(json!({"op": "import", "graph": planted(), "limit": 300,
        "options": {"communities": 60, "communityAlgorithm": "louvain"}}));
    assert_eq!(
        explicit["snapshot"], plain["snapshot"],
        "an explicit default is the default"
    );
    // An explicit resolution of 1 is the default and is not stored either.
    let one = call(json!({"op": "import", "graph": planted(), "limit": 300,
        "options": {"communities": 60, "communityResolution": 1}}));
    assert_eq!(
        one["snapshot"], plain["snapshot"],
        "an explicit resolution 1 is the default"
    );
    let one = call(json!({"op": "import", "graph": planted(), "limit": 300,
        "options": {"communities": 60, "communityResolution": 1.0}}));
    assert_eq!(one["snapshot"], plain["snapshot"]);
    let leiden = call(json!({"op": "import", "graph": planted(), "limit": 300,
        "options": {"communities": 60, "communityAlgorithm": "leiden", "communityResolution": 1.5}}));
    assert_eq!(leiden["ok"], true, "{leiden}");
    assert_eq!(
        leiden["snapshot"]["options"]["communityAlgorithm"],
        "leiden"
    );
    assert_eq!(leiden["snapshot"]["options"]["communityResolution"], 1.5);
    // The communities operation hands the same view's communities to a host as groups, with the algorithm.
    let groups = call(json!({"op": "communities", "graph": planted(),
        "options": {"communities": 60, "communityAlgorithm": "leiden"}}));
    assert_eq!(groups["ok"], true, "{groups}");
    assert!(!groups["groups"].as_object().unwrap().is_empty());
}
