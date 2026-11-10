//! Keeps the committed TypeScript contracts in step with the Rust types.
//!
//! - The generated files must match a fresh export byte for byte (drift check).
//! - Every enum value in the generated TypeScript must be accepted by the Rust type and serialize back to the
//!   same string. JSON imports widen string literals in `contract.check.ts`, so this is the enum check.

use std::fs;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::Serialize;

fn committed_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/contracts/src/generated")
}

fn listing(dir: &Path) -> Vec<(String, String)> {
    let mut files: Vec<(String, String)> = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
        .map(|entry| {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            (name, fs::read_to_string(&path).unwrap())
        })
        .collect();
    files.sort();
    files
}

#[test]
fn committed_contracts_match_the_rust_types() {
    let out = std::env::temp_dir().join(format!("arqlet-contracts-{}", std::process::id()));
    let _ = fs::remove_dir_all(&out);
    arqlet_contracts::export_to(&out).expect("export contracts");
    let fresh = listing(&out);
    let committed = listing(&committed_dir());
    let _ = fs::remove_dir_all(&out);
    let names = |files: &[(String, String)]| {
        files
            .iter()
            .map(|(name, _)| name.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        names(&committed),
        names(&fresh),
        "generated file set differs; run `pnpm --filter @arqlet/contracts run generate`"
    );
    for ((name, expected), (_, actual)) in fresh.iter().zip(&committed) {
        assert_eq!(
            actual, expected,
            "{name} is stale; run `pnpm --filter @arqlet/contracts run generate`"
        );
    }
}

/// The string literals of `export type <name> = "a" | "b";` in the committed file.
fn enum_values(name: &str) -> Vec<String> {
    let text = fs::read_to_string(committed_dir().join(format!("{name}.ts"))).unwrap();
    let line = text
        .lines()
        .find(|line| line.starts_with(&format!("export type {name} =")))
        .unwrap_or_else(|| panic!("{name} is not a string-union type in the generated contracts"));
    line.split('"')
        .skip(1)
        .step_by(2)
        .map(str::to_owned)
        .collect()
}

fn check_enum<T: DeserializeOwned + Serialize>(name: &str) {
    let values = enum_values(name);
    assert!(!values.is_empty(), "{name} has no values");
    for value in values {
        let json = format!("\"{value}\"");
        let parsed: T = serde_json::from_str(&json)
            .unwrap_or_else(|e| panic!("{name}: generated value {value} is rejected by Rust: {e}"));
        assert_eq!(
            serde_json::to_string(&parsed).unwrap(),
            json,
            "{name}: {value} does not round-trip"
        );
    }
}

#[test]
fn generated_enum_values_round_trip_through_rust() {
    use arqlet_model::{
        Bundling, HybridCriterion, LayoutStrategy, Objective, Orientation, PortSide, RoutingEngine,
    };
    check_enum::<PortSide>("PortSide");
    check_enum::<Orientation>("Orientation");
    check_enum::<LayoutStrategy>("LayoutStrategy");
    check_enum::<Objective>("Objective");
    check_enum::<Bundling>("Bundling");
    check_enum::<RoutingEngine>("RoutingEngine");
    check_enum::<arqlet_reference::document::Status>("Status");
    check_enum::<HybridCriterion>("HybridCriterion");
}
