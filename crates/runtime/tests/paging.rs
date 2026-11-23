//! Provisional: paging through a container shown in part (`reveal` with `more`) and a step back over it (`intent`,
//! `restore` keep the containers shown in part).
use serde_json::{json, Value};

fn run(v: Value) -> Value {
    let answer: Value = serde_json::from_str(&arqlet_runtime::dispatch(&v.to_string())).unwrap();
    assert_eq!(answer["ok"], true, "{answer}");
    answer
}

fn on(s: &Value, op: Value) -> Value {
    let mut v = op;
    v["snapshot"] = s.clone();
    v["expectedRevision"] = s["revision"].clone();
    v["expectedLayoutVersion"] = s["layoutVersion"].clone();
    run(v)["snapshot"].clone()
}

fn shown(s: &Value) -> usize {
    s["options"]["partial"]["box"]
        .as_array()
        .map_or(0, |a| a.len())
}

#[test]
fn reveal_more_pages_through_a_rest_and_a_step_back_undoes_it() {
    // A container of 30 children, collapsed at the start.
    let mut nodes = vec![json!({"id": "box", "kind": "ecu", "label": "box"})];
    for i in 0..30 {
        nodes.push(json!({"id": format!("c{i:02}"), "kind": "frame", "label": format!("c{i:02}"), "parentId": "box"}));
    }
    let graph = json!({"nodes": nodes, "edges": []});
    let first = run(json!({"op": "import", "graph": graph, "limit": 100, "options": {"levelOfDetail": {"depth": 0}}}))["snapshot"].clone();
    // Two children asked for: the rest holds 28.
    let two = on(&first, json!({"op": "reveal", "nodeIds": ["c05", "c07"]}));
    assert_eq!(shown(&two), 2);
    // The next ten, in the graph's order (c00 first).
    let twelve = on(
        &two,
        json!({"op": "reveal", "more": {"container": "box", "count": 10}}),
    );
    assert_eq!(shown(&twelve), 12);
    let ids: Vec<&str> = twelve["options"]["partial"]["box"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(
        ids.contains(&"c00") && ids.contains(&"c11") && !ids.contains(&"c12"),
        "{ids:?}"
    );
    // The rest gives the others; when none is left the container is whole, without its rest.
    let all = on(
        &twelve,
        json!({"op": "reveal", "more": {"container": "box", "count": 100}}),
    );
    assert!(
        all["options"]["partial"].get("box").is_none(),
        "{}",
        all["options"]
    );
    assert!(all["collapsedIds"]
        .as_array()
        .map_or(true, |c| !c.iter().any(|c| c == "rest:box")));
    // A step back to the page of twelve: the intent keeps the containers shown in part.
    let remembered = run(json!({"op": "intent", "snapshot": twelve}))["intent"].clone();
    assert_eq!(remembered["partial"]["box"].as_array().unwrap().len(), 12);
    let back = on(&all, json!({"op": "restore", "intent": remembered}));
    assert_eq!(shown(&back), 12);
    assert!(back["collapsedIds"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c == "rest:box"));
    // Paging a container that is shown whole is refused.
    let refused: Value = serde_json::from_str(&arqlet_runtime::dispatch(
        &json!({"op": "reveal", "snapshot": all, "expectedRevision": all["revision"], "expectedLayoutVersion": all["layoutVersion"],
            "more": {"container": "box", "count": 5}}).to_string(),
    ))
    .unwrap();
    assert_eq!(refused["ok"], false);
}

/// #146: paging starts from a collapsed container: its first page, then the next one, and a step back collapses it.
#[test]
fn reveal_more_on_a_collapsed_container_shows_its_first_page() {
    let mut nodes = vec![json!({"id": "box", "kind": "ecu", "label": "box"})];
    for i in 0..30 {
        nodes.push(json!({"id": format!("c{i:02}"), "kind": "frame", "label": format!("c{i:02}"), "parentId": "box"}));
    }
    let graph = json!({"nodes": nodes, "edges": []});
    let first = run(json!({"op": "import", "graph": graph, "limit": 100, "options": {"levelOfDetail": {"depth": 0}}}))["snapshot"].clone();
    assert!(first["collapsedIds"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c == "box"));
    let page = on(
        &first,
        json!({"op": "reveal", "more": {"container": "box", "count": 12}}),
    );
    assert_eq!(shown(&page), 12);
    let ids: Vec<&str> = page["options"]["partial"]["box"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(ids.first(), Some(&"c00"), "{ids:?}");
    assert!(!page["collapsedIds"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c == "box"));
    let next = on(
        &page,
        json!({"op": "reveal", "more": {"container": "box", "count": 12}}),
    );
    assert_eq!(shown(&next), 24);
    // Back to the start: the intent of the import restores the collapsed container.
    let remembered = run(json!({"op": "intent", "snapshot": first}))["intent"].clone();
    let back = on(&next, json!({"op": "restore", "intent": remembered}));
    assert!(back["collapsedIds"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c == "box"));
    assert_eq!(shown(&back), 0);
    // An unknown container is refused.
    let refused: Value = serde_json::from_str(&arqlet_runtime::dispatch(
        &json!({"op": "reveal", "snapshot": first, "expectedRevision": first["revision"], "expectedLayoutVersion": first["layoutVersion"],
            "more": {"container": "nowhere", "count": 5}}).to_string(),
    ))
    .unwrap();
    assert_eq!(refused["ok"], false);
}

#[test]
fn scene_nodes_count_their_children_and_the_hidden_ones() {
    let mut nodes = vec![json!({"id": "box", "kind": "ecu", "label": "box"})];
    for i in 0..30 {
        nodes.push(json!({"id": format!("c{i:02}"), "kind": "frame", "label": format!("c{i:02}"), "parentId": "box"}));
    }
    let graph = json!({"nodes": nodes, "edges": []});
    let first = run(json!({"op": "import", "graph": graph, "limit": 100, "options": {"levelOfDetail": {"depth": 0}}}))["snapshot"].clone();
    let counts = |s: &Value, id: &str| {
        let scene = run(json!({"op": "scene", "snapshot": s}))["scene"].clone();
        let node = scene["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["id"] == id)
            .cloned()
            .unwrap();
        (node["childCount"].as_u64(), node["hiddenCount"].as_u64())
    };
    // Collapsed: 30 children, all hidden.
    assert_eq!(counts(&first, "box"), (Some(30), Some(30)));
    // Two shown: 30 children, 28 in the rest; the rest box holds 28, all hidden; a leaf has no counts.
    let two = on(&first, json!({"op": "reveal", "nodeIds": ["c05", "c07"]}));
    assert_eq!(counts(&two, "box"), (Some(30), Some(28)));
    assert_eq!(counts(&two, "rest:box"), (Some(28), Some(28)));
    assert_eq!(counts(&two, "c05"), (None, None));
    // Open: 30 children, none hidden.
    let open = on(
        &two,
        json!({"op": "reveal", "more": {"container": "box", "count": 100}}),
    );
    assert_eq!(counts(&open, "box"), (Some(30), None));
}

#[test]
fn the_remembered_state_keeps_a_filter_and_a_step_back_undoes_it() {
    let mut nodes = vec![json!({"id": "box", "kind": "ecu", "label": "box"})];
    for i in 0..6 {
        nodes.push(json!({"id": format!("c{i}"), "kind": if i % 2 == 0 { "even" } else { "odd" }, "label": format!("c{i}"), "parentId": "box"}));
    }
    let first = run(json!({"op": "import", "graph": {"nodes": nodes, "edges": []}, "limit": 100}))
        ["snapshot"]
        .clone();
    let before = run(json!({"op": "intent", "snapshot": first}))["intent"].clone();
    assert!(
        before.get("hidden").is_none(),
        "omitted when nothing is hidden"
    );
    let filtered = on(
        &first,
        json!({"op": "filter", "hide": {"nodeKinds": ["odd"]}}),
    );
    let remembered = run(json!({"op": "intent", "snapshot": filtered}))["intent"].clone();
    assert_eq!(remembered["hidden"]["nodeKinds"], json!(["odd"]));
    let back = on(&filtered, json!({"op": "restore", "intent": before}));
    assert!(
        back["options"].get("hidden").is_none(),
        "{}",
        back["options"]
    );
    let again = on(&back, json!({"op": "restore", "intent": remembered}));
    assert_eq!(again["options"]["hidden"]["nodeKinds"], json!(["odd"]));
}
