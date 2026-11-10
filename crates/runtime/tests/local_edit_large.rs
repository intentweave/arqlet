//! An edit on the 599-node model of the repository's own code: the local edit against a complete layout with the same options.
//! Reads a generated dataset (`python3 benchmarks/generate.py`), so it is ignored by default:
//!   cargo test --profile fast -p arqlet-runtime --test local_edit_large -- --ignored --nocapture
//! Environment: LW_LIMIT (view limit, default 1000), LW_EDIT (`size`, the default, or `pin`), LW_GROW (units added to the
//! width of the edited leaf, default 60; a large value forces a corridor to be widened), LW_DEBUG_LOCAL (work of the local
//! routing).
use arqlet_model::{Graph, LayoutOptions, NodeSize, Point, Snapshot};
use arqlet_runtime::{configure_snapshot, import_graph, import_graph_with};
use std::time::Instant;

fn counts(s: &Snapshot) -> (usize, usize, usize, usize) {
    let m = arqlet_scene::metrics::measure(&arqlet_layout::published_drawing(s)).unwrap();
    (
        m.line_node_hits,
        m.container_traversals,
        m.sibling_overlaps,
        m.cross_branch_overlaps,
    )
}

fn hard(s: &Snapshot) -> usize {
    let c = counts(s);
    c.0 + c.1 + c.2 + c.3
}

fn same_routes(a: &Snapshot, b: &Snapshot) -> usize {
    a.routes
        .iter()
        .filter(|r| {
            b.routes.iter().find(|o| o.id == r.id).is_some_and(|o| {
                serde_json::to_string(&o.points).unwrap()
                    == serde_json::to_string(&r.points).unwrap()
            })
        })
        .count()
}

#[test]
#[ignore]
fn an_edit_on_the_code_model_is_local_and_valid() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../benchmarks/datasets/layerweave-code.json");
    let Ok(text) = std::fs::read_to_string(path) else {
        eprintln!("no dataset: run python3 benchmarks/generate.py");
        return;
    };
    let graph: Graph = serde_json::from_str(&text).unwrap();
    let limit: usize = std::env::var("LW_LIMIT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1000);
    let start = Instant::now();
    let base = import_graph(graph.clone(), limit).unwrap();
    println!(
        "import {:?}: {} nodes, {} edges",
        start.elapsed(),
        base.graph.nodes.len(),
        base.graph.edges.len()
    );
    let leaf = base
        .graph
        .nodes
        .iter()
        .find(|n| {
            n.parent_id.is_some()
                && !base
                    .graph
                    .nodes
                    .iter()
                    .any(|c| c.parent_id.as_deref() == Some(n.id.as_str()))
        })
        .unwrap()
        .id
        .clone();
    let at = base.boxes[&leaf].clone();
    // A pin on this model makes the layout itself far worse, with or without the local path (see the findings in
    // benchmarks/results/interaction-20261027/README.md); the default edit is a requested size.
    let pin_edit = std::env::var("LW_EDIT").is_ok_and(|v| v == "pin");
    let pin = Point {
        x: at.x + 40.,
        y: at.y + 10.,
    };
    let size = NodeSize {
        width: at.width
            + std::env::var("LW_GROW")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(60.),
        height: at.height,
    };
    let patch = if pin_edit {
        serde_json::json!({"pins": {leaf.clone(): pin}})
    } else {
        serde_json::json!({"nodeSizes": {leaf.clone(): size}})
    };
    let t = Instant::now();
    let local = configure_snapshot(&base, base.revision, base.layout_version, &patch).unwrap();
    let local_time = t.elapsed();

    let mut options = LayoutOptions::default();
    if pin_edit {
        options.pins.insert(leaf.clone(), pin);
    } else {
        options.node_sizes.insert(leaf.clone(), size);
    }
    let t = Instant::now();
    let complete = import_graph_with(graph, limit, &options).unwrap();
    let complete_time = t.elapsed();

    println!(
        "local edit {local_time:?}, areas laid out again {}",
        local.recomputed_containers.len()
    );
    println!(
        "complete layout {complete_time:?}, areas {}",
        complete.recomputed_containers.len()
    );
    for (name, s) in [("base", &base), ("local", &local), ("complete", &complete)] {
        let c = counts(s);
        println!(
            "{name}: node hits {} container traversals {} sibling overlaps {} cross-branch overlaps {}",
            c.0, c.1, c.2, c.3
        );
    }
    // How many distinct grid lines the nodes give the repair router: it gives up above 120,000 cells.
    for (name, s) in [("base", &base), ("local", &local), ("complete", &complete)] {
        let rects = arqlet_layout::absolute_all(&s.graph, &s.boxes);
        let mut xs: Vec<i64> = Vec::new();
        let mut ys: Vec<i64> = Vec::new();
        for r in rects.values() {
            xs.extend([(r.x * 100.) as i64, ((r.x + r.width) * 100.) as i64]);
            ys.extend([(r.y * 100.) as i64, ((r.y + r.height) * 100.) as i64]);
        }
        xs.sort();
        xs.dedup();
        ys.sort();
        ys.dedup();
        println!("{name}: {} x lines, {} y lines", xs.len(), ys.len());
    }
    println!(
        "routes unchanged from the base: local {} of {}, complete {}",
        same_routes(&local, &base),
        local.routes.len(),
        same_routes(&complete, &base)
    );
    assert!(hard(&local) <= hard(&complete).max(hard(&base)));
}

#[test]
#[ignore]
fn adding_a_node_to_the_code_model_reports_its_work() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../benchmarks/datasets/layerweave-code.json");
    let Ok(text) = std::fs::read_to_string(path) else {
        eprintln!("no dataset: run python3 benchmarks/generate.py");
        return;
    };
    let graph: Graph = serde_json::from_str(&text).unwrap();
    let base = import_graph(graph, 1000).unwrap();
    let source = base.graph.edges.first().unwrap().source.clone();
    let t = Instant::now();
    let added = arqlet_runtime::add_node(&base, base.revision, &source, "Added").unwrap();
    println!(
        "add node {:?}, areas {}, report {:?}",
        t.elapsed(),
        added.recomputed_containers.len(),
        added.edit_report
    );
}

#[test]
#[ignore]
fn a_drag_step_that_leaves_routes_through_nodes_is_a_preview_and_the_end_decides() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../benchmarks/datasets/layerweave-code.json");
    let Ok(text) = std::fs::read_to_string(path) else {
        eprintln!("no dataset: run python3 benchmarks/generate.py");
        return;
    };
    let graph: Graph = serde_json::from_str(&text).unwrap();
    let mut base = import_graph(graph, 1000).unwrap();
    let leaf = base
        .graph
        .nodes
        .iter()
        .find(|n| {
            n.parent_id.is_some()
                && !base
                    .graph
                    .nodes
                    .iter()
                    .any(|c| c.parent_id.as_deref() == Some(n.id.as_str()))
        })
        .unwrap()
        .id
        .clone();
    let at = base.boxes[&leaf].clone();
    // The same edit that forced the fallback (a leaf 1500 wider), once as a step of a drag and once as an ordinary edit.
    let patch = serde_json::json!({"nodeSizes": {leaf.clone(): {"width": at.width + 1500., "height": at.height}}});
    let ordinary = configure_snapshot(&base, base.revision, base.layout_version, &patch).unwrap();
    base.gesture = Some(arqlet_model::Gesture {
        id: "g".into(),
        seq: 1,
        node: leaf.clone(),
        active: true,
        previous: None,
        previous_others: Default::default(),
        ending: false,
        origin: None,
    });
    let step = configure_snapshot(&base, base.revision, base.layout_version, &patch).unwrap();
    let a = ordinary.edit_report.clone().unwrap();
    let b = step.edit_report.clone().unwrap();
    println!("ordinary: {a:?}\nstep of a drag: {b:?}");
    assert!(a.complete_fallback && !a.preview, "{a:?}");
    assert!(b.preview && !b.complete_fallback, "{b:?}");
    assert!(step
        .routing_issues
        .iter()
        .any(|i| i.starts_with("Preview:")));
}
