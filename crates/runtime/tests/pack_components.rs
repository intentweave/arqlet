//! Experimental `packComponents`: the components of a layered area packed in rank space towards an aspect ratio. The
//! drawing stays valid (no overlaps, every edge routed in its direction, no line through a node), and the option off
//! leaves everything as it was.
use arqlet_model::Snapshot;
use arqlet_runtime::dispatch;
use serde_json::{json, Value};

fn import(graph: &Value, options: Value) -> Snapshot {
    let r: Value = serde_json::from_str(&dispatch(
        &json!({"op": "import", "graph": graph, "limit": graph["nodes"].as_array().unwrap().len(), "options": options})
            .to_string(),
    ))
    .unwrap();
    assert_eq!(r["ok"], true, "{r}");
    serde_json::from_value(r["snapshot"].clone()).unwrap()
}

/// `count` small trees of `size` nodes (each node below a deterministic earlier one), `lone` nodes without edges, all
/// inside `parent` if given.
fn forest(count: usize, size: usize, lone: usize, parent: Option<&str>) -> Value {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    if let Some(p) = parent {
        nodes.push(json!({"id": p, "kind": "package", "label": p}));
    }
    let mut seed = 11u64;
    for c in 0..count {
        for k in 0..size {
            let id = format!("c{c}.{k}");
            nodes.push(json!({"id": id, "kind": "fn", "label": id, "parentId": parent}));
            if k > 0 {
                seed = seed
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                let up = (seed >> 33) as usize % k;
                edges.push(json!({"id": format!("{id}-e"), "kind": "calls", "source": format!("c{c}.{up}"), "target": id}));
            }
        }
    }
    for k in 0..lone {
        nodes.push(
            json!({"id": format!("lone{k}"), "kind": "fn", "label": "lone", "parentId": parent}),
        );
    }
    json!({"nodes": nodes, "edges": edges})
}

/// Width over height of the drawing, its metrics, and the geometry issues.
fn measure(s: &Snapshot) -> (f64, arqlet_scene::metrics::Metrics, usize) {
    let m = arqlet_layout::published_metrics(s).unwrap();
    let issues =
        arqlet_scene::validate_geometry(&s.graph, &s.boxes, arqlet_scene::GeometryRules::default());
    (m.width / m.height, m, issues.len())
}

fn valid(s: &Snapshot, graph: &Value) {
    let (_, m, issues) = measure(s);
    assert_eq!(issues, 0, "geometry");
    assert_eq!(m.line_node_hits, 0, "no line through a node");
    // Every edge is routed, from its source to its target.
    let edges = graph["edges"].as_array().unwrap();
    assert_eq!(s.routes.len(), edges.len());
    for e in edges {
        let r = s.routes.iter().find(|r| r.id == e["id"]).expect("routed");
        assert!(r.points.len() >= 2);
    }
}

#[test]
fn a_forest_is_packed_about_square_without_losing_quality() {
    let graph = forest(20, 10, 15, None);
    let plain = import(&graph, json!({}));
    let packed = import(&graph, json!({"packComponents": {}}));
    valid(&packed, &graph);
    let (a0, m0, _) = measure(&plain);
    let (a1, m1, _) = measure(&packed);
    assert!(a0 < 0.3, "the default is one tall column: {a0}");
    assert!((0.6..1.6).contains(&a1), "about square: {a1}");
    assert!(
        m1.area <= m0.area * 1.05,
        "not larger: {} vs {}",
        m1.area,
        m0.area
    );
    // No more crossings; bends may change a little with the new neighbours (here 204 against 198).
    assert!(m1.crossings <= m0.crossings, "{m1:?} vs {m0:?}");
    assert!(
        m1.bends as f64 <= m0.bends as f64 * 1.05,
        "{} vs {}",
        m1.bends,
        m0.bends
    );
    assert_eq!(packed.options.pack_components, Some(Default::default()));
}

#[test]
fn the_aspect_and_the_flow_direction_are_respected() {
    let graph = forest(20, 10, 15, None);
    let wide = import(&graph, json!({"packComponents": {"aspect": 2.0}}));
    valid(&wide, &graph);
    let (a, _, _) = measure(&wide);
    assert!(a > 1.3, "wider: {a}");
    // A top-to-bottom flow: still about square on the drawing.
    let mut down = graph.clone();
    down["defaultDirection"] = json!("tb");
    let packed = import(&down, json!({"packComponents": {}}));
    valid(&packed, &down);
    let (a, _, _) = measure(&packed);
    assert!((0.6..1.6).contains(&a), "about square top to bottom: {a}");
}

#[test]
fn a_container_with_edges_to_the_outside_packs_its_content() {
    let mut graph = forest(30, 6, 10, Some("box"));
    graph["nodes"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id": "outside", "kind": "fn", "label": "outside"}));
    for c in (0..30).step_by(7) {
        graph["edges"].as_array_mut().unwrap().push(
            json!({"id": format!("out{c}"), "kind": "calls", "source": format!("c{c}.0"), "target": "outside"}),
        );
    }
    let packed = import(&graph, json!({"packComponents": {}}));
    valid(&packed, &graph);
    let b = &packed.boxes["box"];
    assert!(
        (0.5..2.0).contains(&(b.width / b.height)),
        "{} × {}",
        b.width,
        b.height
    );
    // Collapsing and expanding keeps working on the packed view.
    let s = serde_json::to_value(&packed).unwrap();
    let toggled: Value = serde_json::from_str(&dispatch(
        &json!({"op": "toggle", "snapshot": s, "nodeId": "box", "expectedRevision": s["revision"],
            "expectedLayoutVersion": s["layoutVersion"]})
        .to_string(),
    ))
    .unwrap();
    assert_eq!(toggled["ok"], true, "{toggled}");
}

#[test]
fn one_component_or_the_option_off_changes_nothing() {
    let single = forest(1, 30, 0, None);
    let plain = import(&single, json!({}));
    let packed = import(&single, json!({"packComponents": {}}));
    assert_eq!(
        serde_json::to_value(&plain.boxes).unwrap(),
        serde_json::to_value(&packed.boxes).unwrap(),
        "one component: as before"
    );
    let graph = forest(5, 4, 3, None);
    let off = import(&graph, json!({}));
    assert!(off.options.pack_components.is_none());
    assert!(!serde_json::to_string(&off.options)
        .unwrap()
        .contains("packComponents"));
    // Bad values are refused.
    let bad: Value = serde_json::from_str(&dispatch(
        &json!({"op": "import", "graph": graph, "limit": 30, "options": {"packComponents": {"ratio": 2}}}).to_string(),
    ))
    .unwrap();
    assert_eq!(bad["ok"], false);
}
