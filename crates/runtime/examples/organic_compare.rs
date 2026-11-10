//! Scratch comparison for the organic arrangement: the same graphs with every direction free, under three combinations:
//! organic with straight edges, organic with orthogonal routing, and the hierarchical (plain) layout with orthogonal
//! routing. Writes one JSON row per combination and an SVG per drawing. Visual review and numbers only.
use arqlet_model::{
    DirectionConstraint, EdgeRouting, Graph, LayoutOptions, LayoutStrategy, Snapshot,
    StrategyConstraint,
};
use std::collections::BTreeSet;
use std::time::Instant;

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
        out += &format!(
            r##"<rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" fill="#fff" stroke="#333"/>"##,
            r.x, r.y, r.width, r.height
        );
    }
    for route in &snapshot.routes {
        let pts: Vec<String> = route
            .points
            .iter()
            .map(|p| format!("{:.1},{:.1}", p.x, p.y))
            .collect();
        out += &format!(
            r##"<polyline points="{}" fill="none" stroke="#1565c0" stroke-width="1"/>"##,
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

fn free_variant(graph: &Graph) -> Graph {
    let mut g = graph.clone();
    g.default_direction = DirectionConstraint::Free;
    for d in g.container_directions.values_mut() {
        *d = DirectionConstraint::Free;
    }
    g
}

fn main() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir = std::env::var("ORGANIC_SVG_DIR").ok();
    for file in std::env::args().skip(1) {
        let base: Graph =
            serde_json::from_str(&std::fs::read_to_string(root.join(&file)).unwrap()).unwrap();
        let stem = std::path::Path::new(&file)
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let free = free_variant(&base);
        let mut organic = free.clone();
        organic.default_strategy = StrategyConstraint::Fixed(LayoutStrategy::Organic);
        organic.container_strategies.clear();
        let straight = LayoutOptions {
            edge_routing: Some(EdgeRouting::Straight),
            ..Default::default()
        };
        let cases: Vec<(&str, &Graph, LayoutOptions)> = vec![
            ("organic+straight", &organic, straight),
            ("organic+orthogonal", &organic, LayoutOptions::default()),
            ("hierarchical+orthogonal", &free, LayoutOptions::default()),
        ];
        for (name, graph, options) in cases {
            let t = Instant::now();
            let row = match arqlet_layout::compute(graph, None, &BTreeSet::new(), &options) {
                Ok(s) => {
                    let ms = t.elapsed().as_secs_f64() * 1000.0;
                    let q = serde_json::to_value(arqlet_layout::quality_with(
                        &s,
                        &s,
                        &options.objectives,
                    ))
                    .unwrap();
                    if let Some(dir) = &dir {
                        std::fs::write(
                            format!("{dir}/{stem}-{}.svg", name.replace('+', "-")),
                            svg(graph, &s),
                        )
                        .unwrap();
                    }
                    serde_json::json!({"file": file, "case": name, "ms": ms.round(), "nodes": graph.nodes.len(), "edges": graph.edges.len(),
                        "quality": q, "geometryIssues": s.geometry_issues.len(), "routingIssues": s.routing_issues.len(),
                        "bends": s.routes.iter().map(|r| r.points.len().saturating_sub(2)).sum::<usize>()})
                }
                Err(e) => serde_json::json!({"file": file, "case": name, "refused": e}),
            };
            println!("{row}");
        }
    }
}
