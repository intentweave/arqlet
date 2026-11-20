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
    // Paging a container that is not shown in part is refused.
    let refused: Value = serde_json::from_str(&arqlet_runtime::dispatch(
        &json!({"op": "reveal", "snapshot": all, "expectedRevision": all["revision"], "expectedLayoutVersion": all["layoutVersion"],
            "more": {"container": "box", "count": 5}}).to_string(),
    ))
    .unwrap();
    assert_eq!(refused["ok"], false);
}
