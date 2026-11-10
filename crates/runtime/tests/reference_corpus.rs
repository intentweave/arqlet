//! The curated reference corpus (benchmarks/references) is checked in `cargo test`, without a browser: every stored
//! document matches its manifest hash, is valid or invalid as the manifest expects, and each mode gives the expected
//! outcome. Regenerate the corpus with `python3 benchmarks/references/build.py` after an intended engine change.

use std::collections::BTreeSet;
use std::path::PathBuf;

use arqlet_runtime::dispatch;
use serde_json::{json, Value};

fn corpus() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../benchmarks/references")
}

fn run(request: Value) -> Value {
    serde_json::from_str(&dispatch(&request.to_string())).unwrap()
}

fn manifest() -> Value {
    serde_json::from_str(&std::fs::read_to_string(corpus().join("manifest.json")).unwrap()).unwrap()
}

#[test]
fn the_manifest_lists_each_case_once() {
    let cases = manifest()["cases"].as_array().unwrap().clone();
    let ids: BTreeSet<_> = cases.iter().map(|c| c["id"].as_str().unwrap()).collect();
    assert_eq!(ids.len(), cases.len(), "case ids must be unique");
    assert!(!cases.is_empty());
}

#[test]
fn every_stored_document_matches_its_hash_and_expected_validity() {
    let manifest = manifest();
    assert_eq!(manifest["format"], "layerweave-reference-corpus");
    for case in manifest["cases"].as_array().unwrap() {
        let id = case["id"].as_str().unwrap();
        let bytes = std::fs::read(corpus().join(case["file"].as_str().unwrap())).unwrap();
        assert_eq!(
            arqlet_reference::sha256_hex(&bytes),
            case["sha256"],
            "{id}: the stored file changed; rebuild the corpus if this is intended"
        );
        let document: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(document["inputHash"], case["inputHash"], "{id}");
        assert_eq!(
            document["integrity"]["payloadHash"], case["payloadHash"],
            "{id}"
        );
        let report = run(json!({"op": "referenceValidate", "reference": document}));
        assert_eq!(report["ok"], true, "{id}: {report}");
        assert_eq!(
            report["report"]["valid"], case["expectValid"],
            "{id}: {report}"
        );
        assert_eq!(
            report["report"]["metrics"]["version"], manifest["metricVersion"],
            "{id}"
        );
    }
}

#[test]
fn constrained_reproduction_honours_the_curators_pins() {
    let manifest = manifest();
    for case in manifest["cases"].as_array().unwrap() {
        let Some(pins) = case.get("pins") else {
            continue;
        };
        let id = case["id"].as_str().unwrap();
        let document: Value = serde_json::from_slice(
            &std::fs::read(corpus().join(case["file"].as_str().unwrap())).unwrap(),
        )
        .unwrap();
        let reproduced =
            run(json!({"op": "referenceReproduce", "mode": "constrained", "reference": document}));
        assert_eq!(reproduced["ok"], true, "{id}: {reproduced}");
        assert_eq!(reproduced["pins"]["honoured"], *pins, "{id}");
        assert_eq!(reproduced["pins"]["broken"], json!([]), "{id}");
        assert_eq!(reproduced["candidate"]["valid"], true, "{id}");
    }
}

#[test]
fn a_recorded_repair_reroutes_its_routes_and_moves_nothing_else() {
    let manifest = manifest();
    for case in manifest["cases"].as_array().unwrap() {
        let Some(edits) = case.get("edits") else {
            continue;
        };
        let id = case["id"].as_str().unwrap();
        let document: Value = serde_json::from_slice(
            &std::fs::read(corpus().join(case["file"].as_str().unwrap())).unwrap(),
        )
        .unwrap();
        let repaired = run(json!({"op": "referenceRepair", "reference": document, "edits": edits}));
        assert_eq!(repaired["ok"], true, "{id}: {repaired}");
        assert_eq!(
            repaired["after"]["valid"], case["expectValid"],
            "{id}: {repaired}"
        );
        assert_eq!(
            repaired["movedElsewhere"],
            json!([]),
            "{id}: only the edited node may move"
        );
        assert!(
            !repaired["pendingRoutes"].as_array().unwrap().is_empty(),
            "{id}: the move must affect at least one route"
        );
    }
}
