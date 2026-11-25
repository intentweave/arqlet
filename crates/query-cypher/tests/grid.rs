//! A grid of patterns, pinned: what `lower` makes of every combination of a clause, a path selector, the nodes and a
//! relationship, and of every query in the fixtures of `examples/cypher/`. Test code may read the fixtures from disk.
//!
//! `pinned.tsv` records the outcome of each query: `ok` with a hash of the plan, or the class of the error. A parser
//! upgrade or a change of the lowering that alters an outcome fails `outcomes_are_pinned`; the diff of the regenerated
//! file (`ARQLET_PIN=1 cargo test -p arqlet-query-cypher --test grid`) then shows each query whose outcome changed.

use arqlet_query_cypher::lower;
use std::fs;
use std::path::PathBuf;

fn path(dir: &str, name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(dir)
        .join(name)
}

fn read(path: &PathBuf) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

/// One query of the grid, and whether it uses a construct the layer does not run.
struct Case {
    query: String,
    unsupported: bool,
}

/// Every combination of the parts below: about 1 500 queries.
fn grid() -> Vec<Case> {
    let clauses = ["MATCH", "OPTIONAL MATCH"];
    // (before the pattern, after it, unsupported)
    let selectors = [
        ("", "", false),
        ("ALL ", "", false),
        ("ANY ", "", true),
        ("ANY SHORTEST ", "", true),
        ("SHORTEST 2 ", "", true),
        ("SHORTEST 1 GROUPS ", "", true),
        ("p = ", "", true),
        ("p = shortestPath(", ")", true),
        ("p = allShortestPaths(", ")", true),
    ];
    let lefts = [
        ("(a)", false),
        ("(a:Parse)", false),
        ("(a WHERE a.label = 'Parse')", true),
    ];
    let relationships = [
        ("-->", false),
        ("<--", false),
        ("--", false),
        ("-[:calls]->", false),
        ("-[r:calls]->", false),
        ("-[:calls*1..2]->", false),
        ("-[:calls*]->", false),
        ("-[r:calls WHERE r.weight > 1]->", true),
        ("-[:calls]->{1,2}", true),
        ("-[:calls]->{,2}", true),
        ("-[:calls]->{2}", true),
        ("-[:calls]->+", true),
        ("-[:calls]->*", true),
        (" ((x)-[:calls]->(y)){1,2} ", true),
    ];
    let rights = [("(b)", false), ("(b WHERE b.label = 'Parse')", true)];
    let mut cases = Vec::new();
    for clause in clauses {
        for (open, close, s) in selectors {
            for (left, l) in lefts {
                for (rel, r) in relationships {
                    for (right, b) in rights {
                        cases.push(Case {
                            query: format!("{clause} {open}{left}{rel}{right}{close} RETURN b"),
                            unsupported: s || l || r || b,
                        });
                    }
                }
            }
        }
    }
    cases
}

/// The queries of the fixtures: the corpus and the cases.
fn fixture_queries() -> Vec<String> {
    let mut queries = Vec::new();
    for (i, line) in read(&path("../../examples/cypher", "corpus.tsv"))
        .lines()
        .enumerate()
    {
        if i > 0 && !line.trim().is_empty() {
            queries.push(
                line.splitn(5, '\t')
                    .nth(4)
                    .expect("five columns")
                    .to_string(),
            );
        }
    }
    for line in read(&path("../../examples/cypher", "cases.jsonl")).lines() {
        if line.trim().is_empty() {
            continue;
        }
        let case: serde_json::Value = serde_json::from_str(line).expect("a JSON case");
        queries.push(case["query"].as_str().expect("a query").to_string());
    }
    queries
}

/// FNV-1a, 64 bits: a stable hash of the plan's debug form.
fn fnv(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

fn outcome(query: &str) -> String {
    match lower(query) {
        Ok(plan) => format!("ok:{:016x}", fnv(&format!("{plan:?}"))),
        Err(e) => format!("{:?}", e.class).to_lowercase(),
    }
}

#[test]
fn outcomes_are_pinned() {
    let queries = fixture_queries()
        .into_iter()
        .chain(grid().into_iter().map(|c| c.query));
    let mut got = String::from("outcome\tquery\n");
    for query in queries {
        // A query of the fixtures may span lines; the pinned file holds one per line.
        let query = query.replace('\n', " ");
        got.push_str(&format!("{}\t{query}\n", outcome(&query)));
    }
    let pinned = path("tests", "pinned.tsv");
    if std::env::var_os("ARQLET_PIN").is_some() {
        fs::write(&pinned, &got).expect("write pinned.tsv");
        return;
    }
    let want = read(&pinned);
    let changed: Vec<String> = want
        .lines()
        .zip(got.lines())
        .filter(|(w, g)| w != g)
        .map(|(w, g)| format!("  was {w}\n  now {g}"))
        .collect();
    assert!(
        changed.is_empty() && want.lines().count() == got.lines().count(),
        "{} outcomes changed (regenerate with ARQLET_PIN=1 and review the diff):\n{}",
        changed.len(),
        changed.join("\n")
    );
}

/// No query of the grid that uses a construct the executor does not run yet lowers to a plan: it is refused, never run
/// as a different query.
#[test]
fn unsupported_constructs_never_lower_to_a_plan() {
    let ran: Vec<String> = grid()
        .into_iter()
        .filter(|c| c.unsupported && lower(&c.query).is_ok())
        .map(|c| c.query)
        .collect();
    assert!(ran.is_empty(), "lowered to a plan:\n{}", ran.join("\n"));
}

/// The rest of the grid lowers, and the path selector `ALL` keeps every path, like a pattern without a selector.
#[test]
fn supported_patterns_lower_and_all_is_no_selector() {
    for case in grid().into_iter().filter(|c| !c.unsupported) {
        let plan = lower(&case.query).unwrap_or_else(|e| panic!("{}: {e}", case.query));
        if case.query.contains(" ALL ") {
            let plain = case.query.replace(" ALL ", " ");
            assert_eq!(plan, lower(&plain).unwrap(), "{}", case.query);
        }
    }
}
