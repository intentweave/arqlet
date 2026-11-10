//! Scratch gallery for the tree and radial arrangements: the same asymmetric tree in every direction, radial, with unequal
//! node sizes, and the mixed drawing. Writes one SVG per drawing and an index page. Visual review only.
use arqlet_model::{
    DirectionConstraint, Graph, LayoutOptions, LayoutStrategy, NodeSize, Orientation, Snapshot,
    StrategyConstraint,
};
use std::collections::BTreeSet;

fn svg(graph: &Graph, snapshot: &Snapshot) -> String {
    let parent: std::collections::BTreeMap<&str, Option<&str>> = graph
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.parent_id.as_deref()))
        .collect();
    let absolute = |id: &str| {
        let mut r = snapshot.boxes[id].clone();
        let mut p = parent[id];
        while let Some(pid) = p {
            r.x += snapshot.boxes[pid].x;
            r.y += snapshot.boxes[pid].y;
            p = parent[pid];
        }
        r
    };
    let (mut w, mut h) = (0f64, 0f64);
    let mut out = String::new();
    for n in &graph.nodes {
        let r = absolute(&n.id);
        w = w.max(r.x + r.width);
        h = h.max(r.y + r.height);
        let is_container = graph
            .nodes
            .iter()
            .any(|c| c.parent_id.as_deref() == Some(n.id.as_str()));
        let fill = if is_container { "#f4f4f4" } else { "#ffffff" };
        out += &format!(
            r##"<rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" fill="{fill}" stroke="#333"/><text x="{:.1}" y="{:.1}" font-size="11">{}</text>"##,
            r.x,
            r.y,
            r.width,
            r.height,
            r.x + 4.0,
            r.y + 14.0,
            n.id
        );
    }
    for route in &snapshot.routes {
        let pts: Vec<String> = route
            .points
            .iter()
            .map(|p| format!("{:.1},{:.1}", p.x, p.y))
            .collect();
        out += &format!(
            r##"<polyline points="{}" fill="none" stroke="#1565c0" stroke-width="1.2"/>"##,
            pts.join(" ")
        );
        w = w.max(route.points.iter().map(|p| p.x).fold(0.0, f64::max));
        h = h.max(route.points.iter().map(|p| p.y).fold(0.0, f64::max));
    }
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{:.0}" height="{:.0}" viewBox="0 0 {:.0} {:.0}" style="background:white">{out}</svg>"##,
        w + 20.0,
        h + 20.0,
        w + 20.0,
        h + 20.0
    )
}

fn load(name: &str) -> Graph {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    serde_json::from_str(
        &std::fs::read_to_string(root.join("examples/fixtures/structure").join(name)).unwrap(),
    )
    .unwrap()
}

fn main() {
    let dir = std::env::args().nth(1).expect("output directory");
    let mut cases: Vec<(String, Graph, LayoutOptions)> = Vec::new();
    for (name, orientation) in [
        ("lr", Orientation::Lr),
        ("rl", Orientation::Rl),
        ("tb", Orientation::Tb),
        ("bt", Orientation::Bt),
    ] {
        let mut g = load("asymmetric-tree.json");
        g.default_direction = DirectionConstraint::Fixed(orientation);
        g.default_strategy = StrategyConstraint::Fixed(LayoutStrategy::Tree);
        cases.push((format!("asym-tree-{name}"), g, LayoutOptions::default()));
    }
    let mut radial = load("asymmetric-tree.json");
    radial.default_direction = DirectionConstraint::Free;
    radial.default_strategy = StrategyConstraint::Fixed(LayoutStrategy::Radial);
    cases.push((
        "asym-radial".into(),
        radial.clone(),
        LayoutOptions::default(),
    ));
    let mut sizes = LayoutOptions::default();
    for (id, w, h) in [("a", 320., 140.), ("c", 70., 150.), ("e", 240., 60.)] {
        sizes.node_sizes.insert(
            id.into(),
            NodeSize {
                width: w,
                height: h,
            },
        );
    }
    let mut tree_unequal = load("asymmetric-tree.json");
    tree_unequal.default_direction = DirectionConstraint::Fixed(Orientation::Tb);
    tree_unequal.default_strategy = StrategyConstraint::Fixed(LayoutStrategy::Tree);
    cases.push(("asym-tree-tb-unequal".into(), tree_unequal, sizes.clone()));
    cases.push(("asym-radial-unequal".into(), radial, sizes));
    let mixed = load("mixed-tree-radial.json");
    cases.push(("mixed-tree-radial".into(), mixed, LayoutOptions::default()));

    let mut index = String::from("<!doctype html><html><head><meta charset='utf-8'><title>Tree and radial</title><style>body{font-family:system-ui;margin:16px}figure{display:inline-block;margin:8px;vertical-align:top;border:1px solid #ccc;background:white;padding:6px}figcaption{font-size:13px;color:#333}img{max-width:560px;border:1px solid #eee}</style></head><body><h1>Tree and radial arrangements (visual review)</h1><p>Each drawing is computed by the default layout (no search) and written as SVG. Check edges, ports and spacing by eye.</p>");
    for (name, graph, options) in cases {
        let snapshot = arqlet_layout::compute(&graph, None, &BTreeSet::new(), &options);
        match snapshot {
            Ok(s) => {
                let file = format!("{dir}/{name}.svg");
                std::fs::write(&file, svg(&graph, &s)).unwrap();
                index += &format!("<figure><figcaption>{name}: {} routes, {} geometry issues</figcaption><img src='{name}.svg'></figure>", s.routes.len(), s.geometry_issues.len());
            }
            Err(e) => {
                index += &format!(
                    "<figure><figcaption>{name}: refused: {}</figcaption></figure>",
                    e
                )
            }
        }
    }
    index += "</body></html>";
    std::fs::write(format!("{dir}/index.html"), index).unwrap();
    println!("written to {dir}");
}
