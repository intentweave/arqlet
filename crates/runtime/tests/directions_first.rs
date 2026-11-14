//! Experimental `directionsFirst`: free directions are chosen first, then the drawing is laid out with them fixed, so that
//! single pass builds the edges between areas of different axes instead of delegating them.
use arqlet_runtime::dispatch;
use serde_json::{json, Value};

fn import(graph: &Value, options: Value) -> Value {
    let r: Value = serde_json::from_str(&dispatch(
        &json!({"op": "import", "graph": graph, "limit": 1000, "options": options}).to_string(),
    ))
    .unwrap();
    assert_eq!(r["ok"], true, "{r}");
    r["snapshot"].clone()
}

/// Nested containers whose children chain across them, every area free.
fn nested() -> Value {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut directions = serde_json::Map::new();
    for c in 0..4 {
        let id = format!("c{c}");
        nodes.push(json!({"id": id, "kind": "group", "label": id}));
        directions.insert(id.clone(), json!("auto"));
        for k in 0..5 {
            nodes.push(
                json!({"id": format!("{id}/n{k}"), "kind": "fn", "label": "n", "parentId": id}),
            );
            if k > 0 {
                edges.push(json!({"id": format!("{id}/e{k}"), "kind": "calls", "source": format!("{id}/n{}", k - 1), "target": format!("{id}/n{k}")}));
            }
        }
        if c > 0 {
            edges.push(json!({"id": format!("x{c}"), "kind": "calls", "source": format!("c{}/n4", c - 1), "target": format!("{id}/n0")}));
        }
    }
    json!({"nodes": nodes, "edges": edges, "defaultDirection": "auto", "containerDirections": directions})
}

fn single_pass(snapshot: &Value) -> (u64, u64) {
    let d = snapshot["layoutDiagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|d| d.as_str())
        .find(|d| d.starts_with("Single pass:"))
        .expect("single pass ran")
        .to_string();
    let number = |word: &str| -> u64 {
        d.split(&format!(" {word}"))
            .next()
            .unwrap()
            .rsplit(' ')
            .next()
            .unwrap()
            .parse()
            .unwrap()
    };
    (number("constructed"), number("delegated"))
}

#[test]
fn chosen_directions_are_laid_out_again_fixed_and_single_pass_builds_the_edges() {
    let graph = nested();
    let plain = import(&graph, json!({}));
    let first = import(&graph, json!({"directionsFirst": true}));
    let (built, delegated) = single_pass(&first);
    assert_eq!(delegated, 0, "every edge built ({built})");
    assert!(single_pass(&plain).1 >= delegated);
    // The snapshot keeps the free directions and says which orientations were chosen: one for every free area.
    assert_eq!(first["graph"]["defaultDirection"], "auto");
    assert_eq!(first["graph"]["containerDirections"]["c0"], "auto");
    let chosen = first["areaOrientations"].as_object().unwrap();
    assert_eq!(chosen.len(), 5, "the root and four containers: {chosen:?}");
    assert_eq!(first["options"]["directionsFirst"], true);
    // Never worse than all left to right (one of the candidates), by the published key.
    let mut lr = graph.clone();
    lr["defaultDirection"] = json!("lr");
    lr["containerDirections"] = json!({});
    let fixed_lr: arqlet_model::Snapshot = serde_json::from_value(import(&lr, json!({}))).unwrap();
    let chosen_snapshot: arqlet_model::Snapshot = serde_json::from_value(first.clone()).unwrap();
    assert!(
        arqlet_layout::published_key(&chosen_snapshot).unwrap()
            <= arqlet_layout::published_key(&fixed_lr).unwrap()
    );
    // Fixed directions are untouched by the option.
    let mut fixed = graph.clone();
    fixed["defaultDirection"] = json!("lr");
    fixed["containerDirections"] = json!({});
    assert_eq!(
        import(&fixed, json!({"directionsFirst": true}))["boxes"],
        import(&fixed, json!({}))["boxes"]
    );
}
