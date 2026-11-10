//! The ELK-form adapter through `dispatch` (`elkLayout`): every node, port, label and edge of the input comes back laid out,
//! in ELK's frames, with the edges in their logical direction and the untranslatable options reported.

use arqlet_runtime::dispatch;
use serde_json::{json, Value};

fn run(request: Value) -> Value {
    serde_json::from_str(&dispatch(&request.to_string())).unwrap()
}

fn sample() -> Value {
    json!({
        "id": "root",
        "layoutOptions": {"elk.algorithm": "layered", "elk.direction": "DOWN", "elk.spacing.nodeNode": 20},
        "children": [
            {"id": "a", "width": 40, "height": 30, "labels": [{"text": "A", "width": 10, "height": 10}],
             "ports": [{"id": "a_out", "width": 4, "height": 4, "layoutOptions": {"elk.port.side": "SOUTH"}}]},
            {"id": "group", "layoutOptions": {"elk.direction": "RIGHT"},
             "children": [{"id": "b", "width": 40, "height": 30}, {"id": "c", "width": 40, "height": 30}],
             "edges": [{"id": "bc", "sources": ["b"], "targets": ["c"]}, {"id": "loop", "sources": ["c"], "targets": ["c"]}]}
        ],
        "edges": [
            {"id": "ab", "sources": ["a_out"], "targets": ["b"], "labels": [{"text": "x", "width": 12, "height": 8}]},
            {"id": "hyper", "sources": ["a"], "targets": ["b", "c"]},
            {"id": "back", "sources": ["c"], "targets": ["a"]}
        ]
    })
}

/// The absolute frame of every node, from the written `x`/`y`.
fn frames(graph: &Value) -> std::collections::BTreeMap<String, (f64, f64, f64, f64)> {
    let mut out = std::collections::BTreeMap::new();
    let mut stack = vec![(graph.clone(), 0., 0.)];
    while let Some((node, ox, oy)) = stack.pop() {
        for child in node["children"].as_array().into_iter().flatten() {
            let (x, y) = (
                ox + child["x"].as_f64().unwrap(),
                oy + child["y"].as_f64().unwrap(),
            );
            out.insert(
                child["id"].as_str().unwrap().to_string(),
                (
                    x,
                    y,
                    child["width"].as_f64().unwrap(),
                    child["height"].as_f64().unwrap(),
                ),
            );
            stack.push((child.clone(), x, y));
        }
    }
    out.insert(
        "root".into(),
        (
            0.,
            0.,
            graph["width"].as_f64().unwrap(),
            graph["height"].as_f64().unwrap(),
        ),
    );
    out
}

#[test]
fn an_elk_graph_comes_back_laid_out_in_its_own_form() {
    let reply = run(json!({"op": "elkLayout", "graph": sample()}));
    assert_eq!(reply["ok"], true, "{reply}");
    let graph = &reply["graph"];
    let frames = frames(graph);
    // Leaves keep their size; containers get one.
    assert_eq!((frames["a"].2, frames["a"].3), (40., 30.));
    assert!(frames["group"].2 > 80. && frames["group"].3 > 30.);
    // The port lies on its node's bottom side.
    let port = &graph["children"][0]["ports"][0];
    let (px, py) = (
        port["x"].as_f64().unwrap() + 2.,
        port["y"].as_f64().unwrap() + 2.,
    );
    assert!(
        (py - 30.).abs() < 1e-6 && (0. ..=40.).contains(&px),
        "{port}"
    );
    // Every edge has its sections, starting at its source and ending at its target, in its container's frame.
    let mut edges: Vec<(Value, String)> = graph["edges"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| (e.clone(), "root".to_string()))
        .collect();
    edges.extend(
        graph["children"][1]["edges"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| (e.clone(), "group".to_string())),
    );
    let mut sections = 0;
    for (edge, container) in &edges {
        assert_eq!(edge["container"], container.as_str(), "{edge}");
        let (ox, oy, _, _) = frames[container.as_str()];
        for section in edge["sections"].as_array().unwrap() {
            sections += 1;
            let at = |p: &Value| (ox + p["x"].as_f64().unwrap(), oy + p["y"].as_f64().unwrap());
            let on = |(x, y): (f64, f64), shape: &str| {
                let shape = if shape == "a_out" { "a" } else { shape };
                let (sx, sy, sw, sh) = frames[shape];
                x >= sx - 1e-6 && x <= sx + sw + 1e-6 && y >= sy - 1e-6 && y <= sy + sh + 1e-6
            };
            let (start, end) = (at(&section["startPoint"]), at(&section["endPoint"]));
            assert!(
                on(start, section["incomingShape"].as_str().unwrap()),
                "{section}"
            );
            assert!(
                on(end, section["outgoingShape"].as_str().unwrap()),
                "{section}"
            );
        }
    }
    // ab, two of hyper, back, bc, loop.
    assert_eq!(sections, 6);
    let label = &graph["edges"][0]["labels"][0];
    assert!(label["x"].is_number() && label["y"].is_number(), "{label}");
    let diagnostics = reply["diagnostics"].as_array().unwrap();
    // The spacing is read (as `nodeSpacing`), not reported.
    assert!(!diagnostics
        .iter()
        .any(|d| d.as_str().unwrap().contains("spacing.nodeNode")));
    // The hyperedge (one source) is a bus, not split with a report.
    assert!(!diagnostics
        .iter()
        .any(|d| d.as_str().unwrap().contains("hyperedges")));
}

#[test]
fn a_broken_elk_graph_is_an_explicit_error() {
    let mut broken = sample();
    broken["edges"][0]["targets"] = json!(["nowhere"]);
    let reply = run(json!({"op": "elkLayout", "graph": broken}));
    assert_eq!(reply["ok"], false);
    assert!(reply["error"].as_str().unwrap().contains("nowhere"));
}

/// ELK's spacings change the drawing: wider gaps between neighbours and between layers give a larger graph.
#[test]
fn elk_spacings_widen_the_layout() {
    let size = |spacing: Value| {
        let mut graph = sample();
        graph["layoutOptions"]["elk.spacing.nodeNode"] = spacing.clone();
        graph["layoutOptions"]["elk.spacing.nodeNodeBetweenLayers"] = spacing;
        let reply = run(json!({"op": "elkLayout", "graph": graph}));
        assert_eq!(reply["ok"], true, "{reply}");
        (
            reply["graph"]["width"].as_f64().unwrap(),
            reply["graph"]["height"].as_f64().unwrap(),
        )
    };
    let (tight, wide) = (size(json!(10)), size(json!(150)));
    assert!(wide.0 > tight.0 && wide.1 > tight.1, "{tight:?} {wide:?}");
}

/// `nodeLabels.placement` decides where a node's label is written; `snapshot: true` adds the Arqlet snapshot behind it.
#[test]
fn label_placement_and_snapshot() {
    let mut graph = sample();
    graph["children"][0]["layoutOptions"] =
        json!({"elk.nodeLabels.placement": "[H_LEFT, V_TOP, INSIDE]"});
    let reply = run(json!({"op": "elkLayout", "graph": graph, "snapshot": true}));
    assert_eq!(reply["ok"], true, "{reply}");
    let label = &reply["graph"]["children"][0]["labels"][0];
    assert_eq!(
        (label["x"].as_f64(), label["y"].as_f64()),
        (Some(5.), Some(5.))
    );
    assert!(reply["snapshot"]["boxes"]["a"].is_object());
}

/// Ports on every side (the translation turns on `portSides`): single pass constructs every edge, and each section runs
/// from its source's port to its target's port.
#[test]
fn ports_on_every_side_are_constructed() {
    let node = |id: &str| {
        let ports: Vec<Value> = ["NORTH", "SOUTH", "EAST", "WEST"]
            .iter()
            .map(|s| json!({"id": format!("{id}_{}", &s[..1]), "layoutOptions": {"elk.port.side": s}}))
            .collect();
        json!({"id": id, "width": 80, "height": 40,
            "layoutOptions": {"elk.portConstraints": "FIXED_SIDE"}, "ports": ports})
    };
    let graph = json!({
        "id": "root",
        "children": [node("a"), node("b"), node("c")],
        "edges": [
            {"id": "e1", "sources": ["a_N"], "targets": ["b_W"]},
            {"id": "e2", "sources": ["a_S"], "targets": ["c_N"]},
            {"id": "e3", "sources": ["b_E"], "targets": ["c_S"]},
            {"id": "e4", "sources": ["a_W"], "targets": ["c_W"]}
        ]
    });
    let reply = run(json!({"op": "elkLayout", "graph": graph, "snapshot": true}));
    assert_eq!(reply["ok"], true, "{reply}");
    let summary = reply["snapshot"]["layoutDiagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .find(|d| d.starts_with("Single pass:"))
        .unwrap()
        .to_string();
    assert!(summary.contains("4 constructed, 0 delegated"), "{summary}");
    let ports: std::collections::BTreeMap<String, (f64, f64)> = reply["graph"]["children"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|n| {
            let (x, y) = (n["x"].as_f64().unwrap(), n["y"].as_f64().unwrap());
            // Ports without an edge get no place.
            n["ports"].as_array().unwrap().iter().filter_map(move |p| {
                Some((
                    p["id"].as_str().unwrap().to_string(),
                    (x + p["x"].as_f64()?, y + p["y"].as_f64()?),
                ))
            })
        })
        .collect();
    for edge in reply["graph"]["edges"].as_array().unwrap() {
        let section = &edge["sections"][0];
        let at = |p: &Value| (p["x"].as_f64().unwrap(), p["y"].as_f64().unwrap());
        let near =
            |a: (f64, f64), b: (f64, f64)| (a.0 - b.0).abs() < 1e-6 && (a.1 - b.1).abs() < 1e-6;
        assert!(
            near(
                at(&section["startPoint"]),
                ports[edge["sources"][0].as_str().unwrap()]
            ),
            "{edge}"
        );
        assert!(
            near(
                at(&section["endPoint"]),
                ports[edge["targets"][0].as_str().unwrap()]
            ),
            "{edge}"
        );
    }
}

/// A hyperedge with one source is drawn as a bus: its sections leave by one port, share the way to where they part, and the
/// edge names that place as ELK's junction point.
#[test]
fn a_hyperedge_with_one_source_is_a_bus() {
    let graph = json!({
        "id": "root",
        "children": [
            {"id": "a", "width": 40, "height": 30}, {"id": "b", "width": 40, "height": 30},
            {"id": "c", "width": 40, "height": 30}, {"id": "d", "width": 40, "height": 30}
        ],
        "edges": [{"id": "h", "sources": ["a"], "targets": ["b", "c", "d"]}]
    });
    let reply = run(json!({"op": "elkLayout", "graph": graph}));
    assert_eq!(reply["ok"], true, "{reply}");
    let edge = &reply["graph"]["edges"][0];
    let sections = edge["sections"].as_array().unwrap();
    assert_eq!(sections.len(), 3);
    let start = &sections[0]["startPoint"];
    assert!(sections.iter().all(|s| &s["startPoint"] == start), "{edge}");
    assert!(
        !edge["junctionPoints"].as_array().unwrap().is_empty(),
        "{edge}"
    );
    assert!(
        reply["diagnostics"].as_array().unwrap().is_empty(),
        "{reply}"
    );
}

/// A bus between several senders and several receivers (the generator's CAN example): one line across the channel between
/// them, one stub per node, junction dots in the scene, and no conflict between the members.
#[test]
fn a_backbone_bus_has_one_line_and_one_stub_per_node() {
    let mut graph = json!({"defaultDirection": "tb", "nodes": [], "edges": []});
    for id in ["a", "b", "x", "y"] {
        graph["nodes"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id": id, "kind": "ecu", "label": id}));
    }
    let pairs = [("a", "x"), ("a", "y"), ("b", "x"), ("b", "y")];
    for (k, (s, t)) in pairs.iter().enumerate() {
        graph["edges"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id": format!("e{k}"), "kind": "can", "source": s, "target": t}));
    }
    graph["buses"] = json!([{"id": "can", "edges": ["e0", "e1", "e2", "e3"]}]);
    let reply = run(json!({"op": "import", "graph": graph, "limit": 100}));
    assert_eq!(reply["ok"], true, "{reply}");
    let snapshot = &reply["snapshot"];
    let diagnostics: Vec<&str> = snapshot["layoutDiagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert!(
        !diagnostics.iter().any(|d| d.starts_with("Bus ")),
        "{diagnostics:?}"
    );
    let routes = snapshot["routes"].as_array().unwrap();
    let start = |id: &str| routes.iter().find(|r| r["id"] == id).unwrap()["points"][0].clone();
    assert_eq!(start("e0"), start("e1"), "one stub at a");
    assert_eq!(start("e2"), start("e3"), "one stub at b");
    // Every member crosses the channel on the same line: the turning ones turn on it, a straight one passes it.
    let points = |r: &Value| -> Vec<(f64, f64)> {
        r["points"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| (p["x"].as_f64().unwrap(), p["y"].as_f64().unwrap()))
            .collect()
    };
    let turning: Vec<f64> = routes
        .iter()
        .map(points)
        .filter(|p| p.len() >= 3)
        .map(|p| p[1].1)
        .collect();
    assert!(!turning.is_empty());
    assert!(
        turning.iter().all(|y| (y - turning[0]).abs() < 1e-6),
        "{turning:?}"
    );
    assert!(routes
        .iter()
        .map(points)
        .all(|p| p.first().unwrap().1 < turning[0] && p.last().unwrap().1 > turning[0]));
    let scene = run(json!({"op": "scene", "snapshot": snapshot}));
    assert!(
        !scene["scene"]["junctions"].as_array().unwrap().is_empty(),
        "{scene}"
    );
    assert!(scene["scene"]["routes"]
        .as_array()
        .unwrap()
        .iter()
        .all(|r| r["invalid"] == false));
    // The bus took part in the layout as a virtual node: its box is kept apart, and the canonical graph and routes are
    // untouched by it.
    assert_eq!(
        snapshot["busBoxes"].as_object().unwrap().len(),
        1,
        "{snapshot}"
    );
    assert_eq!(snapshot["graph"]["nodes"].as_array().unwrap().len(), 4);
    let ids: Vec<&str> = routes.iter().map(|r| r["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["e0", "e1", "e2", "e3"]);
    // A later layout of the same snapshot (a configure) continues from it and keeps the bus.
    let again = run(json!({
        "op": "configure", "snapshot": snapshot, "expectedRevision": snapshot["revision"],
        "expectedLayoutVersion": snapshot["layoutVersion"], "options": {"trackSpacing": 6.0}
    }));
    assert_eq!(again["ok"], true, "{again}");
    assert_eq!(again["snapshot"]["busBoxes"].as_object().unwrap().len(), 1);
}

/// A bus whose members pass no channel together (two senders to a gateway, the gateway to two receivers): one backbone per
/// channel, each with its own line, and nothing reported.
#[test]
fn a_bus_over_two_channels_gets_a_line_in_each() {
    let mut graph = json!({"defaultDirection": "tb", "nodes": [], "edges": []});
    for id in ["a", "b", "gw", "y", "z"] {
        graph["nodes"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id": id, "kind": "ecu", "label": id}));
    }
    for (k, (s, t)) in [("a", "gw"), ("b", "gw"), ("gw", "y"), ("gw", "z")]
        .iter()
        .enumerate()
    {
        graph["edges"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id": format!("e{k}"), "kind": "can", "source": s, "target": t}));
    }
    graph["buses"] = json!([{"id": "can", "edges": ["e0", "e1", "e2", "e3"]}]);
    let reply = run(json!({"op": "import", "graph": graph, "limit": 100}));
    assert_eq!(reply["ok"], true, "{reply}");
    let snapshot = &reply["snapshot"];
    let diagnostics: Vec<&str> = snapshot["layoutDiagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert!(
        !diagnostics.iter().any(|d| d.starts_with("Bus ")),
        "{diagnostics:?}"
    );
    let routes = snapshot["routes"].as_array().unwrap();
    let end = |id: &str| {
        routes.iter().find(|r| r["id"] == id).unwrap()["points"]
            .as_array()
            .unwrap()
            .last()
            .unwrap()
            .clone()
    };
    let start = |id: &str| routes.iter().find(|r| r["id"] == id).unwrap()["points"][0].clone();
    assert_eq!(end("e0"), end("e1"), "one stub into the gateway");
    assert_eq!(start("e2"), start("e3"), "one stub out of the gateway");
}

/// Bus members between software components inside ECUs (containers): each keeps its way inside its ECUs and shares the bus
/// line between them; the members that end at one component share its way in.
#[test]
fn a_bus_between_components_inside_containers() {
    let mut graph = json!({"defaultDirection": "tb", "nodes": [], "edges": []});
    let nodes = graph["nodes"].as_array_mut().unwrap();
    for (id, kind, parent) in [
        ("ecu1", "container", None),
        ("tx1", "software", Some("ecu1")),
        ("ecu2", "container", None),
        ("tx2", "software", Some("ecu2")),
        ("ecu3", "container", None),
        ("rx", "software", Some("ecu3")),
    ] {
        nodes.push(json!({"id": id, "kind": kind, "label": id, "parentId": parent}));
    }
    for (k, (s, t)) in [("tx1", "rx"), ("tx2", "rx")].iter().enumerate() {
        graph["edges"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id": format!("m{k}"), "kind": "Msg", "source": s, "target": t}));
    }
    graph["buses"] = json!([{"id": "can", "edges": ["m0", "m1"]}]);
    let reply = run(json!({"op": "import", "graph": graph, "limit": 100}));
    assert_eq!(reply["ok"], true, "{reply}");
    let snapshot = &reply["snapshot"];
    let diagnostics: Vec<&str> = snapshot["layoutDiagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert!(
        !diagnostics.iter().any(|d| d.starts_with("Bus ")),
        "{diagnostics:?}"
    );
    let routes = snapshot["routes"].as_array().unwrap();
    let points = |id: &str| {
        routes.iter().find(|r| r["id"] == id).unwrap()["points"]
            .as_array()
            .unwrap()
            .clone()
    };
    // One way into rx: both end at one point and arrive on one line.
    let (a, b) = (points("m0"), points("m1"));
    assert_eq!(a.last(), b.last(), "{a:?} {b:?}");
    assert_eq!(a[a.len() - 2]["x"], b[b.len() - 2]["x"], "{a:?} {b:?}");
    let scene = run(json!({"op": "scene", "snapshot": snapshot}));
    assert!(
        scene["scene"]["routes"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["invalid"] == false),
        "{scene}"
    );
}

/// A bus keeps working when an ECU is collapsed (the view moves the members' ends onto the ECU), and its label is placed at
/// the bus when the options give its size.
#[test]
fn a_bus_survives_collapsing_an_ecu_and_carries_its_label() {
    let mut graph = json!({"defaultDirection": "tb", "nodes": [], "edges": []});
    let nodes = graph["nodes"].as_array_mut().unwrap();
    for (id, kind, parent) in [
        ("ecu1", "container", None),
        ("tx1", "software", Some("ecu1")),
        ("tx1b", "software", Some("ecu1")),
        ("tx2", "ecu", None),
        ("ecu3", "container", None),
        ("rx", "software", Some("ecu3")),
        ("rx2", "ecu", None),
    ] {
        nodes.push(json!({"id": id, "kind": kind, "label": id, "parentId": parent}));
    }
    graph["edges"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id": "i", "kind": "signal", "source": "tx1b", "target": "tx1"}));
    for (k, (s, t)) in [("tx1", "rx"), ("tx2", "rx"), ("tx1", "rx2"), ("tx2", "rx2")]
        .iter()
        .enumerate()
    {
        graph["edges"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id": format!("m{k}"), "kind": "Msg", "source": s, "target": t}));
    }
    graph["buses"] =
        json!([{"id": "can", "edges": ["m0", "m1", "m2", "m3"], "label": "CAN 500 kbit/s"}]);
    let options =
        json!({"edgeLabels": {"bus:can": {"text": "CAN 500 kbit/s", "width": 110, "height": 22}}});
    let reply = run(json!({"op": "import", "graph": graph, "limit": 100, "options": options}));
    assert_eq!(reply["ok"], true, "{reply}");
    let snapshot = reply["snapshot"].clone();
    let labels = snapshot["labels"].as_array().unwrap();
    assert!(
        labels
            .iter()
            .any(|l| l["kind"] == "bus" && l["text"] == "CAN 500 kbit/s"),
        "{labels:?}"
    );
    let collapsed = run(json!({
        "op": "toggle", "snapshot": snapshot, "expectedRevision": snapshot["revision"],
        "expectedLayoutVersion": snapshot["layoutVersion"], "nodeId": "ecu1"
    }));
    assert_eq!(collapsed["ok"], true, "{collapsed}");
    let s = &collapsed["snapshot"];
    let diagnostics: Vec<&str> = s["layoutDiagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert!(
        !diagnostics.iter().any(|d| d.starts_with("Bus ")),
        "{diagnostics:?}"
    );
    assert_eq!(s["busBoxes"].as_object().unwrap().len(), 1);
    let scene = run(json!({"op": "scene", "snapshot": s}));
    assert!(
        scene["scene"]["routes"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["invalid"] == false),
        "{scene}"
    );
    assert!(scene["scene"]["labels"]
        .as_array()
        .unwrap()
        .iter()
        .any(|l| l["kind"] == "bus"));
}
