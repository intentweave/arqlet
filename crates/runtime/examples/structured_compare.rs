//! Comparison and drawing checks: the joint layout, the raw composition and the chosen winner, for each diagram.
//! Same options and one thread for every candidate. Prints one JSON row per diagram. With COMPARE_SVG_DIR set, writes an
//! SVG of each candidate. Scratch tooling for `benchmarks/results/structured-compare-*.md`.
use arqlet_model::{Graph, LayoutOptions, Point, Rect, Snapshot};
use arqlet_runtime::{coordinator, hierarchy, refine_snapshot_logged};
use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

/// Absolute rectangle of every node: boxes are parent-relative, so each ancestor's offset is added.
fn absolute_boxes(graph: &Graph, snapshot: &Snapshot) -> BTreeMap<String, Rect> {
    let parent: BTreeMap<&str, Option<&str>> = graph
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.parent_id.as_deref()))
        .collect();
    graph
        .nodes
        .iter()
        .map(|n| {
            let mut r = snapshot.boxes[&n.id].clone();
            let mut p = parent[n.id.as_str()];
            while let Some(pid) = p {
                r.x += snapshot.boxes[pid].x;
                r.y += snapshot.boxes[pid].y;
                p = parent[pid];
            }
            (n.id.clone(), r)
        })
        .collect()
}

fn border_distance(p: &Point, r: &Rect) -> f64 {
    let ox = (r.x - p.x).max(0.).max(p.x - r.x - r.width);
    let oy = (r.y - p.y).max(0.).max(p.y - r.y - r.height);
    if ox == 0. && oy == 0. {
        // Inside: distance to the nearest side.
        let dx = (p.x - r.x).abs().min((p.x - r.x - r.width).abs());
        let dy = (p.y - r.y).abs().min((p.y - r.y - r.height).abs());
        dx.min(dy)
    } else {
        (ox * ox + oy * oy).sqrt()
    }
}

/// Segments of a route as (from, to) pairs.
fn segments(points: &[Point]) -> Vec<(Point, Point)> {
    points
        .windows(2)
        .map(|w| (w[0].clone(), w[1].clone()))
        .collect()
}

fn length(points: &[Point]) -> f64 {
    segments(points)
        .iter()
        .map(|(a, b)| ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt())
        .sum()
}

/// Overlapping collinear stretches between two routes: returns the coordinate of each overlap (the line it lies on,
/// horizontal or vertical) and whether it is horizontal.
fn overlaps(a: &[Point], b: &[Point]) -> Vec<(bool, f64)> {
    let mut found = Vec::new();
    for (p, q) in segments(a) {
        for (r, s) in segments(b) {
            if (p.y - q.y).abs() < 1e-6 && (r.y - s.y).abs() < 1e-6 && (p.y - r.y).abs() <= 1.0 {
                let overlap = p.x.max(q.x).min(r.x.max(s.x)) - p.x.min(q.x).max(r.x.min(s.x));
                if overlap > 1.0 {
                    found.push((true, (p.y + r.y) / 2.));
                }
            }
            if (p.x - q.x).abs() < 1e-6 && (r.x - s.x).abs() < 1e-6 && (p.x - r.x).abs() <= 1.0 {
                let overlap = p.y.max(q.y).min(r.y.max(s.y)) - p.y.min(q.y).max(r.y.min(s.y));
                if overlap > 1.0 {
                    found.push((false, (p.x + r.x) / 2.));
                }
            }
        }
    }
    found
}

/// Independent checks of a drawing, recomputed from its geometry (not from the report).
fn check(graph: &Graph, snapshot: &Snapshot) -> serde_json::Value {
    let boxes = absolute_boxes(graph, snapshot);
    let route_of: BTreeMap<&str, &arqlet_model::Route> =
        snapshot.routes.iter().map(|r| (r.id.as_str(), r)).collect();
    let mut missing = Vec::new();
    let mut endpoint_off = Vec::new();
    let mut total = 0.;
    let (mut longest, mut longest_id) = (0., String::new());
    for edge in &graph.edges {
        let Some(route) = route_of.get(edge.id.as_str()) else {
            missing.push(edge.id.clone());
            continue;
        };
        let target = edge.target.as_deref().unwrap();
        let first = route.points.first().unwrap();
        let last = route.points.last().unwrap();
        if border_distance(first, &boxes[&edge.source]) > 1.0 {
            endpoint_off.push(format!("{}:start", edge.id));
        }
        if border_distance(last, &boxes[target]) > 1.0 {
            endpoint_off.push(format!("{}:end", edge.id));
        }
        let l = length(&route.points);
        total += l;
        if l > longest {
            longest = l;
            longest_id = edge.id.clone();
        }
    }
    // Containers, from the absolute boxes: children must lie inside their parent's rectangle.
    let mut outside = Vec::new();
    let mut container_frames = Vec::new();
    for node in &graph.nodes {
        if let Some(pid) = &node.parent_id {
            let c = &boxes[&node.id];
            let p = &boxes[pid];
            if c.x < p.x
                || c.y < p.y
                || c.x + c.width > p.x + p.width
                || c.y + c.height > p.y + p.height
            {
                outside.push(node.id.clone());
            }
        }
        if graph
            .nodes
            .iter()
            .any(|c| c.parent_id.as_deref() == Some(&node.id))
        {
            container_frames.push(boxes[&node.id].clone());
        }
    }
    // Shared stretches: count route pairs, and how many of their lines lie on a container frame (within 2 units).
    let ids: Vec<&str> = graph.edges.iter().map(|e| e.id.as_str()).collect();
    let (mut pairs, mut on_frame, mut stretches) = (0usize, 0usize, 0usize);
    for i in 0..ids.len() {
        for j in i + 1..ids.len() {
            let (Some(a), Some(b)) = (route_of.get(ids[i]), route_of.get(ids[j])) else {
                continue;
            };
            let found = overlaps(&a.points, &b.points);
            if found.is_empty() {
                continue;
            }
            pairs += 1;
            stretches += found.len();
            for (horizontal, at) in found {
                let hit = container_frames.iter().any(|f| {
                    if horizontal {
                        (at - f.y).abs() <= 2. || (at - (f.y + f.height)).abs() <= 2.
                    } else {
                        (at - f.x).abs() <= 2. || (at - (f.x + f.width)).abs() <= 2.
                    }
                });
                if hit {
                    on_frame += 1;
                }
            }
        }
    }
    serde_json::json!({
        "routesFor": snapshot.routes.len(),
        "edges": graph.edges.len(),
        "missingRoutes": missing,
        "endpointsOffBorder": endpoint_off,
        "outsideParent": outside,
        "totalLength": total.round(),
        "longestRoute": {"id": longest_id, "length": longest.round()},
        "sharedRoutePairs": pairs,
        "sharedStretches": stretches,
        "sharedStretchesOnContainerFrame": on_frame,
    })
}

/// The report of a snapshot, measured against the joint baseline with the same objectives.
fn quality(snapshot: &Snapshot, baseline: &Snapshot, options: &LayoutOptions) -> serde_json::Value {
    serde_json::to_value(arqlet_layout::quality_with(
        snapshot,
        baseline,
        &options.objectives,
    ))
    .unwrap()
}

fn svg(graph: &Graph, snapshot: &Snapshot) -> String {
    let boxes = absolute_boxes(graph, snapshot);
    let (mut w, mut h) = (0f64, 0f64);
    let mut out = String::new();
    for n in &graph.nodes {
        let r = &boxes[&n.id];
        w = w.max(r.x + r.width);
        h = h.max(r.y + r.height);
        let is_container = graph
            .nodes
            .iter()
            .any(|c| c.parent_id.as_deref() == Some(&n.id));
        let fill = if is_container { "#f4f4f4" } else { "#ffffff" };
        out += &format!(
            r##"<rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" fill="{fill}" stroke="#333"/><text x="{:.1}" y="{:.1}" font-size="9">{}</text>"##,
            r.x,
            r.y,
            r.width,
            r.height,
            r.x + 3.0,
            r.y + 11.0,
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

fn main() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let svg_dir = std::env::var("COMPARE_SVG_DIR").ok();
    let options = LayoutOptions::default();
    for file in std::env::args().skip(1) {
        let mut graph: Graph =
            serde_json::from_str(&std::fs::read_to_string(root.join(&file)).unwrap()).unwrap();
        // Regime: the authored directions (default), or every container set free (`auto`) or to the allowed set [lr, tb],
        // with the root unchanged. Joint and structured both use the same resolved constraints.
        let regime = std::env::var("REGIME").unwrap_or_else(|_| "authored".into());
        let containers: std::collections::BTreeSet<String> = graph
            .nodes
            .iter()
            .filter_map(|n| n.parent_id.clone())
            .collect();
        for id in &containers {
            match regime.as_str() {
                "auto" => {
                    graph
                        .container_directions
                        .insert(id.clone(), arqlet_model::DirectionConstraint::Free);
                }
                "allowed" => {
                    graph.container_directions.insert(
                        id.clone(),
                        arqlet_model::DirectionConstraint::Allowed(vec![
                            arqlet_model::Orientation::Lr,
                            arqlet_model::Orientation::Tb,
                        ]),
                    );
                }
                _ => {}
            }
        }
        let stem = std::path::Path::new(&file)
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let stem = format!("{stem}-{regime}");

        let t = Instant::now();
        let laid = arqlet_layout::compute(&graph, None, &BTreeSet::new(), &options).unwrap();
        let (joint, _, _) =
            refine_snapshot_logged(&laid, laid.revision, laid.layout_version, 1, None).unwrap();
        let joint_ms = t.elapsed().as_secs_f64() * 1000.0;

        let mut candidates = serde_json::Map::new();
        candidates.insert("joint".into(), serde_json::json!({"quality": quality(&joint, &joint, &options), "check": check(&graph, &joint), "key": arqlet_layout::published_key(&joint).ok()}));
        let mut row = serde_json::json!({"file": file, "nodes": graph.nodes.len(), "edges": graph.edges.len(), "jointMs": joint_ms.round()});

        if graph.nodes.iter().any(|n| n.parent_id.is_some()) {
            let t = Instant::now();
            let h = hierarchy::refine_hierarchical(&graph, &options).unwrap();
            let hierarchy_ms = t.elapsed().as_secs_f64() * 1000.0;
            candidates.insert("baseline".into(), serde_json::json!({"quality": quality(&h.baseline_candidate, &joint, &options), "check": check(&graph, &h.baseline_candidate), "key": h.baseline_key}));
            candidates.insert("composed".into(), serde_json::json!({"quality": quality(&h.composed_candidate, &joint, &options), "check": check(&graph, &h.composed_candidate), "key": h.composed_key}));
            candidates.insert("winner".into(), serde_json::json!({"route": h.chosen, "quality": quality(&h.snapshot, &joint, &options), "check": check(&graph, &h.snapshot), "key": arqlet_layout::published_key(&h.snapshot).ok()}));
            row["hierarchyMs"] = serde_json::json!(hierarchy_ms.round());
            row["shapes"] = serde_json::json!(h.shapes);
            row["compositions"] = serde_json::json!(h.compositions);
            if let Some(dir) = &svg_dir {
                std::fs::write(
                    format!("{dir}/{stem}-baseline.svg"),
                    svg(&graph, &h.baseline_candidate),
                )
                .unwrap();
                std::fs::write(
                    format!("{dir}/{stem}-composed.svg"),
                    svg(&graph, &h.composed_candidate),
                )
                .unwrap();
                std::fs::write(format!("{dir}/{stem}-winner.svg"), svg(&graph, &h.snapshot))
                    .unwrap();
            }
        } else if let Some(dir) = &svg_dir {
            std::fs::write(format!("{dir}/{stem}-joint.svg"), svg(&graph, &joint)).unwrap();
        }
        if let Some(dir) = &svg_dir {
            std::fs::write(format!("{dir}/{stem}-joint.svg"), svg(&graph, &joint)).unwrap();
        }
        row["regime"] = serde_json::json!(regime);
        row["candidates"] = serde_json::Value::Object(candidates);
        println!("{row}");
    }
}
