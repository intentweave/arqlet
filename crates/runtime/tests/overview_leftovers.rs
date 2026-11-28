//! The leftovers of the overview as a server result (#241): edge ids on request, a session given a new graph with
//! `start: {}`, a start by index after a continuation by id, and the keys of `start.nodes` entries.
use arqlet_runtime::dispatch;
use arqlet_runtime::engine_session::EngineSession;
use serde_json::{json, Value};

/// Three groups of three files of five leaves, with edges inside the files, between them and across the groups.
fn graph() -> Value {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    for g in 0..3 {
        nodes.push(json!({"id": format!("g{g}"), "kind": "dir", "label": format!("g{g}"), "parentId": null}));
        for f in 0..3 {
            let file = format!("g{g}/f{f}");
            nodes.push(
                json!({"id": file, "kind": "file", "label": file, "parentId": format!("g{g}")}),
            );
            for l in 0..5 {
                nodes.push(json!({"id": format!("{file}/n{l}"), "kind": "fn", "label": "n", "parentId": file}));
                edges.push(json!({"id": format!("{file}/r{l}"), "kind": "call", "source": format!("{file}/n{l}"),
                    "target": format!("{file}/n{}", (l + 1) % 5)}));
            }
            if f > 0 {
                edges.push(json!({"id": format!("{file}/up"), "kind": "call", "source": format!("{file}/n0"),
                    "target": format!("g{g}/f{}/n1", f - 1)}));
            }
        }
    }
    edges.push(json!({"id": "open", "kind": "call", "source": "g0/f0/n0", "target": null}));
    edges.push(json!({"id": "x", "kind": "call", "source": "g0/f0/n0", "target": "g1/f0/n0"}));
    json!({"nodes": nodes, "edges": edges})
}

fn answer(request: &Value) -> Value {
    let response: Value = serde_json::from_str(&dispatch(&request.to_string())).unwrap();
    assert_eq!(response["ok"], true, "{response}");
    response["overview"].clone()
}

fn refusal(request: &Value) -> String {
    let response: Value = serde_json::from_str(&dispatch(&request.to_string())).unwrap();
    assert_eq!(response["ok"], false, "{response}");
    response["error"].as_str().unwrap().to_string()
}

fn places(o: &Value) -> Value {
    let mut map = serde_json::Map::new();
    for (i, id) in o["ids"].as_array().unwrap().iter().enumerate() {
        let mut place = json!({"x": o["x"][i], "y": o["y"][i], "r": o["r"][i]});
        if let Some(p) = o["parent"][i].as_i64().filter(|p| *p >= 0) {
            place["parent"] = o["ids"][p as usize].clone();
        }
        map.insert(id.as_str().unwrap().to_string(), place);
    }
    Value::Object(map)
}

/// The graph with one more leaf and its edge.
fn grown() -> Value {
    let mut g = graph();
    g["nodes"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id": "g0/f0/new", "kind": "fn", "label": "new", "parentId": "g0/f0"}));
    g["edges"].as_array_mut().unwrap().push(
        json!({"id": "new-edge", "kind": "call", "source": "g0/f0/new", "target": "g0/f0/n1"}),
    );
    g
}

fn session_answer(session: &mut EngineSession, request: &Value) -> Value {
    let response = session.run_value(request);
    assert_eq!(response["ok"], true, "{response}");
    response["overview"].clone()
}

#[test]
fn edge_ids_come_on_request_aligned_with_the_edges_and_leave_other_answers_alone() {
    let g = graph();
    let plain = answer(&json!({"op": "overview", "graph": g}));
    assert!(plain.get("edgeIds").is_none());
    let o = answer(&json!({"op": "overview", "graph": g, "edgeIds": true}));
    // Nothing else changes: the same bytes once the field is taken out.
    let mut without = o.clone();
    without.as_object_mut().unwrap().remove("edgeIds");
    assert_eq!(without, plain);
    // The unresolved edge is out; the others follow input order, aligned with `edges` (and `edgeKind`).
    let ids = o["edgeIds"].as_array().unwrap();
    let resolved: Vec<&Value> = g["edges"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| !e["target"].is_null())
        .map(|e| &e["id"])
        .collect();
    assert_eq!(ids.iter().collect::<Vec<_>>(), resolved);
    assert_eq!(ids.len() * 2, o["edges"].as_array().unwrap().len());
    let index = |id: &str| {
        plain["ids"]
            .as_array()
            .unwrap()
            .iter()
            .position(|x| x == id)
            .unwrap()
    };
    let k = ids.iter().position(|e| e == "g0/f1/up").unwrap();
    assert_eq!(o["edges"][2 * k], index("g0/f1/n0"));
    assert_eq!(o["edges"][2 * k + 1], index("g0/f0/n1"));
    // With the edge data too, and not true: as without.
    let both = answer(&json!({"op": "overview", "graph": g, "edgeIds": true, "edgeData": true}));
    assert_eq!(both["edgeIds"], o["edgeIds"]);
    assert_eq!(both["edgeKind"].as_array().unwrap().len(), ids.len());
    assert_eq!(
        answer(&json!({"op": "overview", "graph": g, "edgeIds": false})),
        plain
    );
    assert!(refusal(&json!({"op": "overview", "graph": g, "edgeIds": 1})).contains("edgeIds"));
    // The same for any thread count.
    assert_eq!(
        answer(&json!({"op": "overview", "graph": g, "edgeIds": true, "threads": 4})),
        o
    );
    // The typed contract takes the field.
    serde_json::from_value::<arqlet_model::Overview>(o).unwrap();
}

#[test]
fn edge_ids_stay_in_the_json_of_a_columnar_answer_and_a_session_sends_them_when_asked_anew() {
    let g = graph();
    let whole = answer(&json!({"op": "overview", "graph": g, "edgeIds": true}));
    let mut session = EngineSession::new();
    let first = session_answer(
        &mut session,
        &json!({"op": "overview", "graph": g, "edgeIds": true, "columns": true}),
    );
    assert_eq!(first["edgeIds"], whole["edgeIds"]);
    let names: Vec<&str> = session
        .overview_columns()
        .unwrap()
        .columns()
        .iter()
        .map(|(n, _)| *n)
        .collect();
    assert!(!names.contains(&"edgeIds"));
    // Asked again with them: only what moves; without them, or newly asked: the whole overview.
    let next = json!({"op": "overview", "start": {}, "positionsOnly": true, "edgeIds": true});
    assert_eq!(session_answer(&mut session, &next)["positionsOnly"], true);
    let mut off = next.clone();
    off["edgeIds"] = json!(false);
    assert!(session_answer(&mut session, &off)["ids"].is_array());
    assert_eq!(session_answer(&mut session, &off)["positionsOnly"], true);
    let again = session_answer(&mut session, &next);
    assert_eq!(again["edgeIds"], whole["edgeIds"], "{again}");
}

#[test]
fn a_session_given_a_new_graph_with_an_empty_start_continues_from_its_places_by_id() {
    let (g, changed) = (graph(), grown());
    let mut session = EngineSession::new();
    let first = session_answer(&mut session, &json!({"op": "overview", "graph": g}));
    let next = session_answer(
        &mut session,
        &json!({"op": "overview", "graph": changed, "start": {}}),
    );
    // As dispatch, given the places by id of the first answer.
    let by_id =
        answer(&json!({"op": "overview", "graph": changed, "start": {"nodes": places(&first)}}));
    assert_eq!(next, by_id);
    assert_eq!(next["continuation"]["anew"], 1);
    // A third request without graph continues from the answer of the second, as after any continuation by id.
    let third = session_answer(
        &mut session,
        &json!({"op": "overview", "start": {"settle": false}}),
    );
    assert_eq!(third["ids"], next["ids"]);
    // Without a last overview there is nothing to continue from.
    let mut fresh = EngineSession::new();
    let refused = fresh.run_value(&json!({"op": "overview", "graph": g, "start": {}}));
    assert_eq!(refused["ok"], false);
    assert!(refused["error"]
        .as_str()
        .unwrap()
        .contains("No overview to continue from"));
    // A refused request changes nothing; places given explicitly are still used as given.
    let explicit = session_answer(
        &mut session,
        &json!({"op": "overview", "graph": changed, "start": {"nodes": places(&first)}}),
    );
    assert_eq!(explicit, by_id);
}

#[test]
fn a_start_by_index_after_a_continuation_by_id_belongs_to_the_clusters_of_that_answer() {
    let (g, changed) = (graph(), grown());
    let mut session = EngineSession::new();
    let first = session_answer(&mut session, &json!({"op": "overview", "graph": g}));
    let by_id = session_answer(
        &mut session,
        &json!({"op": "overview", "graph": changed, "start": {"nodes": places(&first)}}),
    );
    // Without a graph, x, y and r are those of the session's last answer (its clusters, one more node than the first).
    let next = session_answer(
        &mut session,
        &json!({"op": "overview", "start": {"x": by_id["x"], "y": by_id["y"], "r": by_id["r"]}}),
    );
    assert_eq!(next["ids"], by_id["ids"]);
    assert_eq!(next["x"], by_id["x"]);
    // The arrays of the first answer do not fit: refused, with the lengths.
    let refused = session.run_value(
        &json!({"op": "overview", "start": {"x": first["x"], "y": first["y"], "r": first["r"]}}),
    );
    assert_eq!(refused["ok"], false);
    assert!(refused["error"]
        .as_str()
        .unwrap()
        .contains("must come from the same graph"));
}

#[test]
fn entries_of_start_nodes_refuse_unknown_keys_as_the_typed_place_does() {
    let g = graph();
    let first = answer(&json!({"op": "overview", "graph": g}));
    let mut from = places(&first);
    from["g0"]["radius"] = json!(3);
    let error = refusal(&json!({"op": "overview", "graph": g, "start": {"nodes": from}}));
    assert!(
        error.contains("g0") && error.contains("unknown key: radius"),
        "{error}"
    );
    let typed = serde_json::from_value::<arqlet_model::OverviewPlace>(from["g0"].clone());
    assert!(typed.is_err());
    // Known keys, with a parent or without, are as before.
    answer(&json!({"op": "overview", "graph": g, "start": {"nodes": places(&first)}}));
    let mut with_null = places(&first);
    with_null["g0"]["parent"] = Value::Null;
    answer(&json!({"op": "overview", "graph": g, "start": {"nodes": with_null}}));
    // In a session too, and the session keeps its state.
    let mut session = EngineSession::new();
    session_answer(&mut session, &json!({"op": "overview", "graph": g}));
    let refused =
        session.run_value(&json!({"op": "overview", "graph": g, "start": {"nodes": from}}));
    assert_eq!(refused["ok"], false);
    session_answer(&mut session, &json!({"op": "overview", "start": {}}));
}
