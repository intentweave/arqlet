//! Provisional (ADR 0019, step 3): a session that keeps its snapshot inside the engine. A host sends only the change
//! (`{"op": "toggle", "nodeId": …}`), not the whole snapshot, and gets back what it draws (the new versions, the collapsed
//! containers and the scene), not the whole snapshot. The snapshot leaves the session only on request (`export`), for
//! saving or for handing the view to another engine (`load`).
//!
//! A plain value without global state: a Rust host owns one per view; the WASM adapter keeps them by handle.
//!
//! Requests (JSON, as for `dispatch`, without `snapshot`):
//! - `import` (as `dispatch`), `load` (`snapshot`): start the session on a graph or on a snapshot;
//! - every operation on a snapshot (`toggle`, `reveal`, `direction`, `move`, `refine`, …): applied to the session's
//!   snapshot; `expectedRevision` and `expectedLayoutVersion` default to the session's own, and a request that names
//!   other ones is refused as stale;
//! - `scene`, `trace`, `neighbourhood`, `intent`: read the session's snapshot;
//! - `export`: the whole snapshot.
//!
//! Answers to `import`, `load` and the operations: `{ok, revision, layoutVersion, collapsedIds, scene}` (`"scene": false`
//! in the request leaves the scene out).
//!
//! `overview` keeps its own state beside the snapshot (`overview::OverviewSession`): the first request carries the
//! graph, later ones leave it out and lay out on the kept graph and cluster hierarchy; a `start` without positions
//! continues from the last overview. The answer is that of `dispatch`. Every other operation (`elkLayout`, …) goes to
//! `dispatch` unchanged.
use crate::protocol::{handle, Request, Response};
use arqlet_model::Snapshot;
use serde_json::{json, Value};

/// A view whose snapshot stays in the engine.
#[derive(Default)]
pub struct EngineSession {
    snapshot: Option<Snapshot>,
    overview: crate::overview::OverviewSession,
}

impl EngineSession {
    /// An empty session; `import` or `load` starts it.
    pub fn new() -> Self {
        Self::default()
    }

    /// The session's snapshot, if it has one.
    pub fn snapshot(&self) -> Option<&Snapshot> {
        self.snapshot.as_ref()
    }

    /// Starts the session on a snapshot (a hand-over from another engine, or a saved one).
    pub fn load(&mut self, snapshot: Snapshot) {
        self.snapshot = Some(snapshot);
    }

    /// Runs one request given as JSON text and answers as JSON text.
    pub fn run(&mut self, input: &str) -> String {
        match serde_json::from_str::<Value>(input) {
            Ok(v) => self.run_value(&v).to_string(),
            Err(e) => json!({"ok": false, "error": e.to_string()}).to_string(),
        }
    }

    /// Runs one request given as a JSON value.
    pub fn run_value(&mut self, v: &Value) -> Value {
        match self.answer(v) {
            Ok(answer) => answer,
            Err(error) => json!({"ok": false, "error": error}),
        }
    }

    fn answer(&mut self, v: &Value) -> Result<Value, String> {
        let with_scene = v["scene"].as_bool() != Some(false);
        match v["op"].as_str() {
            Some("import") => {
                let request = Request::from_value(v)?;
                self.apply(handle(request), with_scene)
            }
            Some("load") => {
                let snapshot: Snapshot =
                    serde_json::from_value(v["snapshot"].clone()).map_err(|e| e.to_string())?;
                self.snapshot = Some(snapshot);
                self.summary(with_scene)
            }
            Some("export") => Ok(json!({"ok": true, "snapshot": self.current()?})),
            Some("scene") => Ok(json!({"ok": true, "scene": scene_of(self.current()?)?})),
            Some("overview") => {
                let threads = v["threads"]
                    .as_u64()
                    .unwrap_or(1)
                    .clamp(1, crate::MAX_THREADS as u64) as usize;
                Ok(self.overview.run(v, threads))
            }
            Some("trace") => crate::trace::trace_on(self.current()?, v),
            Some("neighbourhood") => crate::trace::neighbourhood_on(self.current()?, v),
            Some("intent") => {
                Ok(json!({"ok": true, "intent": crate::history::intent_of(self.current()?)}))
            }
            Some(
                "direction" | "strategy" | "routing" | "move" | "add" | "toggle" | "reveal"
                | "filter" | "configure" | "port" | "settle" | "restore" | "shape" | "measure"
                | "refine" | "refineProgressive",
            ) => {
                let current = self.current()?;
                // The session's versions unless the request names its own (then a stale request is refused as always).
                let mut v = v.clone();
                if v["expectedRevision"].is_null() {
                    v["expectedRevision"] = json!(current.revision);
                }
                if v["expectedLayoutVersion"].is_null() {
                    v["expectedLayoutVersion"] = json!(current.layout_version);
                }
                let request = Request::from_value_on(&v, Box::new(current.clone()))?;
                self.apply(handle(request), with_scene)
            }
            // Everything that does not read the session's view.
            _ => serde_json::from_str(&crate::dispatch(&v.to_string())).map_err(|e| e.to_string()),
        }
    }

    fn current(&self) -> Result<&Snapshot, String> {
        self.snapshot
            .as_ref()
            .ok_or_else(|| "The session has no view yet: import or load one first.".to_string())
    }

    /// Keeps a new snapshot (the old one stays when the operation was refused) and answers with what a host draws.
    fn apply(&mut self, response: Response, with_scene: bool) -> Result<Value, String> {
        match response {
            Response::Snapshot(snapshot) => {
                self.snapshot = Some(*snapshot);
                self.summary(with_scene)
            }
            Response::Error(e) => Err(e),
            Response::Encoded(text) => serde_json::from_str(&text).map_err(|e| e.to_string()),
            #[allow(unreachable_patterns)]
            _ => Err("Unexpected answer".into()),
        }
    }

    fn summary(&self, with_scene: bool) -> Result<Value, String> {
        let s = self.current()?;
        let mut answer = json!({
            "ok": true,
            "revision": s.revision,
            "layoutVersion": s.layout_version,
            "collapsedIds": s.collapsed_ids,
        });
        if with_scene {
            answer["scene"] = scene_of(s)?;
        }
        Ok(answer)
    }
}

fn scene_of(snapshot: &Snapshot) -> Result<Value, String> {
    crate::filter::scene_of(snapshot)
        .and_then(|scene| serde_json::to_value(scene).map_err(|e| e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dispatch;

    fn fixture() -> Value {
        serde_json::from_str(include_str!("../../../examples/fixtures/small.json")).unwrap()
    }

    /// A session answers like `dispatch` on the same snapshot: the scene of every step is the scene `dispatch` gives
    /// for the snapshot it would have sent, and the snapshot it exports is that snapshot.
    #[test]
    fn a_session_answers_like_dispatch_on_the_same_snapshot() {
        let mut session = EngineSession::new();
        let graph = fixture();
        let first = session.run_value(&json!({"op": "import", "graph": graph, "limit": 100}));
        assert_eq!(first["ok"], json!(true), "{first}");
        let sent: Value = serde_json::from_str(&dispatch(
            &json!({"op": "import", "graph": graph, "limit": 100}).to_string(),
        ))
        .unwrap();
        let scene = |s: &Value| -> Value {
            serde_json::from_str::<Value>(&dispatch(
                &json!({"op": "scene", "snapshot": s}).to_string(),
            ))
            .unwrap()["scene"]
                .clone()
        };
        assert_eq!(first["scene"], scene(&sent["snapshot"]));
        // A toggle: no snapshot in the request, the same result as sending it.
        let toggled = session.run_value(&json!({"op": "toggle", "nodeId": "root"}));
        let s = &sent["snapshot"];
        let by_dispatch: Value = serde_json::from_str(&dispatch(
            &json!({"op": "toggle", "snapshot": s, "nodeId": "root",
                "expectedRevision": s["revision"], "expectedLayoutVersion": s["layoutVersion"]})
            .to_string(),
        ))
        .unwrap();
        assert_eq!(toggled["collapsedIds"], json!(["root"]));
        assert_eq!(
            toggled["layoutVersion"],
            by_dispatch["snapshot"]["layoutVersion"]
        );
        assert_eq!(toggled["scene"], scene(&by_dispatch["snapshot"]));
        let exported = session.run_value(&json!({"op": "export"}));
        assert_eq!(exported["snapshot"], by_dispatch["snapshot"]);
    }

    #[test]
    fn a_stale_request_is_refused_and_the_view_stays() {
        let mut session = EngineSession::new();
        session
            .run_value(&json!({"op": "import", "graph": fixture(), "limit": 100, "scene": false}));
        let stale = session
            .run_value(&json!({"op": "toggle", "nodeId": "root", "expectedLayoutVersion": 7}));
        assert_eq!(stale, json!({"ok": false, "error": "Stale view request"}));
        let still = session.run_value(&json!({"op": "intent"}));
        assert_eq!(still["ok"], json!(true));
        assert_eq!(
            session.snapshot().unwrap().collapsed_ids,
            Vec::<String>::new()
        );
        let refused = session.run_value(&json!({"op": "toggle", "nodeId": "nope"}));
        assert_eq!(refused["ok"], json!(false));
        assert_eq!(
            session.snapshot().unwrap().layout_version,
            0,
            "a refused change leaves the view as it was"
        );
    }

    #[test]
    fn a_view_handed_over_continues_in_another_session() {
        let mut backend = EngineSession::new();
        backend
            .run_value(&json!({"op": "import", "graph": fixture(), "limit": 100, "scene": false}));
        let saved = backend.run_value(&json!({"op": "export"}))["snapshot"].clone();
        let mut browser = EngineSession::new();
        let loaded = browser.run_value(&json!({"op": "load", "snapshot": saved}));
        assert_eq!(loaded["ok"], json!(true));
        let a = backend.run_value(&json!({"op": "toggle", "nodeId": "root"}));
        let b = browser.run_value(&json!({"op": "toggle", "nodeId": "root"}));
        assert_eq!(a, b, "both continue the same view identically");
        let traced = browser.run_value(&json!({"op": "trace", "from": "a", "to": "b"}));
        assert_eq!(traced["ok"], json!(true), "{traced}");
    }

    #[test]
    fn an_empty_session_says_so() {
        let mut session = EngineSession::new();
        let answer = session.run_value(&json!({"op": "toggle", "nodeId": "root"}));
        assert_eq!(answer["ok"], json!(false));
        assert!(answer["error"].as_str().unwrap().contains("import or load"));
        // Operations that need no view still work.
        assert_eq!(
            session.run_value(&json!({"op": "optionDefaults"}))["ok"],
            json!(true)
        );
        assert!(session.run("not json").contains(r#""ok":false"#));
    }
}
