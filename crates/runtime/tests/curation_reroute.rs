//! A move that leaves one route with no path must not refuse the whole re-route: the other routes are re-routed, and the
//! route that has no path stays pending, so the draft shows exactly what is left.

use arqlet_runtime::dispatch;
use serde_json::{json, Value};

const SMALL: &str = include_str!("../../../examples/fixtures/small.json");

fn run(request: Value) -> Value {
    serde_json::from_str(&dispatch(&request.to_string())).unwrap()
}

#[test]
fn a_move_that_needs_several_passes_still_routes_every_route_it_can() {
    let graph: Value = serde_json::from_str(SMALL).unwrap();
    let imported = run(json!({"op": "import", "graph": graph, "limit": 150}));
    assert_eq!(imported["ok"], true, "{imported}");
    let created = run(
        json!({"op": "referenceCreate", "snapshot": imported["snapshot"], "referenceId": "moved"}),
    );
    assert_eq!(created["ok"], true, "{created}");
    let mut reference = created["reference"].clone();
    // The move that the editor showed: the node 'a' (Parse) moves by (72, 36).
    reference["curated"]["nodes"]["a"]["x"] =
        json!(reference["curated"]["nodes"]["a"]["x"].as_f64().unwrap() + 72.0);
    reference["curated"]["nodes"]["a"]["y"] =
        json!(reference["curated"]["nodes"]["a"]["y"].as_f64().unwrap() + 36.0);
    // The routes that touch the moved node are pending before the refresh, as the session marks them.
    reference["curated"]["pendingRoutes"] = json!(["aa", "ab", "ab2", "ba"]);
    let refreshed = run(json!({"op": "referenceRefresh", "reference": reference}));
    assert_eq!(refreshed["ok"], true, "{refreshed}");
    let rerouted = run(json!({"op": "referenceReroute", "reference": refreshed["reference"]}));
    assert_eq!(
        rerouted["ok"], true,
        "a route without a path must not refuse the re-route: {rerouted}"
    );
    let pending = rerouted["reference"]["curated"]["pendingRoutes"]
        .as_array()
        .unwrap();
    assert!(
        pending
            .iter()
            .all(|id| ["aa", "ab", "ab2", "ba"].contains(&id.as_str().unwrap())),
        "only routes that were asked for may stay pending: {pending:?}"
    );
}
