//! Explicit requirements are binding for every optimisation: a pin, a port the user placed and a fixed container size hold in every
//! publication of a refinement, in its final result, and in the result of the structured optimisation. An optimisation may move
//! everything else.
use arqlet_model::{PortSide, Snapshot};
use arqlet_runtime::{
    configure_snapshot, import_view, port_edit::set_port, refine_snapshot_progressive,
};
use serde_json::{json, Value};

type Constraints = (
    Snapshot,
    String,
    (f64, f64),
    (String, String, PortSide),
    (String, f64, f64),
);

/// The snapshot of `text` with a pin on a leaf, a placed port on an edge of that leaf, and a fixed size on a container that has room.
fn constrained_from(text: &str) -> Constraints {
    let base = import_view(text, 200).unwrap();
    let has_children = |id: &str| {
        base.graph
            .nodes
            .iter()
            .any(|c| c.parent_id.as_deref() == Some(id))
    };
    let container = base
        .graph
        .nodes
        .iter()
        .find(|n| has_children(&n.id))
        .expect("a container")
        .id
        .clone();
    let leaf = base
        .graph
        .edges
        .iter()
        .find(|e| {
            e.target.is_some() && Some(&e.source) != e.target.as_ref() && !has_children(&e.source)
        })
        .expect("an edge from a leaf")
        .source
        .clone();
    let at = base.boxes[&leaf].clone();
    let size = base.boxes[&container].clone();
    let (width, height) = (size.width + 60., size.height + 60.);
    let patch = json!({
        "pins": {leaf.clone(): {"x": at.x, "y": at.y}},
        "containerSizing": {container.clone(): {"width": {"fixed": width}, "height": {"fixed": height}}}
    });
    let with_pin = configure_snapshot(&base, base.revision, base.layout_version, &patch).unwrap();
    let edge = with_pin
        .graph
        .edges
        .iter()
        .find(|e| e.source == leaf && e.target.is_some() && e.target.as_ref() != Some(&leaf))
        .expect("an edge")
        .id
        .clone();
    let placed = set_port(
        &with_pin,
        with_pin.revision,
        with_pin.layout_version,
        &edge,
        "source",
        Some((PortSide::Top, 0.25)),
    )
    .unwrap();
    (
        placed,
        leaf,
        (at.x, at.y),
        (edge, "source".into(), PortSide::Top),
        (container, width, height),
    )
}

fn constrained() -> Constraints {
    constrained_from(include_str!(
        "../../../examples/fixtures/structure/portfolio-nested.json"
    ))
}

/// Everything explicit holds in `snapshot`.
fn holds(
    snapshot: &Snapshot,
    leaf: &str,
    pin: (f64, f64),
    port: Option<&(String, String, PortSide)>,
    sized: Option<&(String, f64, f64)>,
    who: &str,
) {
    let at = &snapshot.boxes[leaf];
    assert!(
        (at.x - pin.0).abs() < 1e-6 && (at.y - pin.1).abs() < 1e-6,
        "{who}: the pinned node moved to {at:?}, pin {pin:?}"
    );
    if let Some(port) = port {
        let end = snapshot
            .resolved_ports
            .iter()
            .find(|p| p.edge_id == port.0 && p.endpoint == port.1)
            .unwrap_or_else(|| panic!("{who}: the end of {} has no port", port.0));
        assert!(
            end.explicit && end.side == port.2,
            "{who}: the placed port was changed: {end:?}"
        );
    }
    if let Some(sized) = sized {
        let container = &snapshot.boxes[&sized.0];
        assert!(
            (container.width - sized.1).abs() < 1e-6 && (container.height - sized.2).abs() < 1e-6,
            "{who}: the fixed size of {} became {}x{}",
            sized.0,
            container.width,
            container.height
        );
    }
}

#[test]
fn the_refinement_keeps_every_explicit_requirement_in_every_publication_and_in_its_result() {
    let (start, leaf, pin, port, sized) = constrained();
    holds(&start, &leaf, pin, Some(&port), Some(&sized), "the start");
    let mut seen = 0;
    let (result, _) = refine_snapshot_progressive(
        "job",
        &start,
        start.revision,
        start.layout_version,
        1,
        None,
        None,
        &mut |p| {
            seen += 1;
            holds(
                &p.snapshot,
                &leaf,
                pin,
                Some(&port),
                Some(&sized),
                &format!("publication {}", p.sequence),
            );
        },
    )
    .unwrap();
    holds(
        &result,
        &leaf,
        pin,
        Some(&port),
        Some(&sized),
        "the final result",
    );
    println!("{seen} publications checked");
}

/// The structured optimisation returns a result and keeps every requirement. The prototypes (by container, by component) do not read
/// pins, authored sizes or explicit ports; they must refuse such a view and not move or resize anything silently.
#[test]
fn every_other_optimisation_keeps_every_explicit_requirement() {
    let (start, leaf, pin, port, sized) = constrained();
    let reply: Value = serde_json::from_str(&arqlet_runtime::dispatch(
        &json!({"op": "structuredRefine", "snapshot": start, "expectedRevision": start.revision, "expectedLayoutVersion": start.layout_version}).to_string(),
    ))
    .unwrap();
    assert_eq!(reply["ok"], true, "structuredRefine: {reply}");
    let result: Snapshot = serde_json::from_value(reply["snapshot"].clone()).unwrap();
    holds(
        &result,
        &leaf,
        pin,
        Some(&port),
        Some(&sized),
        "structuredRefine",
    );

    let refuses = |op: &str, graph: &arqlet_model::Graph, options: &arqlet_model::LayoutOptions| {
        let reply: Value = serde_json::from_str(&arqlet_runtime::dispatch(
            &json!({"op": op, "graph": graph, "options": options}).to_string(),
        ))
        .unwrap();
        reply["ok"] == false
    };
    // Each requirement alone: a pin, an authored size, a placed port.
    let (one, ..) = constrained_from(include_str!(
        "../../../examples/fixtures/structure/container.json"
    ));
    let mut bare_graph = one.graph.clone();
    bare_graph.ports.clear();
    bare_graph.edge_ports.clear();
    let mut bare = one.options.clone();
    bare.pins.clear();
    bare.container_sizing.clear();
    let mut pinned = bare.clone();
    pinned.pins = one.options.pins.clone();
    let mut sized_only = bare.clone();
    sized_only.container_sizing = one.options.container_sizing.clone();
    assert!(!pinned.pins.is_empty() && !sized_only.container_sizing.is_empty());
    assert!(
        !refuses("containerRefine", &bare_graph, &bare),
        "the bare view must run"
    );
    assert!(refuses("containerRefine", &bare_graph, &pinned), "a pin");
    assert!(
        refuses("containerRefine", &bare_graph, &sized_only),
        "an authored size"
    );
    assert!(
        refuses("containerRefine", &one.graph, &bare),
        "a placed port"
    );

    let flat = import_view(
        include_str!("../../../examples/fixtures/structure/disconnected.json"),
        100,
    )
    .unwrap();
    let mut pins = flat.options.clone();
    let id = flat.graph.nodes[0].id.clone();
    let at = &flat.boxes[&id];
    pins.pins
        .insert(id, arqlet_model::Point { x: at.x, y: at.y });
    assert!(
        !refuses("componentRefine", &flat.graph, &flat.options),
        "the bare view must run"
    );
    assert!(refuses("componentRefine", &flat.graph, &pins), "a pin");
}

/// A direction, a strategy and a routing set through `dispatch` change the configuration the engine accepted: read back from the
/// snapshot's intent, for the view and for a container, and a refused value changes nothing.
#[test]
fn the_accepted_configuration_is_the_requested_one() {
    let (start, ..) = constrained();
    let container = start
        .graph
        .nodes
        .iter()
        .find(|n| {
            start
                .graph
                .nodes
                .iter()
                .any(|c| c.parent_id.as_deref() == Some(n.id.as_str()))
        })
        .unwrap()
        .id
        .clone();
    let apply = |snapshot: &Snapshot, op: &str, node: Option<&str>, value: Value| -> Value {
        serde_json::from_str(&arqlet_runtime::dispatch(
            &json!({"op": op, "snapshot": snapshot, "expectedRevision": snapshot.revision,
                "expectedLayoutVersion": snapshot.layout_version, "nodeId": node, op: value})
            .to_string(),
        ))
        .unwrap()
    };
    let mut current = start.clone();
    for (op, value) in [
        ("direction", json!("tb")),
        ("strategy", json!("organic")),
        ("routing", json!("straight")),
    ] {
        for node in [None, Some(container.as_str())] {
            let reply = apply(&current, op, node, value.clone());
            assert_eq!(reply["ok"], true, "{op} {node:?}: {reply}");
            current = serde_json::from_value(reply["snapshot"].clone()).unwrap();
            let intent = arqlet_runtime::history::intent_of(&current);
            let got = match (op, node) {
                ("direction", None) => json!(intent.default_direction),
                ("direction", Some(id)) => json!(intent.container_directions[id]),
                ("strategy", None) => json!(intent.default_strategy),
                ("strategy", Some(id)) => json!(intent.container_strategies[id]),
                ("routing", None) => json!(intent.default_routing),
                (_, Some(id)) => json!(intent.container_routing[id]),
                _ => unreachable!(),
            };
            assert_eq!(got, value, "{op} {node:?} was not accepted as requested");
        }
    }
    // A refused value changes nothing: a tree on a view with antiparallel links.
    let before = arqlet_runtime::history::intent_of(&current);
    let reply = apply(&current, "strategy", None, json!("tree"));
    if reply["ok"] == false {
        assert_eq!(arqlet_runtime::history::intent_of(&current), before);
    }
}

/// The strategy an `auto` area took in the structured optimisation is in the snapshot; a fixed area never appears, and a plain layout
/// (which treats `auto` as `none`) records nothing.
#[test]
fn the_strategy_an_auto_area_chose_is_recorded_only_by_the_search() {
    let base = import_view(
        include_str!("../../../examples/fixtures/structure/portfolio-nested.json"),
        200,
    )
    .unwrap();
    let apply = |snapshot: &Snapshot, node: Option<&str>, value: Value| -> Snapshot {
        let reply: Value = serde_json::from_str(&arqlet_runtime::dispatch(
            &json!({"op": "strategy", "snapshot": snapshot, "expectedRevision": snapshot.revision,
                "expectedLayoutVersion": snapshot.layout_version, "nodeId": node, "strategy": value})
            .to_string(),
        ))
        .unwrap();
        assert_eq!(reply["ok"], true, "{reply}");
        serde_json::from_value(reply["snapshot"].clone()).unwrap()
    };
    let free = apply(&base, None, json!("auto"));
    assert!(
        free.area_strategies.is_empty(),
        "a plain layout records no choice"
    );
    let reply: Value = serde_json::from_str(&arqlet_runtime::dispatch(
        &json!({"op": "structuredRefine", "snapshot": free, "expectedRevision": free.revision, "expectedLayoutVersion": free.layout_version}).to_string(),
    ))
    .unwrap();
    assert_eq!(reply["ok"], true, "{reply}");
    let refined: Snapshot = serde_json::from_value(reply["snapshot"].clone()).unwrap();
    println!(
        "route {}, chose {:?}",
        reply["route"], refined.area_strategies
    );
    if reply["route"] == "hierarchy" {
        assert!(refined.area_strategies.contains_key("<root>"));
    } else {
        assert!(refined.area_strategies.is_empty());
    }
}
