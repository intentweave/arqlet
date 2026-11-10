//! The engine defaults the playground reads for its detail settings.

use arqlet_runtime::dispatch;
use serde_json::{json, Value};

#[test]
fn option_defaults_are_the_engine_defaults() {
    let response: Value =
        serde_json::from_str(&dispatch(&json!({"op": "optionDefaults"}).to_string())).unwrap();
    assert_eq!(response["ok"], true);
    let options = &response["options"];
    assert_eq!(options["candidates"], json!([1, 2, 3, 4]));
    assert_eq!(options["routingEngine"], "legacy");
    assert_eq!(options["hybridBy"], "compact");
    assert_eq!(options["freePlacement"], false);
    assert_eq!(options["layeredConstruction"], true);
    assert_eq!(options["hierarchicalOrdering"], true);
}
