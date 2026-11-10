//! A pin placed over a free sibling must not leave the two overlapping: the sibling moves, the pinned node stays at its pin.
use arqlet_runtime::{configure_snapshot, import_view};
use serde_json::json;

#[test]
fn a_pin_over_a_sibling_is_resolved_and_the_pin_is_kept() {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../benchmarks/datasets/er-diagram.json"
    ))
    .unwrap();
    let imported = import_view(&text, 1000).unwrap();
    for (x, y) in [(224., 88.), (700., 48.)] {
        let snapshot = configure_snapshot(
            &imported,
            imported.revision,
            imported.layout_version,
            &json!({"pins": {"ent:Customer": {"x": x, "y": y}}}),
        )
        .unwrap();
        assert!(
            snapshot.geometry_issues.is_empty(),
            "pin at ({x}, {y}): {:?}",
            snapshot.geometry_issues
        );
        let customer = &snapshot.boxes["ent:Customer"];
        assert_eq!((customer.x, customer.y), (x, y), "the pin is kept");
    }
}
