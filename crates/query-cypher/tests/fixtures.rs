//! The guard against the fixtures of `examples/cypher/` (ADR 0021, stage M1): what `check` must accept or refuse, by the
//! status of a case and the verdict of a corpus query. Test code may read the fixtures from disk.
//!
//! This is only the front end: a `supported` case must pass the parser and the guard here; that it also gives the right
//! rows is the executor's test, which comes with M2 to M4.
use arqlet_query::ErrorClass;
use arqlet_query_cypher::check;
use std::fs;
use std::path::PathBuf;

fn fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/cypher")
        .join(name);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

/// What the front end must do for a status (a case) or a verdict (the corpus), or `None` when nothing is asserted yet.
fn expectation(status: &str) -> Option<Option<ErrorClass>> {
    match status {
        "supported" => Some(None),
        "later" | "rejected" => Some(Some(ErrorClass::Unsupported)),
        "invalid" => Some(Some(ErrorClass::Syntax)),
        "open" => None, // the ADR is silent: decided before the construct is implemented
        other => panic!("unknown status or verdict `{other}`"),
    }
}

fn mismatch(id: &str, query: &str, status: &str, want: Option<ErrorClass>) -> Option<String> {
    let got = check(query).err();
    let got_class = got.as_ref().map(|e| e.class);
    if got_class == want {
        return None;
    }
    Some(format!(
        "{id} [{status}] {query}\n    expected {}, got {}",
        want.map_or("acceptance".to_string(), |c| c.as_str().to_string()),
        got.map_or("acceptance".to_string(), |e| e.to_string())
    ))
}

#[test]
fn every_case_gets_the_answer_its_status_asks_for() {
    let mut failures = Vec::new();
    let mut checked = 0;
    for line in fixture("cases.jsonl").lines() {
        let case: serde_json::Value = serde_json::from_str(line).expect("a JSON line");
        let (id, query, status) = (
            case["id"].as_str().unwrap(),
            case["query"].as_str().unwrap(),
            case["status"].as_str().unwrap(),
        );
        let Some(want) = expectation(status) else {
            continue;
        };
        checked += 1;
        if let Some(error) = case["error"].as_str() {
            // The fixture's own error class must be one the front end can answer with.
            assert!(
                ["syntax", "unsupported"].contains(&error),
                "{id}: unknown error class `{error}`"
            );
            assert_eq!(
                want.map(|c| c.as_str()),
                Some(error),
                "{id}: the status and the error class disagree"
            );
        }
        failures.extend(mismatch(id, query, status, want));
    }
    assert!(
        checked >= 40,
        "only {checked} cases were checked; is the fixture file complete?"
    );
    assert!(
        failures.is_empty(),
        "{} of {checked} cases:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn every_corpus_query_gets_the_answer_its_verdict_asks_for() {
    let mut failures = Vec::new();
    let mut checked = 0;
    for (i, line) in fixture("corpus.tsv").lines().enumerate() {
        if i == 0 || line.trim().is_empty() {
            continue; // the header
        }
        let f: Vec<&str> = line.splitn(5, '\t').collect();
        assert_eq!(f.len(), 5, "corpus line {} has {} columns", i + 1, f.len());
        let (id, parse, verdict, query) = (f[0], f[2], f[3], f[4]);
        // The parse column is what a parser alone must do.
        assert_eq!(
            decypher::parse(query).is_ok(),
            parse == "ok",
            "{id}: decypher and the corpus disagree on whether this parses"
        );
        let Some(want) = expectation(verdict) else {
            continue;
        };
        checked += 1;
        failures.extend(mismatch(id, query, verdict, want));
    }
    assert!(checked >= 30, "only {checked} corpus queries were checked");
    assert!(
        failures.is_empty(),
        "{} of {checked} corpus queries:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
