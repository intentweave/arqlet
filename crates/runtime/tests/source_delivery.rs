//! Provisional (ADR 0019): a host that keeps a large source graph asks the import to leave it out of the snapshot
//! (`omitSource`) and hands it back as `source` with each request; the fingerprint (`sourceDigest`) guards the hand-back.
use arqlet_runtime::dispatch;
use serde_json::{json, Value};

fn graph() -> Value {
    let mut nodes = vec![
        json!({"id": "g", "kind": "team", "label": "G"}),
        json!({"id": "h", "kind": "team", "label": "H"}),
    ];
    let mut edges = Vec::new();
    for i in 0..4 {
        nodes.push(json!({"id": format!("a{i}"), "kind": "fn", "label": "a", "parentId": "g"}));
        nodes.push(json!({"id": format!("b{i}"), "kind": "fn", "label": "b", "parentId": "h"}));
        edges.push(json!({"id": format!("e{i}"), "kind": "calls", "source": format!("a{i}"), "target": format!("b{i}")}));
    }
    json!({"defaultDirection": "lr", "nodes": nodes, "edges": edges})
}

fn run(request: Value) -> Value {
    serde_json::from_str(&dispatch(&request.to_string())).unwrap()
}

fn toggle(snapshot: &Value, source: Option<&Value>) -> Value {
    let mut request = json!({"op": "toggle", "snapshot": snapshot, "nodeId": "g",
        "expectedRevision": snapshot["revision"], "expectedLayoutVersion": snapshot["layoutVersion"]});
    if let Some(source) = source {
        request["source"] = source.clone();
    }
    run(request)
}

#[test]
fn an_import_can_leave_its_source_out_and_get_it_back_checked() {
    let options = json!({"levelOfDetail": {"depth": 0}});
    let whole = run(json!({"op": "import", "graph": graph(), "limit": 100, "options": options}));
    assert!(whole["snapshot"]["sourceGraph"].is_object());
    assert!(
        whole["snapshot"].get("sourceDigest").is_none(),
        "unchanged without the option"
    );
    let lean = run(
        json!({"op": "import", "graph": graph(), "limit": 100, "options": options, "omitSource": true}),
    );
    let snapshot = &lean["snapshot"];
    assert!(snapshot["sourceGraph"].is_null());
    let digest = snapshot["sourceDigest"].as_str().unwrap().to_string();
    // The same view otherwise.
    let strip = |s: &Value| {
        let mut s = s.clone();
        s.as_object_mut().unwrap().remove("sourceGraph");
        s.as_object_mut().unwrap().remove("sourceDigest");
        s
    };
    assert_eq!(strip(snapshot), strip(&whole["snapshot"]));
    // Without the source, or with another graph: refused, not computed on the view.
    assert_eq!(toggle(snapshot, None)["ok"], false);
    let mut other = graph();
    other["nodes"][2]["label"] = json!("changed");
    let refused = toggle(snapshot, Some(&other));
    assert!(refused["error"].as_str().unwrap().contains("sourceDigest"));
    // With it: the answer of the whole snapshot, again without the source.
    let opened = toggle(snapshot, Some(&graph()));
    assert_eq!(opened["ok"], true, "{opened}");
    assert!(opened["snapshot"]["sourceGraph"].is_null());
    assert_eq!(opened["snapshot"]["sourceDigest"], json!(digest));
    let opened_whole = toggle(&whole["snapshot"], None);
    assert_eq!(strip(&opened["snapshot"]), strip(&opened_whole["snapshot"]));
}

#[test]
fn an_edit_that_changes_the_source_answers_with_the_new_source() {
    let lean = run(json!({"op": "import", "graph": graph(), "limit": 100, "omitSource": true}));
    let snapshot = &lean["snapshot"];
    let added = run(json!({"op": "add", "snapshot": snapshot, "source": graph(),
        "expectedRevision": snapshot["revision"], "sourceId": "a0", "label": "new"}));
    assert_eq!(added["ok"], true, "{added}");
    assert!(
        added["snapshot"]["sourceGraph"]["nodes"]
            .as_array()
            .unwrap()
            .len()
            > graph()["nodes"].as_array().unwrap().len()
    );
    assert!(added["snapshot"].get("sourceDigest").is_none());
}

#[test]
fn a_source_with_unresolved_edges_is_handed_back_as_the_host_keeps_it() {
    let mut source = graph();
    source["edges"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id": "dangling", "kind": "calls", "source": "a0", "target": null}));
    let options = json!({"levelOfDetail": {"depth": 0}});
    let lean = run(
        json!({"op": "import", "graph": source, "limit": 100, "options": options, "omitSource": true}),
    );
    let snapshot = &lean["snapshot"];
    assert!(snapshot["sourceGraph"].is_null());
    // The host hands back the graph it sent, the unresolved edge included.
    let opened = toggle(snapshot, Some(&source));
    assert_eq!(opened["ok"], true, "{opened}");
}

#[test]
fn an_import_that_keeps_part_of_the_graph_answers_whole() {
    // A limit below the graph without an abstracting view: the snapshot keeps a selection, which the host cannot hand back.
    let lean = run(json!({"op": "import", "graph": graph(), "limit": 3, "omitSource": true}));
    assert_eq!(lean["ok"], true);
    assert!(lean["snapshot"]["sourceGraph"].is_object());
    assert!(lean["snapshot"].get("sourceDigest").is_none());
}
