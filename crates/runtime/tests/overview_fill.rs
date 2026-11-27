//! Filling given containers of an overview later (#235, ADR 0018 addendum of 2026-11-27): `start.fill` arranges the
//! interior of named containers anew inside their discs and keeps every other place as it was given, bit for bit; it
//! works within a work budget, in a session as through `dispatch`, and for any thread count.
use arqlet_runtime::dispatch;
use arqlet_runtime::engine_session::EngineSession;
use serde_json::{json, Value};

/// A hierarchical graph: `groups` directories of `files` files with `leaves` leaves each, a ring and chords in every
/// file, edges between neighbouring files and directories, and a cycle, a self-loop, a parallel and an unresolved edge.
fn graph(groups: usize, files: usize, leaves: usize) -> Value {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    for g in 0..groups {
        nodes.push(json!({"id": format!("g{g}"), "kind": "dir", "label": format!("g{g}")}));
        for f in 0..files {
            let file = format!("g{g}/f{f}");
            nodes.push(
                json!({"id": file, "kind": "file", "label": file, "parentId": format!("g{g}")}),
            );
            for l in 0..leaves {
                nodes.push(json!({"id": format!("{file}/n{l}"), "kind": "fn", "label": "n", "parentId": file}));
                for d in [1, 7] {
                    edges.push(json!({"id": format!("{file}/e{l}-{d}"), "kind": "call",
                        "source": format!("{file}/n{l}"), "target": format!("{file}/n{}", (l + d) % leaves)}));
                }
            }
            if f > 0 {
                edges.push(json!({"id": format!("{file}/up"), "kind": "call", "source": format!("{file}/n0"),
                    "target": format!("g{g}/f{}/n1", f - 1)}));
            }
        }
        edges.push(
            json!({"id": format!("g{g}/x"), "kind": "call", "source": format!("g{g}/f0/n0"),
            "target": format!("g{}/f0/n2", (g + 1) % groups)}),
        );
    }
    edges.push(json!({"id": "self", "kind": "call", "source": "g0/f0/n0", "target": "g0/f0/n0"}));
    edges.push(
        json!({"id": "parallel", "kind": "call", "source": "g0/f0/n0", "target": "g0/f0/n1"}),
    );
    edges.push(json!({"id": "open", "kind": "call", "source": "g0/f0/n0", "target": null}));
    json!({"nodes": nodes, "edges": edges})
}

fn run(request: &Value) -> Value {
    serde_json::from_str(&dispatch(&request.to_string())).unwrap()
}

fn answer(request: &Value) -> Value {
    let response = run(request);
    assert_eq!(response["ok"], true, "{response}");
    response["overview"].clone()
}

fn options() -> Value {
    json!({"communities": 20})
}

/// No overlaps between the children of one container, every child inside its parent's disc.
fn assert_valid(o: &Value) {
    let n = o["ids"].as_array().unwrap().len();
    let f = |k: &str, i: usize| o[k][i].as_f64().unwrap();
    let distance = |i: usize, j: usize| (f("x", i) - f("x", j)).hypot(f("y", i) - f("y", j));
    assert_eq!(o["stats"]["overlaps"], 0);
    let mut children: std::collections::BTreeMap<i64, Vec<usize>> = Default::default();
    for i in 0..n {
        let p = o["parent"][i].as_i64().unwrap();
        children.entry(p).or_default().push(i);
        if p >= 0 {
            assert!(
                distance(i, p as usize) + f("r", i) <= f("r", p as usize) + 0.05,
                "{} leaves its container",
                o["ids"][i]
            );
        }
    }
    for list in children.values() {
        for (a, &i) in list.iter().enumerate() {
            for &j in &list[a + 1..] {
                assert!(
                    distance(i, j) >= f("r", i) + f("r", j) - 0.05,
                    "{} and {} overlap",
                    o["ids"][i],
                    o["ids"][j]
                );
            }
        }
    }
}

/// Whether node `i` lies strictly inside container `c` (-1: the top level holds every node).
fn inside(o: &Value, i: usize, c: i64) -> bool {
    let mut at = o["parent"][i].as_i64().unwrap();
    while at >= 0 {
        if at == c {
            return true;
        }
        at = o["parent"][at as usize].as_i64().unwrap();
    }
    c == -1
}

/// Asserts that every node outside the containers `filled` has the same `x`, `y` and `r` in both answers, bit for bit,
/// and that every filled container kept its radius (the top level excepted).
fn assert_kept_outside(before: &Value, after: &Value, filled: &[i64]) {
    let n = before["ids"].as_array().unwrap().len();
    assert_eq!(after["ids"], before["ids"]);
    for i in 0..n {
        if filled.iter().any(|&c| inside(before, i, c)) {
            continue;
        }
        for k in ["x", "y", "r"] {
            assert_eq!(
                after[k][i].as_f64().unwrap().to_bits(),
                before[k][i].as_f64().unwrap().to_bits(),
                "{k} of {} changed",
                before["ids"][i]
            );
        }
    }
}

fn start_of(o: &Value) -> Value {
    json!({"x": o["x"], "y": o["y"], "r": o["r"]})
}

/// The fill fields of a start: node ids, and -1 as `fillTop`.
fn with_fill(mut start: Value, o: &Value, containers: &[i64]) -> Value {
    let ids: Vec<Value> = containers
        .iter()
        .filter(|&&c| c >= 0)
        .map(|&c| o["ids"][c as usize].clone())
        .collect();
    start["fill"] = json!(ids);
    if containers.contains(&-1) {
        start["fillTop"] = json!(true);
    }
    start
}

/// A budgeted overview with containers packed and the clustering complete (so that a start by index fits).
fn packed_overview(g: &Value) -> Value {
    let full = answer(
        &json!({"op": "overview", "graph": g, "options": options(), "workBudget": u64::MAX}),
    );
    let total = full["work"]["used"].as_u64().unwrap();
    let clustering = full["work"]["clustering"].as_u64().unwrap();
    let budget = clustering + (total - clustering) * 3 / 10;
    let o =
        answer(&json!({"op": "overview", "graph": g, "options": options(), "workBudget": budget}));
    assert_eq!(o["work"]["unclustered"], json!([]), "{}", o["work"]);
    assert!(
        o["work"]["packed"].as_array().unwrap().len() > 3,
        "{}",
        o["work"]
    );
    assert_valid(&o);
    o
}

fn radius_of(o: &Value, c: i64) -> f64 {
    if c < 0 {
        o["rootRadius"].as_f64().unwrap()
    } else {
        o["r"][c as usize].as_f64().unwrap()
    }
}

#[test]
fn filling_every_packed_container_one_by_one_keeps_everything_else_bit_for_bit() {
    let g = graph(3, 4, 30);
    let mut o = packed_overview(&g);
    // The smallest first, the top level last.
    let mut packed: Vec<i64> = o["work"]["packed"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_i64().unwrap())
        .collect();
    packed.sort_by(|&a, &b| {
        (a < 0)
            .cmp(&(b < 0))
            .then(radius_of(&o, a).total_cmp(&radius_of(&o, b)))
            .then(a.cmp(&b))
    });
    for &c in &packed {
        let request = json!({"op": "overview", "graph": g, "options": options(),
            "start": with_fill(start_of(&o), &o, &[c])});
        let next = answer(&request);
        assert_valid(&next);
        let fill = &next["fill"];
        assert_eq!(fill["filled"], json!([c]), "{c}: {fill}");
        assert_eq!(fill["unfit"], json!([]));
        assert!(fill.get("ignored").is_none());
        if c >= 0 {
            assert_kept_outside(&o, &next, &[c]);
        } else {
            // The top level arranged anew: its children keep their discs, and what they hold moves with them.
            let n = o["ids"].as_array().unwrap().len();
            for i in 0..n {
                assert_eq!(next["r"][i], o["r"][i], "{}", o["ids"][i]);
                let p = o["parent"][i].as_i64().unwrap();
                if p >= 0 {
                    for k in ["x", "y"] {
                        let was = o[k][i].as_f64().unwrap() - o[k][p as usize].as_f64().unwrap();
                        let now =
                            next[k][i].as_f64().unwrap() - next[k][p as usize].as_f64().unwrap();
                        assert!(
                            (was - now).abs() <= 0.02,
                            "{} moved in its container",
                            o["ids"][i]
                        );
                    }
                }
            }
        }
        // The same for any number of threads.
        let mut threaded = request.clone();
        threaded["threads"] = json!(4);
        assert_eq!(answer(&threaded), next, "{c}");
        o = next;
    }
    // Filling arranges the children more tightly than rows: the discs keep slack, never overflow.
    assert_valid(&o);
}

#[test]
fn a_budget_fills_as_much_as_fits_in_a_fixed_order_and_lists_the_rest_as_packed() {
    let g = graph(3, 4, 30);
    let mut o = packed_overview(&g);
    let all: Vec<i64> = o["work"]["packed"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_i64().unwrap())
        .filter(|&c| c >= 0)
        .collect();
    // Every container once with the whole work: what one request filling all of them costs.
    let whole = answer(&json!({"op": "overview", "graph": g, "options": options(),
        "start": with_fill(start_of(&o), &o, &all), "workBudget": u64::MAX}));
    assert_eq!(whole["fill"]["filled"], json!(all));
    let cost = whole["work"]["used"].as_u64().unwrap();
    let mut left = all.clone();
    let mut rounds = 0;
    while !left.is_empty() {
        rounds += 1;
        assert!(rounds < 20, "the fill does not end: {left:?}");
        let request = json!({"op": "overview", "graph": g, "options": options(),
            "start": with_fill(start_of(&o), &o, &left), "workBudget": cost / 3});
        let next = answer(&request);
        assert_valid(&next);
        let work = &next["work"];
        assert!(work["used"].as_u64().unwrap() <= cost / 3, "{work}");
        let filled: Vec<i64> = next["fill"]["filled"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c.as_i64().unwrap())
            .collect();
        let packed: Vec<i64> = work["packed"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c.as_i64().unwrap())
            .collect();
        // Every container named is either filled or listed as packed, never both; nothing else is packed.
        let mut both: Vec<i64> = filled.iter().chain(&packed).copied().collect();
        both.sort_unstable();
        assert_eq!(both, left, "{work}");
        assert!(!filled.is_empty(), "no progress: {work}");
        assert_kept_outside(&o, &next, &filled);
        for threads in [2, 4] {
            let mut threaded = request.clone();
            threaded["threads"] = json!(threads);
            assert_eq!(answer(&threaded), next, "threads {threads}");
        }
        left = packed;
        o = next;
    }
    assert!(rounds > 1, "the budget left nothing out");
    // Filled in parts or at once: the same, up to the rounding of the places a later part starts from.
    for k in ["x", "y", "r"] {
        for (a, b) in o[k]
            .as_array()
            .unwrap()
            .iter()
            .zip(whole[k].as_array().unwrap())
        {
            assert!(
                (a.as_f64().unwrap() - b.as_f64().unwrap()).abs() <= 0.02,
                "{k}: {a} {b}"
            );
        }
    }
}

#[test]
fn a_filled_container_is_arranged_as_a_layout_without_start_inside_its_disc() {
    let g = graph(2, 3, 25);
    let plain = answer(&json!({"op": "overview", "graph": g, "options": options()}));
    let ids = plain["ids"].as_array().unwrap();
    let file = ids.iter().position(|id| id == "g1/f2").unwrap() as i64;
    let next = answer(&json!({"op": "overview", "graph": g, "options": options(),
        "start": with_fill(start_of(&plain), &plain, &[file])}));
    assert_eq!(next["fill"]["filled"], json!([file]));
    // The same arrangement the layout gave it, up to the rounding of the start.
    for i in 0..ids.len() {
        for k in ["x", "y", "r"] {
            let (a, b) = (plain[k][i].as_f64().unwrap(), next[k][i].as_f64().unwrap());
            assert!((a - b).abs() <= 0.02, "{k} of {}: {a} {b}", ids[i]);
        }
    }
    assert_kept_outside(&plain, &next, &[file]);
}

#[test]
fn a_container_that_does_not_fit_its_disc_keeps_its_places_and_grows_with_radii() {
    let g = graph(2, 3, 25);
    let o = packed_overview(&g);
    let ids = o["ids"].as_array().unwrap();
    let file = ids.iter().position(|id| id == "g1/f2").unwrap();
    let id = ids[file].as_str().unwrap();
    // A disc far too small for what it holds.
    let tiny = answer(&json!({"op": "overview", "graph": g, "options": options(),
        "radii": {id: 5.}, "start": with_fill(start_of(&o), &o, &[file as i64])}));
    let fill = &tiny["fill"];
    assert_eq!(fill["filled"], json!([]));
    assert_eq!(fill["unfit"], json!([file]));
    let needed = fill["needed"][0].as_f64().unwrap();
    assert!(needed > 5., "{fill}");
    // As without fill: its children kept their places in it.
    for i in (0..ids.len()).filter(|&i| inside(&o, i, file as i64)) {
        for k in ["x", "y"] {
            let was = o[k][i].as_f64().unwrap() - o[k][file].as_f64().unwrap();
            let now = tiny[k][i].as_f64().unwrap() - tiny[k][file].as_f64().unwrap();
            assert!(
                (was - now).abs() <= 0.02,
                "{} moved in its container",
                ids[i]
            );
        }
    }
    // With the radius it needs, it is filled and its neighbours make room.
    let grown = answer(&json!({"op": "overview", "graph": g, "options": options(),
        "radii": {id: needed}, "start": with_fill(start_of(&o), &o, &[file as i64])}));
    assert_eq!(grown["fill"]["filled"], json!([file]));
    assert_eq!(grown["r"][file].as_f64().unwrap(), needed);
    assert_valid(&grown);
}

#[test]
fn a_session_fills_as_dispatch_does() {
    let g = graph(3, 4, 30);
    let o = packed_overview(&g);
    let budget = o["work"]["budget"].clone();
    let packed: Vec<i64> = o["work"]["packed"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_i64().unwrap())
        .filter(|&c| c >= 0)
        .collect();
    let mut session = EngineSession::new();
    let first = session.run_value(
        &json!({"op": "overview", "graph": g, "options": options(), "workBudget": budget}),
    );
    assert_eq!(first["overview"], o);
    let mut last = o.clone();
    for (step, &c) in packed.iter().take(3).enumerate() {
        let id = &o["ids"][c as usize];
        let request = json!({"op": "overview", "options": options(), "start": {"fill": [id]},
            "positionsOnly": step == 1, "threads": 2});
        let s = session.run_value(&request);
        assert_eq!(s["ok"], true, "{s}");
        let d = answer(&json!({"op": "overview", "graph": g, "options": options(),
            "start": with_fill(start_of(&last), &last, &[c])}));
        if step == 1 {
            // Positions only: what moves, with the fill report.
            assert_eq!(s["overview"]["positionsOnly"], true);
            for k in ["x", "y", "r", "fill", "rootRadius"] {
                assert_eq!(s["overview"][k], d[k], "{k}");
            }
        } else {
            assert_eq!(s["overview"], d, "step {step}");
        }
        last = d;
    }
    // The columnar answer carries the report in its metadata.
    let id = &o["ids"][packed[3] as usize];
    let columns = session.run_value(&json!({"op": "overview", "options": options(),
        "start": {"fill": [id, "nobody"]}, "positionsOnly": true, "columns": true}));
    assert_eq!(
        columns["overview"]["fill"]["filled"],
        json!([packed[3]]),
        "{columns}"
    );
    assert_eq!(columns["overview"]["fill"]["ignored"], json!(["nobody"]));
}

#[test]
fn fill_by_id_and_bad_fills() {
    let g = graph(2, 3, 25);
    let o = packed_overview(&g);
    let n = o["ids"].as_array().unwrap().len();
    let places: serde_json::Map<String, Value> = (0..n)
        .map(|i| {
            let mut place = json!({"x": o["x"][i], "y": o["y"][i], "r": o["r"][i]});
            let p = o["parent"][i].as_i64().unwrap();
            if p >= 0 {
                place["parent"] = o["ids"][p as usize].clone();
            }
            (o["ids"][i].as_str().unwrap().to_string(), place)
        })
        .collect();
    // By id: the same as by index.
    let by_id = answer(&json!({"op": "overview", "graph": g, "options": options(),
        "start": {"nodes": places, "fill": ["g1", "g0/f0/n3", "gone"]}}));
    let by_index = answer(&json!({"op": "overview", "graph": g, "options": options(),
        "start": {"x": o["x"], "y": o["y"], "r": o["r"], "fill": ["g1", "g0/f0/n3", "gone"]}}));
    assert_eq!(by_id["x"], by_index["x"]);
    assert_eq!(by_id["r"], by_index["r"]);
    let g1 = (0..n).find(|&i| o["ids"][i] == "g1").unwrap();
    assert_eq!(by_index["fill"]["filled"], json!([g1]));
    // A leaf and an unknown id are no containers: left out and listed.
    assert_eq!(by_index["fill"]["ignored"], json!(["g0/f0/n3", "gone"]));
    assert_valid(&by_index);
    // Refused: not a list, a moved node inside a filled container, fill without earlier places.
    for (start, part) in [
        (
            json!({"x": o["x"], "y": o["y"], "r": o["r"], "fill": "g1"}),
            "fill must be a list",
        ),
        (
            json!({"x": o["x"], "y": o["y"], "r": o["r"], "fill": [1]}),
            "fill must be a list",
        ),
        (
            json!({"x": o["x"], "y": o["y"], "r": o["r"], "fillTop": 1}),
            "fillTop",
        ),
        (
            json!({"x": o["x"], "y": o["y"], "r": o["r"], "fill": ["g1"], "moved": {"g1/f0/n2": {"x": 0., "y": 0.}}}),
            "inside a container it fills",
        ),
        (
            json!({"x": o["x"], "y": o["y"], "r": o["r"], "fillTop": true, "moved": {"g1": {"x": 0., "y": 0.}}}),
            "inside a container it fills",
        ),
        (json!({"fill": ["g1"]}), "start needs x"),
    ] {
        let response =
            run(&json!({"op": "overview", "graph": g, "options": options(), "start": start}));
        assert_eq!(response["ok"], false, "{start}");
        assert!(
            response["error"].as_str().unwrap().contains(part),
            "{response}"
        );
    }
    // A moved node outside the filled containers is applied as before.
    let moved = answer(&json!({"op": "overview", "graph": g, "options": options(),
        "start": {"x": o["x"], "y": o["y"], "r": o["r"], "fill": ["g1"],
            "moved": {"g0": {"x": o["x"][0], "y": o["y"][0]}}}}));
    assert_valid(&moved);
    // The typed contract.
    let typed: arqlet_model::Overview = serde_json::from_value(by_index.clone()).expect("contract");
    assert_eq!(typed.fill.unwrap().filled, vec![g1 as i64]);
    let request = arqlet_model::OverviewRequest {
        start: Some(arqlet_model::OverviewStart {
            x: serde_json::from_value(o["x"].clone()).unwrap(),
            y: serde_json::from_value(o["y"].clone()).unwrap(),
            r: serde_json::from_value(o["r"].clone()).unwrap(),
            fill: Some(vec!["g1".into()]),
            fill_top: Some(false),
            ..Default::default()
        }),
        ..Default::default()
    };
    let mut v = serde_json::to_value(&request).unwrap();
    v["graph"] = g.clone();
    v["options"] = options();
    v["op"] = json!("overview");
    assert_eq!(answer(&v)["fill"]["filled"], json!([g1]));
}

#[test]
fn without_fill_nothing_changes() {
    let g = graph(2, 3, 25);
    let o = packed_overview(&g);
    let plain =
        answer(&json!({"op": "overview", "graph": g, "options": options(), "start": start_of(&o)}));
    assert!(plain.get("fill").is_none());
    let null = answer(&json!({"op": "overview", "graph": g, "options": options(),
        "start": {"x": o["x"], "y": o["y"], "r": o["r"], "fill": null, "fillTop": null}}));
    assert_eq!(null, plain);
}
