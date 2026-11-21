//! Provisional (phase timings): `timings: true` adds where a request's time went, when a host installed a clock; the
//! layout itself is the same with and without.
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU64, Ordering};

static TICKS: AtomicU64 = AtomicU64::new(0);

#[test]
fn timings_are_reported_on_request_and_change_nothing_else() {
    let graph: Value = serde_json::from_str(
        &std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../examples/fixtures/small.json"
        ))
        .unwrap(),
    )
    .unwrap();
    let request = json!({"op": "import", "graph": graph, "limit": 100});
    let plain: Value =
        serde_json::from_str(&arqlet_runtime::dispatch(&request.to_string())).unwrap();
    // A clock that advances one ms on every read (the test's clock, deterministic).
    arqlet_runtime::clock::install(|| TICKS.fetch_add(1, Ordering::Relaxed) as f64);
    let mut asked = request.clone();
    asked["timings"] = json!(true);
    let timed: Value = serde_json::from_str(&arqlet_runtime::dispatch(&asked.to_string())).unwrap();
    assert_eq!(timed["ok"], true, "{timed}");
    let t = &timed["timings"];
    for key in [
        "total",
        "layout",
        "placement",
        "organicStress",
        "separation",
        "repair",
        "portResolution",
        "routing",
    ] {
        assert!(t[key].as_f64().is_some(), "{key} in {t}");
    }
    assert!(t["total"].as_f64().unwrap() >= t["layout"].as_f64().unwrap());
    assert!(t["layout"].as_f64().unwrap() > 0.);
    // The same snapshot as without timings; a request without them gets none.
    assert_eq!(timed["snapshot"], plain["snapshot"]);
    let again: Value =
        serde_json::from_str(&arqlet_runtime::dispatch(&request.to_string())).unwrap();
    assert!(again.get("timings").is_none());
    assert_eq!(again["snapshot"], plain["snapshot"]);
    // A resident session answers with timings too.
    let mut session = arqlet_runtime::engine_session::EngineSession::new();
    let answer = session.run_value(&asked);
    assert_eq!(answer["ok"], true, "{answer}");
    assert!(answer["timings"]["layout"].as_f64().unwrap() > 0.);
    let toggled = session.run_value(&json!({"op": "toggle", "nodeId": "root", "timings": true,
        "expectedRevision": answer["revision"], "expectedLayoutVersion": answer["layoutVersion"]}));
    assert_eq!(toggled["ok"], true, "{toggled}");
    assert!(toggled["timings"]["total"].as_f64().is_some());
}
