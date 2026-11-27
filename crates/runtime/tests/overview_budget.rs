//! The work budget of the `overview` operation (#211, ADR 0018 addendum of 2026-11-27): deterministic, the same for any
//! thread count, a valid answer when it runs out (or a clean refusal), and no change without it.
use arqlet_runtime::dispatch;
use arqlet_runtime::engine_session::EngineSession;
use serde_json::{json, Value};

/// A hierarchical graph with large flat containers, so that both clustering and arranging cost something: `groups`
/// top-level directories of `files` files with `leaves` leaves each, a ring and chords in every file, edges between
/// neighbouring files, and a cycle, a self-loop, a parallel edge and an unresolved edge.
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

/// An answer without its work report.
fn without_work(mut o: Value) -> Value {
    o.as_object_mut().unwrap().remove("work");
    o
}

const OPTIONS: &str = r#"{"communities": 20}"#;

fn options() -> Value {
    serde_json::from_str(OPTIONS).unwrap()
}

#[test]
fn without_a_budget_nothing_changes_and_a_large_one_reports_the_whole_work() {
    let g = graph(3, 4, 30);
    let plain = answer(&json!({"op": "overview", "graph": g, "options": options()}));
    assert!(plain.get("work").is_none(), "no report without a budget");
    let huge = answer(
        &json!({"op": "overview", "graph": g, "options": options(), "workBudget": 9007199254740991u64}),
    );
    let work = huge["work"].clone();
    assert_eq!(without_work(huge), plain, "within budget, the same answer");
    assert_eq!(work["limited"], false);
    assert_eq!(work["budget"], 9007199254740991u64);
    let used = work["used"].as_u64().unwrap();
    let clustering = work["clustering"].as_u64().unwrap();
    assert!(used > clustering && clustering > 0, "{work}");
    assert_eq!(work["unclustered"], json!([]));
    assert_eq!(work["packed"], json!([]));
    // Exactly the work used is enough.
    let exact =
        answer(&json!({"op": "overview", "graph": g, "options": options(), "workBudget": used}));
    assert_eq!(exact["work"]["limited"], false);
    assert_eq!(without_work(exact), plain);
}

#[test]
fn a_budget_that_runs_out_answers_valid_discs_and_says_what_it_left_out() {
    let g = graph(3, 4, 30);
    let full = answer(
        &json!({"op": "overview", "graph": g, "options": options(), "workBudget": u64::MAX}),
    );
    let total = full["work"]["used"].as_u64().unwrap();
    let clustering = full["work"]["clustering"].as_u64().unwrap();
    let mut seen_unclustered = false;
    let mut seen_packed = false;
    for share in [0, 1, 5, 20, 40, 60, 80, 95, 99] {
        let budget = total * share / 100;
        let request =
            json!({"op": "overview", "graph": g, "options": options(), "workBudget": budget});
        let o = answer(&request);
        assert_valid(&o);
        let work = &o["work"];
        assert!(work["used"].as_u64().unwrap() <= budget, "{work}");
        assert_eq!(work["limited"], true, "share {share}: {work}");
        // Every node keeps its disc; edges keep their direction.
        assert_eq!(o["edges"], full["edges"]);
        if budget < clustering {
            assert!(
                !work["unclustered"].as_array().unwrap().is_empty(),
                "{work}"
            );
            seen_unclustered = true;
        }
        seen_packed |= !work["packed"].as_array().unwrap().is_empty();
        // Packed containers are containers of the answer, the top level as -1.
        for c in work["packed"].as_array().unwrap() {
            let c = c.as_i64().unwrap();
            assert!(
                c == -1
                    || o["parent"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|p| p.as_i64() == Some(c))
            );
        }
        // The same answer for any number of threads.
        for threads in [2, 3, 8] {
            let mut threaded = request.clone();
            threaded["threads"] = json!(threads);
            assert_eq!(answer(&threaded), o, "share {share}, threads {threads}");
        }
    }
    assert!(seen_unclustered && seen_packed);
    // Nothing budgeted at all: every container of two or more children is packed, none is split.
    let none =
        answer(&json!({"op": "overview", "graph": g, "options": options(), "workBudget": 0}));
    assert_eq!(none["work"]["used"], 0);
    assert_eq!(none["stats"]["communities"], 0);
    assert_eq!(
        none["work"]["packed"].as_array().unwrap().len(),
        1 + 3 + 3 * 4,
        "{}",
        none["work"]
    );
}

#[test]
fn over_budget_refuse_fails_cleanly_and_a_session_keeps_its_state() {
    let g = graph(2, 3, 25);
    let refused = run(
        &json!({"op": "overview", "graph": g, "options": options(), "workBudget": 1000, "overBudget": "refuse"}),
    );
    assert_eq!(refused["ok"], false);
    let error = refused["error"].as_str().unwrap();
    assert!(
        error.starts_with("Overview work budget exhausted: "),
        "{error}"
    );
    // A budget that fits answers as without one.
    let plain = answer(&json!({"op": "overview", "graph": g, "options": options()}));
    let fits = answer(
        &json!({"op": "overview", "graph": g, "options": options(), "workBudget": u64::MAX, "overBudget": "refuse"}),
    );
    assert_eq!(without_work(fits), plain);
    // In a session, the refusal changes nothing: continuing goes on from the last accepted answer.
    let mut session = EngineSession::new();
    let first = session.run_value(&json!({"op": "overview", "graph": g, "options": options()}));
    assert_eq!(first["overview"], plain);
    let no = session.run_value(
        &json!({"op": "overview", "options": options(), "radii": {"g0/f1": 80.}, "start": {}, "workBudget": 10, "overBudget": "refuse"}),
    );
    assert_eq!(no["ok"], false, "{no}");
    let next = session.run_value(
        &json!({"op": "overview", "options": options(), "radii": {"g0/f1": 80.}, "start": {}}),
    );
    let expected = answer(
        &json!({"op": "overview", "graph": g, "options": options(), "radii": {"g0/f1": 80.},
        "start": {"x": plain["x"], "y": plain["y"], "r": plain["r"]}}),
    );
    assert_eq!(next["overview"], expected);
}

#[test]
fn a_session_answers_a_budget_as_dispatch_does() {
    let g = graph(3, 3, 30);
    let full = answer(
        &json!({"op": "overview", "graph": g, "options": options(), "workBudget": u64::MAX}),
    );
    let total = full["work"]["used"].as_u64().unwrap();
    let clustering = full["work"]["clustering"].as_u64().unwrap();
    let mut session = EngineSession::new();
    // First a complete overview, then requests that reuse its hierarchy: its clustering is charged again.
    let first = session.run_value(&json!({"op": "overview", "graph": g, "options": options()}));
    assert_eq!(first["ok"], true);
    for budget in [total / 2, clustering + 1, clustering / 2, total, 0] {
        let s = session.run_value(
            &json!({"op": "overview", "options": options(), "workBudget": budget, "threads": 2}),
        );
        let d = answer(
            &json!({"op": "overview", "graph": g, "options": options(), "workBudget": budget}),
        );
        assert_eq!(s["overview"], d, "budget {budget}");
    }
    // After an answer with containers left unsplit, a request without a budget is the complete overview again.
    let s = session.run_value(&json!({"op": "overview", "options": options()}));
    assert_eq!(
        s["overview"],
        answer(&json!({"op": "overview", "graph": g, "options": options()}))
    );
    // The work report comes with positions only and with columns.
    let half = total / 2;
    let moved = session.run_value(
        &json!({"op": "overview", "options": options(), "start": {}, "positionsOnly": true, "workBudget": half}),
    );
    assert_eq!(moved["overview"]["positionsOnly"], true, "{moved}");
    assert!(moved["overview"]["work"]["used"].as_u64().unwrap() <= half);
    let columns = session.run_value(
        &json!({"op": "overview", "options": options(), "start": {}, "positionsOnly": true, "columns": true,
            "workBudget": half}),
    );
    assert_eq!(columns["overview"]["work"], moved["overview"]["work"]);
    let whole = session.run_value(
        &json!({"op": "overview", "options": options(), "columns": true, "workBudget": half}),
    );
    assert_eq!(whole["overview"]["work"]["budget"], half);
}

#[test]
fn a_continuation_keeps_its_unchanged_containers_within_any_budget() {
    let g = graph(2, 3, 25);
    let plain = answer(&json!({"op": "overview", "graph": g, "options": options()}));
    let start = json!({"x": plain["x"], "y": plain["y"], "r": plain["r"]});
    // Nothing changed: every container keeps its arrangement, even without any budget left.
    let kept = answer(
        &json!({"op": "overview", "graph": g, "options": options(), "start": start, "workBudget": 0}),
    );
    assert_valid(&kept);
    assert_eq!(kept["work"]["packed"], json!([]), "{}", kept["work"]);
    let continued =
        answer(&json!({"op": "overview", "graph": g, "options": options(), "start": start}));
    assert_eq!(without_work(kept), continued, "as without a budget");
    // A grown cluster with a budget too small: valid all the same, and the same for any thread count.
    let ids = plain["ids"].as_array().unwrap();
    let file = ids.iter().position(|id| id == "g1/f2").unwrap();
    let r = plain["r"][file].as_f64().unwrap();
    for budget in [0u64, 50, 5_000] {
        let request = json!({"op": "overview", "graph": g, "options": options(), "start": start,
            "radii": {"g1/f2": r * 4.}, "workBudget": budget});
        let o = answer(&request);
        assert_valid(&o);
        let mut threaded = request.clone();
        threaded["threads"] = json!(4);
        assert_eq!(answer(&threaded), o);
    }
}

#[test]
fn bad_budgets_are_refused_and_the_answer_is_the_typed_contract() {
    let g = graph(1, 2, 10);
    for (bad, part) in [
        (json!({"workBudget": -1}), "workBudget"),
        (json!({"workBudget": "100"}), "workBudget"),
        (json!({"workBudget": 1.5}), "workBudget"),
        (json!({"overBudget": "refuse"}), "needs a workBudget"),
        (
            json!({"workBudget": 10, "overBudget": "later"}),
            "overBudget",
        ),
    ] {
        let mut request = json!({"op": "overview", "graph": g});
        for (k, v) in bad.as_object().unwrap() {
            request[k] = v.clone();
        }
        let response = run(&request);
        assert_eq!(response["ok"], false, "{request}");
        assert!(
            response["error"].as_str().unwrap().contains(part),
            "{response}"
        );
    }
    let o = answer(&json!({"op": "overview", "graph": g, "workBudget": 0, "overBudget": "answer"}));
    let typed: arqlet_model::Overview = serde_json::from_value(o.clone()).expect("the contract");
    assert!(typed.work.is_some_and(|w| w.limited));
    let request = arqlet_model::OverviewRequest {
        work_budget: Some(0),
        over_budget: Some(arqlet_model::OverBudget::Answer),
        ..Default::default()
    };
    let mut v = serde_json::to_value(&request).unwrap();
    v["graph"] = g.clone();
    v["op"] = json!("overview");
    assert_eq!(answer(&v), o);
}
