//! ADR 0023, step R2: the change log of a session. Structure (`patch`) and content (`payload`) changes are numbered groups
//! in one sequence; a replica that took a snapshot at a number and applies the groups after it arrives at the same state,
//! byte for byte.
use arqlet_runtime::engine_session::EngineSession;
use serde_json::{json, Value};

fn ok(answer: Value) -> Value {
    assert_eq!(answer["ok"], true, "{answer}");
    answer
}

fn graph() -> Value {
    let mut nodes = vec![json!({"id": "pkg", "kind": "package", "label": "pkg"})];
    let mut edges = Vec::new();
    for i in 0..6 {
        nodes.push(json!({"id": format!("n{i}"), "kind": "fn", "label": format!("n{i}"), "parentId": "pkg"}));
        if i > 0 {
            edges.push(json!({"id": format!("e{i}"), "kind": "calls", "source": format!("n{}", i - 1), "target": format!("n{i}")}));
        }
    }
    json!({"nodes": nodes, "edges": edges})
}

/// A small deterministic generator (xorshift64).
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// A random change group that the session accepts: add a node with an edge and properties, remove an added node, set or
/// unset properties, or replace the memberships.
fn random_group(rng: &mut Rng, step: usize, added: &mut Vec<String>) -> Value {
    match rng.below(4) {
        0 => {
            let id = format!("x{step}");
            let to = format!("n{}", rng.below(6));
            added.push(id.clone());
            json!({"patch": {"nodes": [{"id": id, "kind": "fn", "label": id, "parentId": "pkg"}],
                "edges": [{"id": format!("ex{step}"), "kind": "calls", "source": id, "target": to}]},
                "payload": {"set": {"nodes": {id.clone(): {"owner": format!("o{}", rng.below(5)), "step": step}}}}})
        }
        1 if !added.is_empty() => {
            let id = added.remove(rng.below(added.len() as u64) as usize);
            json!({"patch": {"removeNodes": [id]}})
        }
        2 => {
            json!({"payload": {"set": {"nodes": {format!("n{}", rng.below(6)): {"loc": rng.below(1000)}}},
            "unset": {"nodes": {format!("n{}", rng.below(6)): ["owner"]}}}})
        }
        _ => json!({"patch": {"memberships": {"dimensions": [{"id": "team", "groups": [
            {"id": "a", "members": [format!("n{}", rng.below(6)), format!("n{}", rng.below(6))]},
            {"id": "b", "members": [format!("n{}", rng.below(6))]}]}]}}}),
    }
}

#[test]
fn replaying_the_log_on_the_old_snapshot_gives_the_same_state() {
    for seed in [1u64, 7, 42] {
        let mut primary = EngineSession::new();
        ok(primary
            .run_value(&json!({"op": "import", "graph": graph(), "limit": 100, "scene": false})));
        let start = ok(primary.run_value(&json!({"op": "export"})))["snapshot"].clone();
        let at = ok(primary.run_value(&json!({"op": "changesSince", "sequence": 0})))["sequence"]
            .as_u64()
            .unwrap();
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(1));
        let mut added = Vec::new();
        for step in 0..12 {
            let mut group = random_group(&mut rng, step, &mut added);
            group["op"] = json!("change");
            group["scene"] = json!(false);
            ok(primary.run_value(&group));
        }
        let log = ok(primary.run_value(&json!({"op": "changesSince", "sequence": at})));
        let changes = log["changes"].as_array().unwrap().clone();
        assert_eq!(changes.len(), 12, "seed {seed}");

        // The replica: the snapshot at `at`, then every group in order.
        let mut replica = EngineSession::new();
        ok(replica.run_value(&json!({"op": "load", "snapshot": start})));
        for group in &changes {
            let mut request = json!({"op": "change", "scene": false});
            for part in ["patch", "payload"] {
                if let Some(x) = group.get(part) {
                    request[part] = x.clone();
                }
            }
            ok(replica.run_value(&request));
        }
        let a = ok(primary.run_value(&json!({"op": "export"})))["snapshot"].to_string();
        let b = ok(replica.run_value(&json!({"op": "export"})))["snapshot"].to_string();
        assert_eq!(a, b, "seed {seed}: the same snapshot, byte for byte");
        let content = |s: &mut EngineSession| {
            let c = ok(s.run_value(&json!({"op": "content"})));
            (c["nodes"].to_string(), c["edges"].to_string())
        };
        assert_eq!(
            content(&mut primary),
            content(&mut replica),
            "seed {seed}: the same content"
        );
    }
}

#[test]
fn a_change_is_one_group_all_or_nothing() {
    let mut s = EngineSession::new();
    ok(s.run_value(&json!({"op": "import", "graph": graph(), "limit": 100, "scene": false})));
    // patch and payload each log a group; a change with both logs one.
    let p = ok(s.run_value(&json!({"op": "patch", "patch": {"nodes": [{"id": "y", "kind": "fn", "label": "y"}]}, "scene": false})));
    assert_eq!(p["sequence"], 1);
    let q = ok(s.run_value(&json!({"op": "payload", "set": {"nodes": {"y": {"owner": "Ann"}}}})));
    assert_eq!(q["sequence"], 2);
    let c = ok(s.run_value(&json!({"op": "change", "expectedSequence": 2, "scene": false,
        "patch": {"nodes": [{"id": "z", "kind": "fn", "label": "z"}]}, "payload": {"set": {"nodes": {"z": {"owner": "Bob"}}}}})));
    assert_eq!(
        (c["sequence"].clone(), c["contentRevision"].clone()),
        (json!(3), json!(2))
    );
    // A payload that changes nothing logs nothing.
    assert!(
        ok(s.run_value(&json!({"op": "payload", "set": {"nodes": {"z": {"owner": "Bob"}}}})))
            .get("sequence")
            .is_none()
    );

    // All or nothing: the payload part is refused, so the node is not added either.
    let before = ok(s.run_value(&json!({"op": "export"})))["snapshot"].to_string();
    let refused = s.run_value(&json!({"op": "change", "scene": false,
        "patch": {"nodes": [{"id": "w", "kind": "fn", "label": "w"}]}, "payload": {"set": {"nodes": {"w": {"label": "no"}}}}}));
    assert_eq!(refused["ok"], false, "{refused}");
    assert_eq!(
        ok(s.run_value(&json!({"op": "export"})))["snapshot"].to_string(),
        before
    );
    assert_eq!(
        ok(s.run_value(&json!({"op": "changesSince", "sequence": 0})))["sequence"],
        3
    );
    // Stale and empty changes are refused.
    assert_eq!(s.run_value(&json!({"op": "change", "expectedSequence": 1, "payload": {"set": {"nodes": {"z": {"x": 1}}}}}))["ok"], false);
    assert_eq!(s.run_value(&json!({"op": "change"}))["ok"], false);

    // The log: groups after a number; a number beyond it is an error; a new import clears it (resync), the number goes on.
    let since = ok(s.run_value(&json!({"op": "changesSince", "sequence": 1})));
    let numbers: Vec<u64> = since["changes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g["sequence"].as_u64().unwrap())
        .collect();
    assert_eq!(numbers, [2, 3]);
    assert!(
        since["changes"][1].get("patch").is_some() && since["changes"][1].get("payload").is_some()
    );
    assert_eq!(
        s.run_value(&json!({"op": "changesSince", "sequence": 9}))["ok"],
        false
    );
    ok(s.run_value(&json!({"op": "import", "graph": graph(), "limit": 100, "scene": false})));
    let after_import = ok(s.run_value(&json!({"op": "changesSince", "sequence": 2})));
    assert_eq!(
        (
            after_import["resync"].clone(),
            after_import["sequence"].clone()
        ),
        (json!(true), json!(3))
    );
    assert_eq!(
        ok(s.run_value(&json!({"op": "changesSince", "sequence": 3})))["changes"],
        json!([])
    );

    // Memberships through patch (ADR 0022 §5b): by dimension and group, removals first; a change of memberships only
    // moves nothing (the boxes and routes stay), with a new revision.
    let source = |s: &mut EngineSession| {
        let exported = ok(s.run_value(&json!({"op": "export"})))["snapshot"].clone();
        let graph = if exported["sourceGraph"].is_null() {
            exported["graph"].clone()
        } else {
            exported["sourceGraph"].clone()
        };
        (exported, graph["memberships"].clone())
    };
    let (before, _) = source(&mut s);
    ok(s.run_value(&json!({"op": "patch", "scene": false, "patch": {"memberships": {"dimensions": [{"id": "team",
        "groups": [{"id": "a", "members": ["n0", "nobody"]}, {"id": "sub", "parent": "a", "members": ["n1"]},
            {"id": "b", "members": ["n2"]}]}]}}})));
    let (after, memberships) = source(&mut s);
    assert_eq!(
        memberships["dimensions"][0]["groups"][0]["members"],
        json!(["n0", "nobody"]),
        "unknown ids kept"
    );
    assert_eq!(after["boxes"], before["boxes"], "nothing moves");
    assert_eq!(after["routes"], before["routes"]);
    assert_eq!(
        after["revision"].as_u64(),
        before["revision"].as_u64().map(|r| r + 1)
    );
    // A group of the same id is replaced, a new one appended; removing `a` takes `sub` along.
    ok(s.run_value(&json!({"op": "patch", "scene": false, "patch": {
        "removeMemberships": [{"dimension": "team", "group": "a"}],
        "memberships": {"dimensions": [{"id": "team", "groups": [{"id": "b", "members": ["n3"]}, {"id": "c", "members": ["n4"]}]}]}}})));
    let (_, memberships) = source(&mut s);
    let groups: Vec<(String, Value)> = memberships["dimensions"][0]["groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| (g["id"].as_str().unwrap().to_string(), g["members"].clone()))
        .collect();
    assert_eq!(
        groups,
        [
            ("b".to_string(), json!(["n3"])),
            ("c".to_string(), json!(["n4"]))
        ]
    );
    // Unknown removals and malformed groups are refused.
    for patch in [
        json!({"removeMemberships": [{"dimension": "nowhere"}]}),
        json!({"removeMemberships": [{"dimension": "team", "group": "nowhere"}]}),
        json!({"memberships": {"dimensions": [{"id": "t", "groups": [{"id": "a"}, {"id": "a"}]}]}}),
    ] {
        assert_eq!(
            s.run_value(&json!({"op": "patch", "patch": patch}))["ok"],
            false,
            "{patch}"
        );
    }
    ok(s.run_value(&json!({"op": "patch", "scene": false, "patch": {"removeMemberships": [{"dimension": "team"}]}})));
    assert!(
        source(&mut s).1.is_null(),
        "the last dimension removed: none left"
    );
    // A membership change with nodes is laid out as any patch.
    ok(s.run_value(&json!({"op": "patch", "scene": false, "patch": {"nodes": [{"id": "m", "kind": "fn", "label": "m"}],
        "memberships": {"dimensions": [{"id": "team", "groups": [{"id": "a", "members": ["m"]}]}]}}})));

    // Without a session these operations are refused.
    for op in ["change", "changesSince", "content"] {
        let answer: Value =
            serde_json::from_str(&arqlet_runtime::dispatch(&json!({"op": op}).to_string()))
                .unwrap();
        assert!(
            answer["error"]
                .as_str()
                .unwrap()
                .contains("resident session"),
            "{op}: {answer}"
        );
    }
}
