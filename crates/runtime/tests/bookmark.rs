//! The compact bookmark (#151, provisional): `compactBookmark` writes a view's decisions as a short text, `readBookmark`
//! reads it against a snapshot, and `restore` applies what was read.
use arqlet_runtime::dispatch;
use arqlet_runtime::engine_session::EngineSession;
use serde_json::{json, Value};

fn call(request: Value) -> Value {
    serde_json::from_str(&dispatch(&request.to_string())).unwrap()
}

fn ok(request: Value) -> Value {
    let answer = call(request);
    assert_eq!(answer["ok"], true, "{answer}");
    answer
}

fn package(p: usize) -> String {
    format!("services/domain-{p:02}")
}
fn module(p: usize, m: usize) -> String {
    format!("{}/module-{m:02}.ts", package(p))
}
fn function(p: usize, m: usize, f: usize) -> String {
    format!("{}#function-{f:02}", module(p, m))
}

/// A large view, generated without randomness: `packages` packages of `modules` modules of `functions` functions, ids
/// that are paths, calls inside a module, from module to module and across packages.
fn large(packages: usize, modules: usize, functions: usize) -> Value {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let edge = |edges: &mut Vec<Value>, source: String, target: String, kind: &str| {
        let id = format!("call-{}", edges.len());
        edges.push(json!({"id": id, "kind": kind, "source": source, "target": target}));
    };
    for p in 0..packages {
        nodes.push(json!({"id": package(p), "kind": "package", "label": format!("domain {p}")}));
        for m in 0..modules {
            nodes.push(
                json!({"id": module(p, m), "kind": "module", "label": format!("module {m}"),
                "parentId": package(p)}),
            );
            for f in 0..functions {
                let kind = if f % 4 == 3 { "test" } else { "function" };
                nodes.push(
                    json!({"id": function(p, m, f), "kind": kind, "label": format!("function {f}"),
                    "parentId": module(p, m)}),
                );
                if f > 0 {
                    edge(
                        &mut edges,
                        function(p, m, f - 1),
                        function(p, m, f),
                        "calls",
                    );
                }
            }
            if m > 0 {
                edge(
                    &mut edges,
                    function(p, m - 1, functions - 1),
                    function(p, m, 0),
                    "calls",
                );
            }
        }
        if p > 0 {
            edge(
                &mut edges,
                function(p - 1, 0, 1),
                function(p, 0, 0),
                "imports",
            );
            edge(
                &mut edges,
                function(p - 1, 3 % modules, 2),
                function(p, 1, 1),
                "imports",
            );
        }
    }
    json!({"nodes": nodes, "edges": edges})
}

fn import(graph: &Value, options: Value) -> Value {
    ok(json!({"op": "import", "graph": graph, "limit": 10000, "options": options}))["snapshot"]
        .clone()
}

/// A change of `snapshot`.
fn edit(snapshot: &Value, mut request: Value) -> Value {
    request["snapshot"] = snapshot.clone();
    request["expectedRevision"] = snapshot["revision"].clone();
    request["expectedLayoutVersion"] = snapshot["layoutVersion"].clone();
    ok(request)["snapshot"].clone()
}

fn intent(snapshot: &Value) -> Value {
    ok(json!({"op": "intent", "snapshot": snapshot}))["intent"].clone()
}

/// What is drawn: the scene's nodes and routes.
fn drawing(snapshot: &Value) -> (Value, Value) {
    let scene = ok(json!({"op": "scene", "snapshot": snapshot}))["scene"].clone();
    assert!(!scene["nodes"].as_array().unwrap().is_empty());
    (scene["nodes"].clone(), scene["routes"].clone())
}

fn compact(snapshot: &Value, extra: Value) -> String {
    let mut request = json!({"op": "compactBookmark", "snapshot": snapshot});
    for (k, v) in extra.as_object().unwrap() {
        request[k] = v.clone();
    }
    ok(request)["text"].as_str().unwrap().to_string()
}

fn read(snapshot: &Value, text: &str, defaults: Option<&Value>) -> Value {
    let mut request = json!({"op": "readBookmark", "snapshot": snapshot, "text": text});
    if let Some(defaults) = defaults {
        request["defaults"] = defaults.clone();
    }
    call(request)
}

/// The view after the bookmark: read against `snapshot` and restored.
fn opened(snapshot: &Value, text: &str, defaults: Option<&Value>) -> (Value, Value) {
    let answer = read(snapshot, text, defaults);
    assert_eq!(answer["ok"], true, "{answer}");
    let restored = edit(
        snapshot,
        json!({"op": "restore", "intent": answer["bookmark"]["intent"]}),
    );
    (restored, answer)
}

/// The long (JSON) form of the view, as the view controller writes it.
fn long_form(snapshot: &Value) -> Value {
    let digest = ok(json!({"op": "digest", "snapshot": snapshot}))["digest"].clone();
    json!({"version": 1, "digest": digest, "intent": intent(snapshot)})
}

fn read_long(snapshot: &Value, bookmark: &Value) -> Value {
    ok(json!({"op": "readBookmark", "snapshot": snapshot, "bookmark": bookmark}))
}

/// The length of the JSON form as URL text (`encodeBookmark`): base64url without padding.
fn json_form_length(snapshot: &Value, marks: &Value) -> usize {
    let mut bookmark = long_form(snapshot);
    for (k, v) in marks.as_object().unwrap() {
        bookmark[k] = v.clone();
    }
    bookmark.to_string().len().div_ceil(3) * 4
}

/// The large view of the issue: opened, paged, filtered, pinned, with a path highlighted and a node selected.
fn explored(first: &Value) -> (Value, Value) {
    let mut view = first.clone();
    for p in 0..6 {
        for m in 0..2 {
            view = edit(&view, json!({"op": "toggle", "nodeId": module(p, m)}));
        }
    }
    view = edit(&view, json!({"op": "toggle", "nodeId": package(9)}));
    view = edit(
        &view,
        json!({"op": "reveal", "nodeIds": [function(11, 7, 5), function(11, 7, 6)]}),
    );
    view = edit(
        &view,
        json!({"op": "filter", "hide": {"nodeKinds": ["test"],
        "nodes": [function(0, 0, 0), function(1, 1, 2), package(14)]}}),
    );
    view = edit(
        &view,
        json!({"op": "direction", "nodeId": package(2), "direction": "tb"}),
    );
    let marks = json!({
        "highlight": {"kind": "trace", "from": function(0, 0, 1), "to": function(3, 1, 4),
            "init": {"mode": "shortest", "edgeKinds": ["calls", "imports"]}},
        "selection": {"nodeId": function(1, 0, 4)},
    });
    (view, marks)
}

#[test]
fn a_large_explored_view_fits_a_url_and_comes_back_as_it_was() {
    let graph = large(24, 24, 10);
    let options = json!({"levelOfDetail": {"depth": 1}});
    let first = import(&graph, options.clone());
    let defaults = intent(&first);
    assert!(defaults["collapsedIds"].as_array().unwrap().len() >= 400);
    let (view, marks) = explored(&first);

    let mut request = marks.clone();
    request["defaults"] = defaults.clone();
    let text = compact(&view, request.clone());
    let long = json_form_length(&view, &marks);
    println!(
        "JSON form {long} characters, compact {} characters",
        text.len()
    );
    assert!(text.starts_with("b1."), "{text}");
    assert!(text[3..]
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'));
    assert!(long > 20_000, "the JSON form is {long} characters");
    assert!(text.len() < 3000, "{} characters", text.len());
    // The same view gives the same text.
    assert_eq!(compact(&view, request), text);

    // On a fresh import of the same graph, with and without the defaults: they hold nothing but the collapsed containers.
    let again = import(&graph, options);
    for defaults in [Some(&defaults), None] {
        let (restored, answer) = opened(&again, &text, defaults);
        assert_eq!(answer["sameSource"], true, "{answer}");
        assert_eq!(answer["sameDefaults"], true, "{answer}");
        assert_eq!(answer["missing"], json!([]));
        assert_eq!(answer["ignored"], json!([]));
        assert_eq!(answer["bookmark"]["highlight"], marks["highlight"]);
        assert_eq!(answer["bookmark"]["selection"], marks["selection"]);
        assert_eq!(answer["bookmark"]["intent"], intent(&view));
        assert_eq!(intent(&restored), intent(&view));
        assert_eq!(drawing(&restored), drawing(&view));
    }

    // On the view itself, which changed the source's directions: still the same source, and nothing to restore.
    let (restored, answer) = opened(&view, &text, None);
    assert_eq!(answer["sameSource"], true);
    assert_eq!(restored["layoutVersion"], view["layoutVersion"]);
}

#[test]
fn another_revision_reports_what_is_gone_and_restores_the_rest() {
    let graph = large(24, 24, 10);
    let options = json!({"levelOfDetail": {"depth": 1}});
    let first = import(&graph, options.clone());
    let defaults = intent(&first);
    let (view, marks) = explored(&first);
    let mut request = marks.clone();
    request["defaults"] = defaults.clone();
    let text = compact(&view, request);

    // The next revision: an opened module, a revealed function, a hidden one and the path's end are gone, with their
    // edges; a package is new.
    let gone = [
        module(0, 0),
        function(11, 7, 5),
        function(1, 1, 2),
        module(3, 1),
    ];
    let inside = |id: &str| {
        gone.iter()
            .any(|g| id == g || id.starts_with(&format!("{g}#")))
    };
    let mut next = graph.clone();
    next["nodes"]
        .as_array_mut()
        .unwrap()
        .retain(|n| !inside(n["id"].as_str().unwrap()));
    next["edges"].as_array_mut().unwrap().retain(|e| {
        !inside(e["source"].as_str().unwrap()) && !inside(e["target"].as_str().unwrap())
    });
    for node in [
        json!({"id": "services/new", "kind": "package", "label": "new"}),
        json!({"id": "services/new/module", "kind": "module", "label": "m", "parentId": "services/new"}),
        json!({"id": "services/new/module#f", "kind": "function", "label": "f", "parentId": "services/new/module"}),
    ] {
        next["nodes"].as_array_mut().unwrap().push(node);
    }
    let fresh = import(&next, options);
    let (restored, answer) = opened(&fresh, &text, Some(&intent(&fresh)));
    assert_eq!(answer["sameSource"], false);
    assert_eq!(answer["sameDefaults"], false);
    // Every id the bookmark names that the revision does not have: the removed ones, and the nodes that went with a
    // removed module (the hidden function, the path's ends, the selection stay where their module stays).
    assert_eq!(
        answer["missing"],
        json!([
            module(0, 0),
            function(0, 0, 0),
            function(0, 0, 1),
            module(3, 1),
            function(1, 1, 2),
            function(11, 7, 5),
            function(3, 1, 4),
        ]
        .into_iter()
        .collect::<std::collections::BTreeSet<_>>())
    );
    let fields: std::collections::BTreeSet<&str> = answer["ignored"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["field"].as_str().unwrap())
        .collect();
    assert_eq!(
        fields.into_iter().collect::<Vec<_>>(),
        ["collapsed", "hidden", "highlight", "partial"]
    );
    assert!(answer["bookmark"].get("highlight").is_none());
    assert_eq!(answer["bookmark"]["selection"], marks["selection"]);
    // The rest is there: the other modules open, the filter, the direction; the new containers as the revision starts.
    let now = intent(&restored);
    let collapsed: Vec<&str> = now["collapsedIds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|id| id.as_str().unwrap())
        .collect();
    assert!(!collapsed.contains(&module(1, 0).as_str()));
    assert!(collapsed.contains(&"services/new/module"));
    assert!(collapsed.contains(&package(9).as_str()));
    assert_eq!(now["hidden"]["nodes"], json!([package(14)]));
    assert_eq!(now["partial"][module(11, 7)], json!([function(11, 7, 6)]));
    assert_eq!(now["containerDirections"][package(2)], "tb");

    // The JSON form against the same revision: `restore` refuses it whole, `readBookmark` leaves out what is gone.
    let json_form = json!({"version": 1, "digest": ok(json!({"op": "digest", "snapshot": view}))["digest"],
        "intent": intent(&view), "highlight": marks["highlight"], "selection": marks["selection"]});
    let refused = call(
        json!({"op": "restore", "snapshot": fresh, "intent": json_form["intent"],
        "expectedRevision": fresh["revision"], "expectedLayoutVersion": fresh["layoutVersion"]}),
    );
    assert_eq!(refused["ok"], false);
    let read = ok(json!({"op": "readBookmark", "snapshot": fresh, "bookmark": json_form}));
    assert_eq!(read["sameSource"], false);
    assert!(read["missing"]
        .as_array()
        .unwrap()
        .contains(&json!(function(1, 1, 2))));
    edit(
        &fresh,
        json!({"op": "restore", "intent": read["bookmark"]["intent"]}),
    );
    // Against its own graph the JSON form is read whole.
    let own = ok(json!({"op": "readBookmark", "snapshot": view, "bookmark": json_form}));
    assert_eq!(own["sameSource"], true);
    assert_eq!(own["bookmark"]["intent"], intent(&view));
    assert_eq!(own["ignored"], json!([]));
}

#[test]
fn communities_of_the_view_come_back() {
    // A flat graph: ten groups of nodes, tightly linked inside, which communities turn into containers of the view.
    let nodes: Vec<Value> = (0..200)
        .map(|i| json!({"id": format!("n{i:03}"), "kind": "fn", "label": format!("n{i}")}))
        .collect();
    let mut edges = Vec::new();
    for i in 0..200usize {
        for step in [1, 2, 3] {
            let t = i / 20 * 20 + (i % 20 + step) % 20;
            edges.push(
                json!({"id": format!("e{i}-{step}"), "kind": "calls", "source": format!("n{i:03}"),
                "target": format!("n{t:03}")}),
            );
        }
        if i % 20 == 0 {
            edges.push(
                json!({"id": format!("x{i}"), "kind": "calls", "source": format!("n{i:03}"),
                "target": format!("n{:03}", (i + 20) % 200)}),
            );
        }
    }
    let graph = json!({"nodes": nodes, "edges": edges});
    let options = json!({"communities": 12});
    let first = import(&graph, options.clone());
    let defaults = intent(&first);
    let community = defaults["collapsedIds"][0].as_str().unwrap().to_string();
    assert!(
        !graph["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n["id"] == community.as_str()),
        "a container only the view has"
    );
    let view = edit(&first, json!({"op": "toggle", "nodeId": community}));
    // A step back over it: `restore` takes the containers the view adds.
    let back = edit(&view, json!({"op": "restore", "intent": defaults}));
    assert_eq!(intent(&back), defaults);
    let paged = edit(
        &first,
        json!({"op": "reveal", "more": {"container": community, "count": 3}}),
    );
    for view in [view, paged] {
        let text = compact(
            &view,
            json!({"defaults": defaults, "selection": {"nodeId": community}}),
        );
        assert!(text.len() < 600, "{} characters", text.len());
        let again = import(&graph, options.clone());
        let (restored, answer) = opened(&again, &text, Some(&defaults));
        assert_eq!(answer["missing"], json!([]), "{answer}");
        assert_eq!(answer["ignored"], json!([]), "{answer}");
        assert_eq!(intent(&restored), intent(&view));
        assert_eq!(drawing(&restored), drawing(&view));
    }
}

/// A graph that declares shapes and a port on every node.
fn authored() -> Value {
    let mut nodes = vec![json!({"id": "box", "kind": "unit", "label": "box"})];
    let mut ports = Vec::new();
    let mut edges = Vec::new();
    let mut bindings = Vec::new();
    for i in 0..60 {
        let id = format!("part-{i:02}");
        nodes.push(
            json!({"id": id, "kind": "unit", "label": id, "parentId": "box", "shape": "square"}),
        );
        ports
            .push(json!({"id": format!("{id}.out"), "nodeId": id, "side": "right", "offset": 0.5}));
        if i > 0 {
            let edge = format!("wire-{i:02}");
            let from = format!("part-{:02}", i - 1);
            edges.push(json!({"id": edge, "kind": "wire", "source": from, "target": id}));
            bindings.push(
                json!({"edgeId": edge, "sourcePort": format!("{from}.out"), "targetPort": null}),
            );
        }
    }
    json!({"nodes": nodes, "edges": edges, "ports": ports, "edgePorts": bindings})
}

#[test]
fn what_the_graph_declares_stays_out_when_the_defaults_are_given() {
    let graph = authored();
    let first = import(&graph, json!({}));
    let defaults = intent(&first);
    // A shape, a port placed by hand, a pin.
    let mut view = edit(
        &first,
        json!({"op": "shape", "nodeId": "part-03", "shape": "circle"}),
    );
    view = edit(
        &view,
        json!({"op": "port", "edgeId": "wire-07", "endpoint": "target", "side": "top", "offset": 0.25}),
    );
    view = edit(
        &view,
        json!({"op": "pin", "nodeId": "part-10", "position": {"x": 40.5, "y": 12.25}}),
    );
    let relative = compact(&view, json!({"defaults": defaults}));
    let absolute = compact(&view, json!({}));
    println!(
        "relative {} characters, absolute {} characters",
        relative.len(),
        absolute.len()
    );
    assert!(relative.len() < 700, "{}", relative.len());
    assert!(absolute.len() > 3 * relative.len());

    let again = import(&graph, json!({}));
    for (text, given) in [
        (&relative, Some(&defaults)),
        (&absolute, None),
        (&absolute, Some(&defaults)),
    ] {
        let (restored, answer) = opened(&again, text, given);
        assert_eq!(answer["sameSource"], true, "{answer}");
        assert_eq!(answer["ignored"], json!([]), "{answer}");
        assert_eq!(intent(&restored), intent(&view));
        assert_eq!(drawing(&restored), drawing(&view));
    }
    // Without the defaults it cannot be read, and says so.
    let refused = read(&again, &relative, None);
    assert_eq!(refused["ok"], false);
    assert!(refused["error"].as_str().unwrap().contains("defaults"));

    // The next revision declares another shape: it applies, beside the user's changes.
    let mut next = graph.clone();
    next["nodes"][1]["shape"] = json!("circle");
    next["nodes"][11]["id"] = json!("part-10b");
    next["edges"]
        .as_array_mut()
        .unwrap()
        .retain(|e| e["source"] != "part-10" && e["target"] != "part-10");
    next["ports"]
        .as_array_mut()
        .unwrap()
        .retain(|p| p["nodeId"] != "part-10");
    next["edgePorts"]
        .as_array_mut()
        .unwrap()
        .retain(|b| b["edgeId"] != "wire-10" && b["edgeId"] != "wire-11");
    let fresh = import(&next, json!({}));
    let (restored, answer) = opened(&fresh, &relative, Some(&intent(&fresh)));
    assert_eq!(answer["sameSource"], false);
    assert_eq!(answer["sameDefaults"], false);
    assert_eq!(answer["missing"], json!(["part-10"]), "{answer}");
    let now = intent(&restored);
    assert_eq!(now["shapes"]["part-00"], "circle", "the revision's");
    assert_eq!(now["shapes"]["part-03"], "circle", "the user's");
    assert_eq!(now["pins"], json!({}));
    assert!(now["edgePorts"]
        .as_array()
        .unwrap()
        .iter()
        .any(|b| b["edgeId"] == "wire-07" && b["targetPort"].is_string()));
}

#[test]
fn the_operations_run_in_a_session_and_refuse_what_they_cannot_read() {
    let graph = large(4, 4, 4);
    let options = json!({"levelOfDetail": {"depth": 1}});
    let first = import(&graph, options);
    let view = edit(&first, json!({"op": "toggle", "nodeId": module(0, 0)}));
    let request = json!({"selection": {"edgeId": "call-0"}, "highlight":
        {"kind": "neighbourhood", "nodeId": [function(0, 0, 0), function(0, 0, 1)], "init": {"hops": 2}}});
    let text = compact(&view, request.clone());
    let mut session = EngineSession::new();
    session.load(serde_json::from_value(view.clone()).unwrap());
    let mut resident = request.clone();
    resident["op"] = json!("compactBookmark");
    assert_eq!(session.run_value(&resident)["text"], text.as_str());
    let answer = session.run_value(&json!({"op": "readBookmark", "text": text}));
    assert_eq!(answer, read(&view, &text, None));
    assert_eq!(answer["bookmark"]["highlight"], request["highlight"]);
    assert_eq!(answer["bookmark"]["selection"], request["selection"]);

    for (bad, message) in [
        ("", "Not a compact bookmark"),
        ("eyJ2ZXJzaW9uIjoxfQ", "Not a compact bookmark"),
        ("b2.eyJ9", "version 2 is not supported"),
        ("b1.!!!", "damaged"),
        (&text[..text.len() / 2], "damaged"),
        // `{"s":"","d":"","c":{"o":[7]}}`: a place the table does not have.
        ("b1.eyJzIjoiIiwiZCI6IiIsImMiOnsibyI6WzddfX0", "damaged"),
    ] {
        let refused = read(&view, bad, None);
        assert_eq!(refused["ok"], false, "{bad}: {refused}");
        assert!(
            refused["error"].as_str().unwrap().contains(message),
            "{bad}: {refused}"
        );
    }
    assert_eq!(
        call(json!({"op": "readBookmark", "snapshot": view}))["ok"],
        false
    );
    assert_eq!(
        call(json!({"op": "compactBookmark", "snapshot": view, "selection": {"node": "a"}}))["ok"],
        false
    );
}

/// [`authored`] with declared directions, strategy and routing too.
fn declared() -> Value {
    let mut graph = authored();
    graph["defaultDirection"] = json!("tb");
    graph["containerDirections"] = json!({"box": "lr"});
    graph["containerStrategies"] = json!({"box": "none"});
    graph["defaultRouting"] = json!("orthogonal");
    graph
}

/// One change of each kind that is written into the view's source graph.
fn source_changes() -> Vec<(&'static str, Value)> {
    vec![
        (
            "shape",
            json!({"op": "shape", "nodeId": "part-03", "shape": "circle"}),
        ),
        (
            "default direction",
            json!({"op": "direction", "direction": "bt"}),
        ),
        (
            "container direction",
            json!({"op": "direction", "nodeId": "box", "direction": "rl"}),
        ),
        (
            "default strategy",
            json!({"op": "strategy", "strategy": ["none", "tree"]}),
        ),
        (
            "container strategy",
            json!({"op": "strategy", "nodeId": "box", "strategy": "tree"}),
        ),
        (
            "default routing",
            json!({"op": "routing", "routing": "straight"}),
        ),
        (
            "container routing",
            json!({"op": "routing", "nodeId": "box", "routing": "polyline"}),
        ),
        (
            "port",
            json!({"op": "port", "edgeId": "wire-07", "endpoint": "target", "side": "top", "offset": 0.3}),
        ),
    ]
}

#[test]
fn a_long_bookmark_of_changed_decisions_belongs_to_a_fresh_import() {
    for graph in [authored(), declared()] {
        let first = import(&graph, json!({}));
        let mut all = first.clone();
        let mut views = Vec::new();
        for (name, change) in source_changes() {
            views.push((name, edit(&first, change.clone())));
            all = edit(&all, change);
        }
        views.push(("all", all));
        // A declared value taken back is a change too.
        if graph.get("containerDirections").is_some() {
            views.push((
                "inherited direction",
                edit(&first, json!({"op": "direction", "nodeId": "box"})),
            ));
        }
        for (name, view) in views {
            assert_ne!(intent(&view), intent(&first), "{name}");
            let bookmark = long_form(&view);
            for (on, target) in [
                ("a fresh import", import(&graph, json!({}))),
                ("the view", view.clone()),
            ] {
                let read = read_long(&target, &bookmark);
                assert_eq!(read["sameSource"], true, "{name} on {on}");
                assert_eq!(read["missing"], json!([]), "{name} on {on}");
                assert_eq!(read["ignored"], json!([]), "{name} on {on}");
                assert_eq!(read["bookmark"]["intent"], intent(&view), "{name} on {on}");
                let restored = edit(
                    &target,
                    json!({"op": "restore", "intent": bookmark["intent"]}),
                );
                assert_eq!(intent(&restored), intent(&view), "{name} on {on}");
                assert_eq!(long_form(&restored), bookmark, "{name} on {on}");
            }
            // Resident, as the view controller asks.
            let mut session = EngineSession::new();
            session.load(serde_json::from_value(import(&graph, json!({}))).unwrap());
            let resident = session.run_value(&json!({"op": "readBookmark", "bookmark": bookmark}));
            assert_eq!(resident["sameSource"], true, "{name}: {resident}");
        }
    }
}

#[test]
fn a_long_bookmark_of_another_graph_stays_one() {
    let graph = declared();
    let first = import(&graph, json!({}));
    let mut changed = first.clone();
    for (_, change) in source_changes() {
        changed = edit(&changed, change);
    }
    // What no decision writes: a label, a kind, a parent, an edge's direction, a node and an edge more or less.
    type Revision = fn(&mut Value);
    let revisions: [(&str, Revision); 6] = [
        ("label", |g| g["nodes"][5]["label"] = json!("renamed")),
        ("kind", |g| g["nodes"][5]["kind"] = json!("other")),
        ("parent", |g| g["nodes"][5]["parentId"] = Value::Null),
        ("edge direction", |g| {
            let edge = &mut g["edges"][20];
            let (source, target) = (edge["source"].clone(), edge["target"].clone());
            edge["source"] = target;
            edge["target"] = source;
            // Its port belongs to the former source.
            g["edgePorts"].as_array_mut().unwrap().remove(20);
        }),
        ("node more", |g| {
            g["nodes"]
                .as_array_mut()
                .unwrap()
                .push(json!({"id": "new", "kind": "unit", "label": "new"}))
        }),
        ("edge less", |g| {
            g["edges"].as_array_mut().unwrap().pop();
            g["edgePorts"].as_array_mut().unwrap().pop();
        }),
    ];
    for (name, revise) in revisions {
        let mut next = graph.clone();
        revise(&mut next);
        let fresh = import(&next, json!({}));
        for bookmark in [long_form(&first), long_form(&changed)] {
            assert_eq!(read_long(&fresh, &bookmark)["sameSource"], false, "{name}");
        }
    }
    // A digest of something else, with the decisions of this graph.
    let mut bookmark = long_form(&changed);
    bookmark["digest"] = json!("0000000000000000");
    assert_eq!(read_long(&first, &bookmark)["sameSource"], false);
}

#[test]
fn declared_values_do_not_make_another_source_in_either_form() {
    // As the compact form: a revision that differs only in what a decision writes is the same source, and a long
    // bookmark restores its own values over the revision's.
    let graph = declared();
    let view = edit(
        &import(&graph, json!({})),
        json!({"op": "shape", "nodeId": "part-03", "shape": "circle"}),
    );
    let mut next = graph.clone();
    next["nodes"][1]["shape"] = json!("circle");
    next["containerDirections"] = json!({"box": "bt"});
    let fresh = import(&next, json!({}));
    let text = compact(&view, json!({}));
    assert_eq!(read(&fresh, &text, None)["sameSource"], true);
    let bookmark = long_form(&view);
    assert_eq!(read_long(&fresh, &bookmark)["sameSource"], true);
    let restored = edit(
        &fresh,
        json!({"op": "restore", "intent": bookmark["intent"]}),
    );
    assert_eq!(intent(&restored), intent(&view));
}

#[test]
fn a_long_bookmark_is_read_against_a_snapshot_that_left_its_source_out() {
    let graph = declared();
    let lean = ok(json!({"op": "import", "graph": graph, "limit": 10000, "omitSource": true}))
        ["snapshot"]
        .clone();
    assert!(lean["sourceDigest"].is_string() && lean["sourceGraph"].is_null());
    let view = edit(
        &import(&graph, json!({})),
        json!({"op": "shape", "nodeId": "part-03", "shape": "circle"}),
    );
    let bookmark = long_form(&view);
    let read =
        ok(json!({"op": "readBookmark", "snapshot": lean, "source": graph, "bookmark": bookmark}));
    assert_eq!(read["sameSource"], true, "{read}");
}
