//! Scratch diagnosis for the structured comparison: where crossings lie (innermost container), and which edges grow
//! longest in the composition against the joint layout (with the containers each route enters).
use arqlet_model::{Graph, LayoutOptions, Point, Rect, Snapshot};
use arqlet_runtime::hierarchy;
use std::collections::{BTreeMap, BTreeSet};

fn absolute(graph: &Graph, snapshot: &Snapshot) -> BTreeMap<String, Rect> {
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

fn inside(r: &Rect, p: &Point) -> bool {
    p.x > r.x && p.x < r.x + r.width && p.y > r.y && p.y < r.y + r.height
}

fn crossing(a: &Point, b: &Point, c: &Point, d: &Point) -> Option<Point> {
    let den = (b.x - a.x) * (d.y - c.y) - (b.y - a.y) * (d.x - c.x);
    if den.abs() < 1e-9 {
        return None;
    }
    let t = ((c.x - a.x) * (d.y - c.y) - (c.y - a.y) * (d.x - c.x)) / den;
    let u = ((c.x - a.x) * (b.y - a.y) - (c.y - a.y) * (b.x - a.x)) / den;
    // Proper crossings only: both parameters strictly inside their segments.
    (t > 1e-6 && t < 1.0 - 1e-6 && u > 1e-6 && u < 1.0 - 1e-6).then(|| Point {
        x: a.x + t * (b.x - a.x),
        y: a.y + t * (b.y - a.y),
    })
}

fn length(points: &[Point]) -> f64 {
    points
        .windows(2)
        .map(|w| ((w[0].x - w[1].x).powi(2) + (w[0].y - w[1].y).powi(2)).sqrt())
        .sum()
}

fn main() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let options = LayoutOptions::default();
    for file in std::env::args().skip(1) {
        let graph: Graph =
            serde_json::from_str(&std::fs::read_to_string(root.join(&file)).unwrap()).unwrap();
        let h = hierarchy::refine_hierarchical(&graph, &options).unwrap();
        let (joint, composed) = (&h.joint_candidate, &h.composed_candidate);
        println!("== {file}: chosen {}", h.chosen);

        // Containers, innermost-first by area.
        let boxes = absolute(&graph, composed);
        let mut containers: Vec<(String, Rect)> = graph
            .nodes
            .iter()
            .filter(|n| {
                graph
                    .nodes
                    .iter()
                    .any(|c| c.parent_id.as_deref() == Some(&n.id))
            })
            .map(|n| (n.id.clone(), boxes[&n.id].clone()))
            .collect();
        containers.sort_by(|a, b| {
            (a.1.width * a.1.height)
                .partial_cmp(&(b.1.width * b.1.height))
                .unwrap()
        });
        let owner = |p: &Point| {
            containers
                .iter()
                .find(|(_, r)| inside(r, p))
                .map(|(id, _)| id.clone())
                .unwrap_or("root".into())
        };

        // Crossings of the joint layout and of the composition, located by their innermost container.
        for (label, s) in [("joint", joint), ("composed", composed)] {
            let abs = absolute(&graph, s);
            let mut tally: BTreeMap<String, usize> = BTreeMap::new();
            let routes = &s.routes;
            let mut total = 0;
            for i in 0..routes.len() {
                for j in i + 1..routes.len() {
                    for w1 in routes[i].points.windows(2) {
                        for w2 in routes[j].points.windows(2) {
                            if let Some(p) = crossing(&w1[0], &w1[1], &w2[0], &w2[1]) {
                                total += 1;
                                *tally.entry(owner(&p)).or_default() += 1;
                            }
                        }
                    }
                }
            }
            let _ = abs;
            println!(
                "  {label}: crossings {total}, by innermost container {:?}",
                tally
            );
        }

        // Edges that grow most in the composition: their length in both, and the containers each endpoint is in.
        let jl: BTreeMap<&str, f64> = joint
            .routes
            .iter()
            .map(|r| (r.id.as_str(), length(&r.points)))
            .collect();
        let mut growth: Vec<(f64, String, f64, f64)> = composed
            .routes
            .iter()
            .map(|r| {
                let c = length(&r.points);
                let j = jl[r.id.as_str()];
                (c - j, r.id.clone(), j, c)
            })
            .collect();
        growth.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
        let endpoints: BTreeMap<&str, (&str, &str)> = graph
            .edges
            .iter()
            .map(|e| {
                (
                    e.id.as_str(),
                    (e.source.as_str(), e.target.as_deref().unwrap()),
                )
            })
            .collect();
        let parent: BTreeMap<&str, Option<&str>> = graph
            .nodes
            .iter()
            .map(|n| (n.id.as_str(), n.parent_id.as_deref()))
            .collect();
        let chain = |id: &str| {
            let mut out = vec![id.to_string()];
            let mut p = parent[id];
            while let Some(pid) = p {
                out.push(pid.to_string());
                p = parent[pid];
            }
            out
        };
        for (d, id, j, c) in growth.iter().take(6) {
            let (s, t) = endpoints[id.as_str()];
            let from: BTreeSet<String> = chain(s).into_iter().collect();
            let to: BTreeSet<String> = chain(t).into_iter().collect();
            println!("  edge {id}: {s} -> {t}, joint {j:.0} composed {c:.0} (+{d:.0}); containers of endpoints differ: {:?}", from.symmetric_difference(&to).cloned().collect::<Vec<_>>());
        }
        let total_growth: f64 = growth.iter().map(|g| g.0).sum();
        let shrink = growth.iter().filter(|g| g.0 < 0.0).count();
        println!(
            "  total growth {total_growth:.0}; edges that shrink: {shrink} of {}",
            growth.len()
        );
    }
}
