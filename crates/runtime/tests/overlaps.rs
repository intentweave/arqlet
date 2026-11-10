//! Unintended shared edge segments are hard for a published result (ADR 0002, ADR 0005): track separation and a required
//! clearance must hold together, and neither may bring the overlaps back.

use arqlet_model::{Graph, LayoutOptions};
use std::collections::BTreeSet;

/// The pipeline before single pass became the default (ADR 0016), which these tests exercise.
fn legacy() -> LayoutOptions {
    LayoutOptions {
        single_pass_auto: false,
        single_pass: false,
        abstract_routing: false,
        ..LayoutOptions::default()
    }
}

/// Five sources and five targets with every source linked to three targets: many routes must share the corridor
/// between the two layers.
fn graph() -> Graph {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    for i in 0..5 {
        nodes.push(format!(
            r#"{{"id":"s{i}","kind":"node","label":"Source {i}","parentId":null}}"#
        ));
        nodes.push(format!(
            r#"{{"id":"t{i}","kind":"node","label":"Target {i}","parentId":null}}"#
        ));
        for k in 0..3 {
            let j = (i + k) % 5;
            edges.push(format!(
                r#"{{"id":"e{i}_{j}","kind":"uses","source":"s{i}","target":"t{j}"}}"#
            ));
        }
    }
    serde_json::from_str(&format!(
        r#"{{"nodes":[{}],"edges":[{}]}}"#,
        nodes.join(","),
        edges.join(",")
    ))
    .unwrap()
}

fn measured(separation: bool, clearance: Option<f64>) -> arqlet_scene::metrics::Metrics {
    let options = LayoutOptions {
        routing_track_separation: separation,
        required_clearance: clearance,
        ..legacy()
    };
    let computed = arqlet_layout::compute(&graph(), None, &BTreeSet::new(), &options).unwrap();
    let laid =
        arqlet_runtime::refine_snapshot(&computed, computed.revision, computed.layout_version)
            .unwrap();
    if std::env::var("DUMP").is_ok() {
        eprintln!("ISSUES {:?}", laid.routing_issues);
        eprintln!("CONFLICTS {:?}", laid.route_conflicts);
        eprintln!("OPT {:?}", laid.optimization);
        for (id, b) in &laid.boxes {
            eprintln!(
                "BOX {id} {:.0},{:.0} {:.0}x{:.0}",
                b.x, b.y, b.width, b.height
            );
        }
        for r in &laid.routes {
            for w in r.points.windows(2) {
                for (id, b) in &laid.boxes {
                    for step in 0..=20 {
                        let t = f64::from(step) / 20.;
                        let (px, py) = (
                            w[0].x + (w[1].x - w[0].x) * t,
                            w[0].y + (w[1].y - w[0].y) * t,
                        );
                        let d = (b.x - px)
                            .max(0.)
                            .max(px - b.x - b.width)
                            .hypot((b.y - py).max(0.).max(py - b.y - b.height));
                        let first = &r.points[0];
                        let last = &r.points[r.points.len() - 1];
                        let at_port = (px - first.x).hypot(py - first.y) < 14.
                            || (px - last.x).hypot(py - last.y) < 14.;
                        if d < 5.9 && !at_port {
                            eprintln!("NEAR {} to {id}: {d:.1} at {px:.0},{py:.0}", r.id);
                        }
                    }
                }
            }
            let pts: Vec<String> = r
                .points
                .iter()
                .map(|p| format!("{:.0},{:.0}", p.x, p.y))
                .collect();
            eprintln!("ROUTE {} {}", r.id, pts.join(" "));
        }
    }
    arqlet_layout::published_metrics(&laid).unwrap()
}

fn refined(options: &LayoutOptions) -> arqlet_model::Snapshot {
    let computed = arqlet_layout::compute(&graph(), None, &BTreeSet::new(), options).unwrap();
    arqlet_runtime::refine_snapshot(&computed, computed.revision, computed.layout_version).unwrap()
}

/// Corridors too narrow for the routes through them are widened in the placement (space request, algorithm plan A2 and
/// A4a): the same graph keeps its row gaps without a clearance and gets wider ones with it.
#[test]
fn a_corridor_too_narrow_for_its_routes_is_widened() {
    let plain = refined(&LayoutOptions {
        layered_construction: false,
        hierarchical_ordering: false,
        required_clearance: None,
        ..legacy()
    });
    let laid = refined(&LayoutOptions {
        layered_construction: false,
        hierarchical_ordering: false,
        required_clearance: Some(6.),
        ..legacy()
    });
    let height =
        |s: &arqlet_model::Snapshot| s.boxes.values().map(|b| b.y + b.height).fold(0., f64::max);
    // Two routes share the corridor between two rows that is 24 high: a clearance of 6 plus a track of 6 on each side
    // leaves one track, so the second route needs 6 more.
    assert!(
        height(&laid) >= height(&plain) + 6. - 1e-6,
        "the widened corridor makes the drawing taller: {} against {}",
        height(&laid),
        height(&plain)
    );
}

#[test]
fn track_separation_with_a_required_clearance_keeps_edges_apart() {
    for (separation, clearance) in [
        (false, None),
        (true, None),
        (false, Some(6.)),
        (true, Some(6.)),
    ] {
        let metrics = measured(separation, clearance);
        assert_eq!(
            metrics.shared_line_pairs, 0,
            "separation {separation}, clearance {clearance:?}: {} edge pairs share a segment ({} long)",
            metrics.shared_line_pairs, metrics.shared_line_length
        );
        assert_eq!(metrics.line_node_hits, 0, "{separation} {clearance:?}");
        if clearance.is_some() {
            assert_eq!(
                metrics.hug_pairs, 0,
                "separation {separation}, clearance {clearance:?}: {} near-node pairs remain",
                metrics.hug_pairs
            );
        }
    }
}
