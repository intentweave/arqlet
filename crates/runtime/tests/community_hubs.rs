//! Hubs left out of Louvain (provisional `communityHubs` / overview option `hubs`): a node linked to everything stays a
//! direct child of its container instead of pulling others into one community named after it.
use arqlet_runtime::dispatch;
use serde_json::{json, Value};

fn call(request: Value) -> Value {
    serde_json::from_str(&dispatch(&request.to_string())).unwrap()
}

/// Ten rings of twenty nodes, and a hub `h` linked both ways to every node.
fn hub_and_rings() -> Value {
    let mut nodes = vec![json!({"id": "h", "kind": "fn", "label": "hub"})];
    let mut edges = Vec::new();
    for i in 0..200 {
        let ring = i / 20;
        let id = format!("n{i:03}");
        nodes.push(json!({"id": id, "kind": "fn", "label": format!("n{i}")}));
        for (k, step) in [("r", 1), ("x", 7)] {
            edges.push(
                json!({"id": format!("{k}{i}"), "kind": "calls", "source": id,
                "target": format!("n{:03}", ring * 20 + (i + step) % 20)}),
            );
        }
        edges.push(json!({"id": format!("h{i}"), "kind": "calls", "source": id, "target": "h"}));
        edges.push(json!({"id": format!("g{i}"), "kind": "calls", "source": "h", "target": id}));
    }
    json!({"nodes": nodes, "edges": edges})
}

/// The overview's top clusters below the root: (label, number of graph nodes below).
fn overview_clusters(options: Value) -> (Value, Vec<(String, usize)>) {
    let r = call(json!({"op": "overview", "graph": hub_and_rings(), "options": options}));
    assert_eq!(r["ok"], true, "{r}");
    let o = r["overview"].clone();
    let parent: Vec<i64> = o["parent"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p.as_i64().unwrap())
        .collect();
    let mut count = std::collections::BTreeMap::new();
    for i in 0..201 {
        let mut top = parent[i];
        while top >= 0 && parent[top as usize] >= 0 {
            top = parent[top as usize];
        }
        *count.entry(top).or_insert(0) += 1;
    }
    let clusters = count
        .into_iter()
        .filter(|(top, _)| *top >= 0)
        .map(|(top, n)| (o["labels"][top as usize].as_str().unwrap().to_string(), n))
        .collect();
    (o, clusters)
}

#[test]
fn a_hub_left_out_of_louvain_stays_alone_and_names_no_community() {
    // Without hubs the hub joins a community, which is named after it and is twice a ring.
    let (_, plain) = overview_clusters(json!({"communities": 120}));
    assert!(
        plain.iter().any(|(label, n)| label == "≈ hub" && *n > 20),
        "{plain:?}"
    );
    for hubs in [json!({"nodes": ["h"]}), json!({"minLinks": 100})] {
        let (o, clusters) = overview_clusters(json!({"communities": 120, "hubs": hubs}));
        let hub = o["ids"]
            .as_array()
            .unwrap()
            .iter()
            .position(|id| id == "h")
            .unwrap();
        assert_eq!(o["parent"][hub], -1, "the hub stays at the top ({hubs})");
        assert_eq!(clusters.len(), 10, "{clusters:?}");
        assert!(
            clusters
                .iter()
                .all(|(label, n)| *n == 20 && !label.contains("hub")),
            "{clusters:?}"
        );
    }
    // A low threshold makes every ring node a hub: no communities are left to find.
    let (_, none) = overview_clusters(json!({"communities": 120, "hubs": {"minLinks": 1}}));
    assert!(none.is_empty(), "{none:?}");
    // Bad values are refused.
    let bad = call(
        json!({"op": "overview", "graph": hub_and_rings(), "options": {"hubs": {"minLinks": -1}}}),
    );
    assert_eq!(bad["ok"], false);
    let unknown = call(
        json!({"op": "overview", "graph": hub_and_rings(), "options": {"hubs": {"degree": 3}}}),
    );
    assert_eq!(unknown["ok"], false);
}

#[test]
fn the_layered_view_leaves_the_hub_out_too() {
    let import = |options: Value| {
        let r = call(
            json!({"op": "import", "graph": hub_and_rings(), "limit": 201, "options": options}),
        );
        assert_eq!(r["ok"], true, "{r}");
        r["snapshot"].clone()
    };
    let labels = |s: &Value| -> Vec<String> {
        s["graph"]["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n["label"].as_str().unwrap().to_string())
            .collect()
    };
    let plain = import(json!({"communities": 120}));
    assert!(
        labels(&plain).iter().any(|l| l.starts_with("hub +")),
        "{:?}",
        labels(&plain)
    );
    let s = import(json!({"communities": 120, "communityHubs": {"nodes": ["h"]}}));
    assert_eq!(s["options"]["communityHubs"], json!({"nodes": ["h"]}));
    let shown = labels(&s);
    assert!(
        shown.contains(&"hub".to_string()),
        "the hub is shown as itself: {shown:?}"
    );
    assert!(shown.iter().all(|l| !l.starts_with("hub +")), "{shown:?}");
    // The first view: the hub and the ten ring communities, collapsed.
    assert_eq!(shown.len(), 11, "{shown:?}");
    // Omitted when empty: the default options do not change.
    assert!(plain["options"].get("communityHubs").is_none());
}
