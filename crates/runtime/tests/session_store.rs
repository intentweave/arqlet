//! ADR 0023, R7: the sessions of a backend process with a limit in bytes; the least recently used ones are dropped.
use arqlet_runtime::session_store::SessionStore;
use serde_json::{json, Value};

fn graph(n: usize) -> Value {
    let mut nodes = vec![json!({"id": "pkg", "kind": "package", "label": "pkg"})];
    for i in 0..n {
        nodes.push(json!({"id": format!("n{i}"), "kind": "fn", "label": format!("function number {i}"), "parentId": "pkg"}));
    }
    json!({"nodes": nodes, "edges": []})
}

fn ask(store: &mut SessionStore, message: Value) -> Value {
    serde_json::from_str(&store.handle(&message)).unwrap()
}

fn open(store: &mut SessionStore) -> u64 {
    ask(store, json!({"session": "open"}))["session"]
        .as_u64()
        .unwrap()
}

fn import(store: &mut SessionStore, id: u64) -> Value {
    ask(
        store,
        json!({"session": id, "request": {"op": "import", "graph": graph(300), "limit": 400, "scene": false,
        "options": {"levelOfDetail": {"depth": 0}}}}),
    )
}

#[test]
fn the_least_recently_used_session_goes_when_the_sessions_weigh_too_much() {
    // The weight of one session with this graph, measured without a limit.
    let mut free = SessionStore::with_limit(None);
    let one = open(&mut free);
    assert_eq!(import(&mut free, one)["ok"], true);
    let weight = free.bytes();
    assert!(weight > 0);

    // Room for two such sessions, not three.
    let mut store = SessionStore::with_limit(Some(weight * 5 / 2));
    let (a, b, c) = (open(&mut store), open(&mut store), open(&mut store));
    assert!(import(&mut store, a).get("evictedSessions").is_none());
    assert!(import(&mut store, b).get("evictedSessions").is_none());
    // a is used again, so b is now the least recently used one.
    assert_eq!(
        ask(
            &mut store,
            json!({"session": a, "request": {"op": "intent"}})
        )["ok"],
        true
    );
    let third = import(&mut store, c);
    assert_eq!(third["ok"], true);
    assert_eq!(third["evictedSessions"], 1);
    let list = ask(&mut store, json!({"sessions": "list"}));
    assert_eq!(list["sessions"], json!([a, c]));
    assert_eq!(list["evicted"], 1);
    assert!(store.bytes() <= weight * 5 / 2);
    // A message to the dropped one is answered as unknown, as after a restart.
    assert_eq!(
        ask(
            &mut store,
            json!({"session": b, "request": {"op": "intent"}})
        )["error"],
        "Unknown session"
    );

    // The session that just ran stays, however heavy: with a limit below one session, the others go and it remains.
    let mut tight = SessionStore::with_limit(Some(weight / 2));
    let (x, y) = (open(&mut tight), open(&mut tight));
    import(&mut tight, x);
    let alone = import(&mut tight, y);
    assert_eq!(alone["ok"], true);
    assert_eq!(
        ask(&mut tight, json!({"sessions": "list"}))["sessions"],
        json!([y])
    );

    // Without a limit nothing is dropped; closing still frees.
    let mut open_ended = SessionStore::with_limit(None);
    let ids: Vec<u64> = (0..4).map(|_| open(&mut open_ended)).collect();
    for id in &ids {
        assert!(import(&mut open_ended, *id)
            .get("evictedSessions")
            .is_none());
    }
    assert_eq!(open_ended.len(), 4);
    assert_eq!(
        ask(&mut open_ended, json!({"session": ids[0], "close": true}))["ok"],
        true
    );
    assert_eq!(open_ended.len(), 3);
}
