//! Experimental `singlePassReversed` (ADR 0020, S1-0): a graph that flows against its axis everywhere is laid out with the
//! positive flow under single pass and mirrored.
use arqlet_model::Snapshot;
use arqlet_runtime::dispatch;
use serde_json::{json, Value};

fn call(request: Value) -> Value {
    serde_json::from_str(&dispatch(&request.to_string())).unwrap()
}

/// Two containers of chained nodes, with edges across them, every area flowing `direction`.
fn chains(direction: &str) -> Value {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut directions = serde_json::Map::new();
    for c in 0..2 {
        let id = format!("c{c}");
        nodes.push(json!({"id": id, "kind": "group", "label": id}));
        directions.insert(id.clone(), json!(direction));
        for k in 0..4 {
            nodes.push(json!({"id": format!("{id}/n{k}"), "kind": "fn", "label": format!("n{k}"), "parentId": id}));
            if k > 0 {
                edges.push(json!({"id": format!("{id}/e{k}"), "kind": "calls", "source": format!("{id}/n{}", k - 1), "target": format!("{id}/n{k}")}));
            }
        }
    }
    edges.push(json!({"id": "x", "kind": "calls", "source": "c0/n3", "target": "c1/n0"}));
    json!({"nodes": nodes, "edges": edges, "defaultDirection": direction, "containerDirections": directions})
}

fn import(graph: &Value, options: Value) -> Snapshot {
    let r = call(json!({"op": "import", "graph": graph, "limit": 100, "options": options}));
    assert_eq!(r["ok"], true, "{r}");
    serde_json::from_value(r["snapshot"].clone()).unwrap()
}

#[test]
fn a_reversed_drawing_is_the_mirror_of_the_positive_one_and_flows_backwards() {
    // Right to left only: `bt` is not mirrored yet (a container's header stays at its top), and keeps the earlier pipeline.
    let bt = import(&chains("bt"), json!({"singlePassReversed": true}));
    assert!(bt
        .layout_diagnostics
        .iter()
        .any(|d| d.starts_with("Single pass was not used")));
    for (positive, reversed, vertical) in [("lr", "rl", false)] {
        let on = json!({"singlePassReversed": true});
        let mirrored = import(&chains(reversed), on.clone());
        let plain = import(&chains(positive), json!({}));
        assert!(
            mirrored
                .layout_diagnostics
                .iter()
                .any(|d| d.starts_with("Single pass:")),
            "single pass ran: {:?}",
            mirrored.layout_diagnostics
        );
        // The same metrics as the positive drawing.
        let (a, b) = (
            arqlet_layout::published_metrics(&mirrored).unwrap(),
            arqlet_layout::published_metrics(&plain).unwrap(),
        );
        assert_eq!(
            (a.crossings, a.bends, a.line_node_hits),
            (b.crossings, b.bends, b.line_node_hits)
        );
        assert_eq!((a.width, a.height), (b.width, b.height));
        // Valid geometry, and every edge runs against the axis: its target lies before its source.
        let issues =
            arqlet_scene::validate_geometry(&mirrored.graph, &mirrored.boxes, Default::default());
        assert!(issues.is_empty(), "{reversed}: {issues:?}");
        let abs = arqlet_layout::absolute_all(&mirrored.graph, &mirrored.boxes);
        for e in &mirrored.graph.edges {
            let (s, t) = (&abs[&e.source], &abs[e.target.as_ref().unwrap()]);
            let (sc, tc) = if vertical {
                (s.y + s.height / 2., t.y + t.height / 2.)
            } else {
                (s.x + s.width / 2., t.x + t.width / 2.)
            };
            assert!(tc < sc, "{reversed}: {} flows backwards", e.id);
            // The route leaves its source and reaches its target.
            let route = mirrored.routes.iter().find(|r| r.id == e.id).unwrap();
            let (first, last) = (&route.points[0], route.points.last().unwrap());
            let on_box = |p: &arqlet_model::Point, r: &arqlet_model::Rect| {
                p.x >= r.x - 0.5
                    && p.x <= r.x + r.width + 0.5
                    && p.y >= r.y - 0.5
                    && p.y <= r.y + r.height + 0.5
            };
            assert!(
                on_box(first, s) && on_box(last, t),
                "{} attaches to its ends",
                e.id
            );
        }
        // The graph keeps its reversed directions.
        assert_eq!(
            serde_json::to_value(&mirrored.graph).unwrap()["defaultDirection"],
            reversed
        );
        // A toggle continues from the mirrored drawing.
        let s = serde_json::to_value(&mirrored).unwrap();
        let toggled = call(json!({"op": "toggle", "snapshot": s, "nodeId": "c0",
            "expectedRevision": s["revision"], "expectedLayoutVersion": s["layoutVersion"]}));
        assert_eq!(toggled["ok"], true, "{toggled}");
        // Without the option: as before, without single pass.
        let off = import(&chains(reversed), json!({}));
        assert!(off
            .layout_diagnostics
            .iter()
            .any(|d| d.starts_with("Single pass was not used")));
    }
}
