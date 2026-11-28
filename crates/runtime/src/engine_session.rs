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
//! - `scene`, `trace`, `neighbourhood`, `intent`, `compactBookmark`, `readBookmark`: read the session's snapshot;
//! - `export`: the whole snapshot;
//! - `estimate` (provisional, ADR 0024 C3): the heap a request (`request`, as this session takes it) will need at its
//!   peak, on the session's view (`crate::estimate`);
//! - `keep` (provisional, #148): keeps the current snapshot under a mark (`{ok, mark}`; the same mark while the snapshot is
//!   unchanged), at most `limit` (default 32, up to 1000) snapshots, the oldest dropped first; `recall` (`mark`) makes a
//!   kept snapshot current again, so a step back restores the geometry instead of laying out again. A recall is refused
//!   (`{ok: false, evicted: true}` or `{ok: false, changed: true}`) when the mark is no longer kept or the graph's
//!   revision changed since; the restored snapshot gets the next layout version, so answers to the replaced one stay
//!   stale. `import` and `load` drop the kept snapshots, `load` with `keep: true` (the same graph continued) keeps them;
//! - `columns` (provisional, step 4): the scene as columns (`columns::SceneColumns`). The answer lists the id tables and
//!   each column's name, type and length; the numbers stay in the session (`EngineSession::columns`, in WASM
//!   `lw_session_column`) until the snapshot changes.
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
    /// The scene as columns, built by a `columns` request; dropped when the snapshot changes.
    columns: Option<crate::columns::SceneColumns>,
    overview: crate::overview::OverviewSession,
    /// Provisional (ADR 0023, R1): the properties of nodes and edges, beside the snapshot and never in it. A new import
    /// starts it empty; `load` (a hand-over of the same graph) keeps it.
    content: crate::content::Content,
    /// Provisional (ADR 0023, R2): the numbered changes of the source (`patch`, `payload`, `change`). A new import or a
    /// `load` clears it (a replica has to resync); the sequence goes on.
    log: crate::changes::ChangeLog,
    /// Provisional (#148): earlier snapshots by mark (`keep`, `recall`), oldest first, and the next mark.
    kept: Vec<(u64, Snapshot)>,
    next_mark: u64,
}

/// How many snapshots `keep` holds unless the request names a limit.
const KEEP_LIMIT: usize = 32;

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
        self.columns = None;
        self.kept.clear();
    }

    /// `keep`: the current snapshot under a mark (the last one again while the snapshot is unchanged).
    fn keep(&mut self, v: &Value) -> Result<Value, String> {
        let limit = match v.get("limit") {
            None => KEEP_LIMIT,
            Some(l) => l
                .as_u64()
                .filter(|l| (1..=1000).contains(l))
                .ok_or("keep limit must be a whole number from 1 to 1000")?
                as usize,
        };
        let current = self.current()?;
        let same = self.kept.last().filter(|(_, s)| {
            s.revision == current.revision && s.layout_version == current.layout_version
        });
        let mark = match same {
            Some((mark, _)) => *mark,
            None => {
                let snapshot = current.clone();
                self.next_mark = self.next_mark.wrapping_add(1);
                self.kept.push((self.next_mark, snapshot));
                self.next_mark
            }
        };
        if self.kept.len() > limit {
            let drop = self.kept.len() - limit;
            self.kept.drain(..drop);
        }
        Ok(json!({"ok": true, "mark": mark, "kept": self.kept.len()}))
    }

    /// `recall`: the kept snapshot of `mark` becomes current, with the next layout version.
    fn recall(&mut self, v: &Value, with_scene: bool) -> Result<Value, String> {
        let mark = v["mark"].as_u64().ok_or("recall needs a mark")?;
        let current = self.current()?;
        let (revision, layout_version) = (current.revision, current.layout_version);
        let Some((_, kept)) = self.kept.iter().find(|(m, _)| *m == mark) else {
            return Ok(json!({"ok": false, "evicted": true,
                "error": format!("The snapshot of mark {mark} is no longer kept")}));
        };
        if kept.revision != revision {
            return Ok(json!({"ok": false, "changed": true,
                "error": format!("The graph changed since mark {mark} (revision {} to {revision})", kept.revision)}));
        }
        let mut restored = kept.clone();
        restored.layout_version = layout_version
            .checked_add(1)
            .ok_or("Layout version overflow")?;
        self.snapshot = Some(restored);
        self.columns = None;
        self.summary(with_scene)
    }

    /// The scene's columns of the last `columns` request, valid until the snapshot changes (provisional, ADR 0019
    /// step 4). A host that views them as memory must not keep the view across a change of this session.
    pub fn columns(&self) -> Option<&crate::columns::SceneColumns> {
        self.columns.as_ref()
    }

    /// The numbers of the last `overview` request with `columns: true`, valid until the session's next overview request.
    pub fn overview_columns(&self) -> Option<&crate::columns::OverviewColumns> {
        self.overview.columns()
    }

    /// Runs one request given as JSON text and answers as JSON text.
    pub fn run(&mut self, input: &str) -> String {
        // An import read in one pass, without a JSON value of the whole graph (which takes several times the graph's own
        // memory and, in WASM, stays as the high-water mark of the linear memory). The same request as the general way;
        // anything the one-pass reading refuses goes the general way, with the errors it has always given.
        if let Some(answer) = self.import_text(input) {
            return answer.to_string();
        }
        if let Some(answer) = self.load_text(input) {
            return answer.to_string();
        }
        // Provisional (ADR 0024, C3): an estimate read in one pass too (it may carry an import's graph).
        if crate::estimate::is_estimate(input) {
            return crate::estimate::estimate_in_session(input, self.snapshot.as_ref()).to_string();
        }
        match serde_json::from_str::<Value>(input) {
            Ok(v) => self.run_value(&v).to_string(),
            Err(e) => json!({"ok": false, "error": e.to_string()}).to_string(),
        }
    }

    /// `run` for a well-formed `load`: the snapshot read straight into its type, without a JSON value of it (a recycled
    /// engine receives a whole snapshot; the value would take several times its memory and stay as WASM's high-water
    /// mark). The same as the general way; anything this reading refuses goes the general way.
    fn load_text(&mut self, input: &str) -> Option<Value> {
        #[derive(serde::Deserialize)]
        struct LoadText {
            op: String,
            snapshot: Snapshot,
            content: Option<Value>,
            sequence: Option<u64>,
            scene: Option<Value>,
            keep: Option<bool>,
        }
        if !input.contains("\"load\"") {
            return None;
        }
        let mut load: LoadText = serde_json::from_str(input).ok()?;
        if load.op != "load" {
            return None;
        }
        let content = match load.content.as_ref().filter(|c| !c.is_null()) {
            Some(c) => Some(crate::content::Content::from_answer(c).ok()?),
            None => None,
        };
        // Read from text, lists grow by doubling; the snapshot is kept, so they are trimmed.
        load.snapshot.graph.shrink_to_fit();
        if let Some(source) = load.snapshot.source_graph.take() {
            let mut source =
                std::sync::Arc::try_unwrap(source).unwrap_or_else(|shared| (*shared).clone());
            source.shrink_to_fit();
            load.snapshot.source_graph = Some(std::sync::Arc::new(source));
        }
        self.snapshot = Some(load.snapshot);
        self.columns = None;
        // As the general `load`: the kept snapshots stay only with `keep: true` (#148).
        if load.keep != Some(true) {
            self.kept.clear();
        }
        if let Some(content) = content {
            self.content = content;
        }
        match load.sequence {
            Some(sequence) => self.log = crate::changes::ChangeLog::starting_at(sequence),
            None => self.log.clear(),
        }
        let with_scene = load.scene.as_ref().and_then(Value::as_bool) != Some(false);
        Some(match self.summary(with_scene) {
            Ok(answer) => answer,
            Err(error) => json!({"ok": false, "error": error}),
        })
    }

    /// `run` for a well-formed `import` without `timings`: the request read straight into its types.
    fn import_text(&mut self, input: &str) -> Option<Value> {
        // The fields this reading needs besides the import's own; everything else (the graph too) is skipped unparsed.
        #[derive(serde::Deserialize)]
        struct Head {
            op: Option<String>,
            scene: Option<Value>,
            timings: Option<Value>,
        }
        if !input.contains("\"import\"") {
            return None;
        }
        let head: Head = serde_json::from_str(input).ok()?;
        if head.op.as_deref() != Some("import") || head.timings.is_some() {
            return None;
        }
        let (request, _) = crate::protocol::Request::from_json_delivered(input).ok()?;
        let with_scene = head.scene.as_ref().and_then(Value::as_bool) != Some(false);
        Some(match self.apply(handle(request), with_scene) {
            Ok(answer) => {
                self.content = Default::default();
                self.log.clear();
                answer
            }
            Err(error) => json!({"ok": false, "error": error}),
        })
    }

    /// Runs one request given as a JSON value.
    pub fn run_value(&mut self, v: &Value) -> Value {
        // Provisional (phase timings), as `dispatch` adds them.
        if v.get("timings").and_then(Value::as_bool) == Some(true) {
            let mut plain = v.clone();
            if let Some(map) = plain.as_object_mut() {
                map.remove("timings");
            }
            let answer = crate::timed(|| self.run_value(&plain).to_string());
            return serde_json::from_str(&answer)
                .unwrap_or_else(|e| json!({"ok": false, "error": e.to_string()}));
        }
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
                let answer = self.apply(handle(request), with_scene)?;
                self.content = Default::default();
                self.log.clear();
                self.kept.clear();
                Ok(answer)
            }
            // Provisional (ADR 0023, R1): set, unset or clear properties of nodes and edges, as one change.
            Some("payload") => {
                let before = self.content.revision;
                let graph = self
                    .snapshot
                    .as_ref()
                    .map(|s| s.source_graph.as_deref().unwrap_or(&s.graph));
                let mut answer = self.content.apply(v, graph)?;
                if self.content.revision != before {
                    answer["sequence"] =
                        json!(self.log.push(json!({"payload": payload_part(v)}))?);
                }
                Ok(answer)
            }
            Some("patch") => {
                let mut answer = self.edit(v, with_scene)?;
                answer["sequence"] = json!(self.log.push(json!({"patch": v["patch"].clone()}))?);
                Ok(answer)
            }
            // Provisional (ADR 0023, R2): structure and content changed together, as one group: all of it or nothing.
            Some("change") => self.change(v, with_scene),
            Some("changesSince") => {
                let since = v["sequence"].as_u64().ok_or("Missing sequence")?;
                Ok(self.log.since(since))
            }
            // Provisional (ADR 0023, R7): what the session holds, part by part, and giving parts back.
            Some("memory") => Ok(self.memory()),
            // Provisional (ADR 0024, C3): the peak a request will need, before it runs, on this session's view.
            Some("estimate") => Ok(crate::estimate::estimate_in_session(
                &v.to_string(),
                self.snapshot.as_ref(),
            )),
            Some("release") => self.release(v),
            // The whole content, for a replica that resyncs.
            Some("content") => Ok(json!({"ok": true, "contentRevision": self.content.revision,
                "sequence": self.log.sequence, "nodes": self.content.nodes, "edges": self.content.edges})),
            Some("load") => {
                let snapshot: Snapshot =
                    serde_json::from_value(v["snapshot"].clone()).map_err(|e| e.to_string())?;
                // Provisional (ADR 0023, R7): a recycled engine takes over the content as it was (with its revision) and
                // the change log's number; the log's groups stay behind, so a replica behind that number resyncs.
                let content = match v.get("content").filter(|c| !c.is_null()) {
                    Some(c) => Some(crate::content::Content::from_answer(c)?),
                    None => None,
                };
                self.snapshot = Some(snapshot);
                self.columns = None;
                // The kept snapshots (#148) stay only when the host says the snapshot continues the same graph (a drag,
                // a refinement): a revision is a counter, not a name of the graph.
                if v["keep"].as_bool() != Some(true) {
                    self.kept.clear();
                }
                if let Some(content) = content {
                    self.content = content;
                }
                match v.get("sequence").and_then(Value::as_u64) {
                    Some(sequence) => self.log = crate::changes::ChangeLog::starting_at(sequence),
                    None => self.log.clear(),
                }
                self.summary(with_scene)
            }
            // Provisional (ADR 0019, step 4): the scene as columns. The answer describes them (versions, id tables,
            // name, type and length of each column); the numbers stay in the session, see `columns()`.
            Some("columns") => {
                let scene = crate::filter::scene_of(self.current()?)?;
                let columns = crate::columns::SceneColumns::from_scene(&scene);
                let mut answer = columns.describe();
                answer["ok"] = json!(true);
                self.columns = Some(columns);
                Ok(answer)
            }
            Some("export") => Ok(json!({"ok": true, "snapshot": self.current()?})),
            Some("keep") => self.keep(v),
            Some("recall") => self.recall(v, with_scene),
            Some("scene") => Ok(json!({"ok": true, "scene": scene_of(self.current()?)?})),
            // Paths and neighbourhoods on the overview's graph: the session's, unless the request brings one.
            Some("overviewTrace" | "overviewNeighbourhood") if v.get("graph").is_none() => {
                let graph = self.overview.graph().cloned().ok_or(
                    "The session has no overview yet: send the first overview with its graph.",
                );
                Ok(crate::trace::graph_read_response(
                    graph.map_err(String::from),
                    v,
                ))
            }
            Some("overview") => {
                let threads = v["threads"]
                    .as_u64()
                    .unwrap_or(1)
                    .clamp(1, crate::MAX_THREADS as u64) as usize;
                Ok(self.overview.run(v, threads))
            }
            Some("trace") => crate::trace::trace_on(self.current()?, v),
            Some("neighbourhood") => crate::trace::neighbourhood_on(self.current()?, v),
            Some("details") => {
                crate::details::details_on_with(self.current()?, v, Some(&self.content))
            }
            Some("find") => crate::find::find_on_with(self.current()?, v, Some(&self.content)),
            Some("peek") => crate::peek::peek_on(self.current()?, v),
            // Provisional (ADR 0021, M5b): a query over the source graph and the session's content.
            Some("query") => crate::query::query_on(self.current()?, Some(&self.content), v),
            Some("digest") => Ok(crate::find::digest_on(self.current()?)),
            Some("compactBookmark") => crate::bookmark::compact_on(self.current()?, v),
            Some("readBookmark") => crate::bookmark::read_on(self.current()?, v),
            Some("intent") => {
                Ok(json!({"ok": true, "intent": crate::history::intent_of(self.current()?)}))
            }
            Some(
                "direction" | "strategy" | "routing" | "move" | "add" | "toggle" | "reveal"
                | "filter" | "configure" | "port" | "settle" | "pin" | "restore" | "shape"
                | "measure" | "refine" | "refineProgressive",
            ) => self.edit(v, with_scene),
            // Everything that does not read the session's view.
            _ => serde_json::from_str(&crate::dispatch(&v.to_string())).map_err(|e| e.to_string()),
        }
    }

    /// What the session holds in heap bytes, estimated cheaply (no JSON text of any part): the source graph, the structure's
    /// view and index, the view graph, boxes, routes, content, change log, columns and overview, without the layered plans
    /// and the rest of the snapshot. For a store that weighs its sessions after every message (`SessionStore`).
    pub fn heap_estimate(&self) -> usize {
        use crate::memory;
        let mut total = memory::properties(&self.content.nodes)
            + memory::properties(&self.content.edges)
            + self.log.heap_estimate()
            + self.columns.as_ref().map_or(0, memory::columns)
            + self.overview.heap_estimate()
            + self.kept_estimate();
        if let Some(s) = &self.snapshot {
            let source = s.source_graph.as_deref().unwrap_or(&s.graph);
            total += memory::graph(source)
                + memory::boxes(&s.boxes)
                + memory::routes(&s.routes, s.routes.capacity());
            if s.source_graph.is_some() {
                total += memory::graph(&s.graph);
            }
            if let Some(structure) = &s.structure {
                let shared = s
                    .source_graph
                    .as_ref()
                    .is_some_and(|src| std::sync::Arc::ptr_eq(src, &structure.view));
                if !shared {
                    total += memory::graph(&structure.view);
                }
                total += memory::index(&structure.index);
                if let Some(base) = &structure.base {
                    total += memory::graph(&base.view) + memory::index(&base.index);
                }
            }
        }
        total
    }

    /// What the session holds, part by part, in heap bytes estimated from sizes and lengths (`crate::memory`); a shared
    /// part is counted once. `nodes` is the source graph's node count, `perNode` the total over it.
    fn memory(&self) -> Value {
        use crate::memory;
        let mut parts = serde_json::Map::new();
        let mut nodes = 0usize;
        if let Some(s) = &self.snapshot {
            let source = s.source_graph.as_deref().unwrap_or(&s.graph);
            nodes = source.nodes.len();
            parts.insert("sourceGraph".into(), json!(memory::graph(source)));
            if s.source_graph.is_some() {
                parts.insert("viewGraph".into(), json!(memory::graph(&s.graph)));
            }
            if let Some(structure) = &s.structure {
                let shared = s
                    .source_graph
                    .as_ref()
                    .is_some_and(|src| std::sync::Arc::ptr_eq(src, &structure.view));
                if !shared {
                    parts.insert(
                        "structureView".into(),
                        json!(memory::graph(&structure.view)),
                    );
                }
                parts.insert(
                    "structureIndex".into(),
                    json!(memory::index(&structure.index)),
                );
                if let Some(base) = &structure.base {
                    parts.insert(
                        "structureBase".into(),
                        json!(memory::graph(&base.view) + memory::index(&base.index)),
                    );
                }
            }
            parts.insert("boxes".into(), json!(memory::boxes(&s.boxes)));
            parts.insert(
                "routes".into(),
                json!(memory::routes(&s.routes, s.routes.capacity())),
            );
            parts.insert("layeredPlans".into(), json!(memory::layered_plans(s)));
            parts.insert("snapshotRest".into(), json!(memory::snapshot_rest(s)));
        }
        parts.insert(
            "content".into(),
            json!(
                memory::properties(&self.content.nodes) + memory::properties(&self.content.edges)
            ),
        );
        parts.insert("changeLog".into(), json!(self.log.heap_estimate()));
        parts.insert(
            "columns".into(),
            json!(self.columns.as_ref().map_or(0, memory::columns)),
        );
        parts.insert("overview".into(), json!(self.overview.heap_estimate()));
        parts.insert("kept".into(), json!(self.kept_estimate()));
        let total: u64 = parts.values().filter_map(Value::as_u64).sum();
        json!({"ok": true, "estimated": true, "parts": parts, "total": total, "nodes": nodes,
            "perNode": if nodes == 0 { 0 } else { total / nodes as u64 }})
    }

    /// The kept snapshots (`keep`): their view graphs, boxes and routes, and a source graph only when it is not the one
    /// of the current snapshot (it is shared while the graph is unchanged).
    fn kept_estimate(&self) -> usize {
        use crate::memory;
        let current = self.snapshot.as_ref().and_then(|s| s.source_graph.as_ref());
        self.kept
            .iter()
            .map(|(_, s)| {
                let shared = match (&s.source_graph, current) {
                    (Some(a), Some(b)) => std::sync::Arc::ptr_eq(a, b),
                    _ => false,
                };
                memory::graph(&s.graph)
                    + memory::boxes(&s.boxes)
                    + memory::routes(&s.routes, s.routes.capacity())
                    + match (&s.source_graph, shared) {
                        (Some(source), false) => memory::graph(source),
                        _ => 0,
                    }
            })
            .sum()
    }

    /// `release`: gives parts back (`parts`: `content`, `changeLog`, `columns`, `overview`, `kept`; `keys` with `content`
    /// forgets only those keys). The snapshot stays. Releasing content is a change of the content and goes into the change log.
    fn release(&mut self, v: &Value) -> Result<Value, String> {
        let parts: Vec<&str> = v["parts"]
            .as_array()
            .ok_or("release needs parts: content, changeLog, columns, overview, kept")?
            .iter()
            .map(|p| p.as_str().ok_or("release parts are strings"))
            .collect::<Result<_, _>>()?;
        if let Some(unknown) = parts.iter().find(|p| {
            !matches!(
                **p,
                "content" | "changeLog" | "columns" | "overview" | "kept"
            )
        }) {
            return Err(format!("release does not know the part {unknown}"));
        }
        let before = self.memory()["total"].as_u64().unwrap_or(0);
        if parts.contains(&"content") {
            let change = match v.get("keys") {
                Some(keys) => json!({"dropKeys": keys}),
                None => json!({"clear": true}),
            };
            let revision = self.content.revision;
            self.content.apply(&change, None)?;
            if self.content.revision != revision {
                self.log.push(json!({"payload": change}))?;
            }
        }
        if parts.contains(&"changeLog") {
            self.log.clear();
        }
        if parts.contains(&"columns") {
            self.columns = None;
        }
        if parts.contains(&"overview") {
            self.overview = Default::default();
        }
        if parts.contains(&"kept") {
            self.kept.clear();
        }
        let mut answer = self.memory();
        answer["released"] = json!(before.saturating_sub(answer["total"].as_u64().unwrap_or(0)));
        Ok(answer)
    }

    /// An operation on the session's snapshot, with the session's versions unless the request names its own (then a stale
    /// request is refused as always).
    fn edit(&mut self, v: &Value, with_scene: bool) -> Result<Value, String> {
        let current = self.current()?;
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

    /// `change`: a `patch` and a `payload` applied as one group (`expectedSequence` refuses a stale one). If either part
    /// is refused, the snapshot and the content stay as they were.
    fn change(&mut self, v: &Value, with_scene: bool) -> Result<Value, String> {
        if let Some(expected) = v.get("expectedSequence").and_then(Value::as_u64) {
            if expected != self.log.sequence {
                return Err(format!(
                    "Stale change: expected sequence {expected}, the log is at {}",
                    self.log.sequence
                ));
            }
        }
        let has_patch = v.get("patch").is_some_and(|p| !p.is_null());
        let has_payload = ["set", "unset", "clear", "dropKeys"]
            .iter()
            .any(|k| v.get("payload").and_then(|p| p.get(k)).is_some());
        if !has_patch && !has_payload {
            return Err("A change needs a patch, a payload or both".into());
        }
        let (snapshot, columns, content) = (
            self.snapshot.clone(),
            self.columns.clone(),
            self.content.clone(),
        );
        let result = (|| -> Result<Value, String> {
            let mut answer = if has_patch {
                let mut request = json!({"op": "patch", "patch": v["patch"].clone()});
                for key in ["expectedRevision", "expectedLayoutVersion"] {
                    if let Some(x) = v.get(key) {
                        request[key] = x.clone();
                    }
                }
                self.edit(&request, with_scene)?
            } else {
                self.summary(with_scene)?
            };
            if has_payload {
                let graph = self
                    .snapshot
                    .as_ref()
                    .map(|s| s.source_graph.as_deref().unwrap_or(&s.graph));
                let content = self.content.apply(&v["payload"], graph)?;
                answer["contentRevision"] = content["contentRevision"].clone();
            }
            Ok(answer)
        })();
        match result {
            Ok(mut answer) => {
                let mut group = json!({});
                if has_patch {
                    group["patch"] = v["patch"].clone();
                }
                if has_payload {
                    group["payload"] = payload_part(&v["payload"]);
                }
                answer["sequence"] = json!(self.log.push(group)?);
                Ok(answer)
            }
            Err(error) => {
                self.snapshot = snapshot;
                self.columns = columns;
                self.content = content;
                Err(error)
            }
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
                self.columns = None;
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

    /// #148: `recall` makes a kept snapshot current again with its geometry, under the next layout version; a stale request
    /// to the replaced snapshot stays refused, a changed graph or an evicted mark is refused, `import` drops what is kept.
    #[test]
    fn recall_restores_a_kept_snapshot_with_its_geometry() {
        let mut session = EngineSession::new();
        let first = session.run_value(&json!({"op": "import", "graph": fixture(), "limit": 100}));
        assert_eq!(first["ok"], json!(true), "{first}");
        let geometry = |session: &mut EngineSession| {
            let s = session.run_value(&json!({"op": "export"}))["snapshot"].clone();
            (
                s["boxes"].clone(),
                s["routes"].clone(),
                s["collapsedIds"].clone(),
            )
        };
        let start = geometry(&mut session);
        let m0 = session.run_value(&json!({"op": "keep"}))["mark"].clone();
        assert_eq!(
            session.run_value(&json!({"op": "keep"}))["mark"],
            m0,
            "unchanged: the same mark"
        );
        let toggled = session.run_value(&json!({"op": "toggle", "nodeId": "root"}));
        let closed = geometry(&mut session);
        let m1 = session.run_value(&json!({"op": "keep"}))["mark"].clone();
        assert_ne!(m0, m1);
        let back = session.run_value(&json!({"op": "recall", "mark": m0}));
        assert_eq!(back["ok"], json!(true), "{back}");
        assert_eq!(
            geometry(&mut session),
            start,
            "the geometry of the kept snapshot"
        );
        let v1 = toggled["layoutVersion"].as_u64().unwrap();
        assert_eq!(
            back["layoutVersion"].as_u64(),
            Some(v1 + 1),
            "the next layout version"
        );
        assert_eq!(back["scene"]["nodes"], first["scene"]["nodes"]);
        assert_eq!(back["scene"]["routes"], first["scene"]["routes"]);
        let stale = session.run_value(&json!({"op": "toggle", "nodeId": "root",
            "expectedRevision": toggled["revision"], "expectedLayoutVersion": v1}));
        assert_eq!(
            stale["ok"],
            json!(false),
            "an answer to the replaced snapshot stays stale"
        );
        let forth = session.run_value(&json!({"op": "recall", "mark": m1}));
        assert_eq!(forth["ok"], json!(true));
        assert_eq!(geometry(&mut session), closed);
        // The kept snapshots count in the memory and can be given back.
        assert!(
            session.run_value(&json!({"op": "memory"}))["parts"]["kept"]
                .as_u64()
                .unwrap()
                > 0
        );
        // Only `limit` snapshots are kept: the oldest goes.
        session.run_value(&json!({"op": "toggle", "nodeId": "root"}));
        session.run_value(&json!({"op": "keep", "limit": 2}));
        let gone = session.run_value(&json!({"op": "recall", "mark": m0}));
        assert_eq!(
            (gone["ok"].clone(), gone["evicted"].clone()),
            (json!(false), json!(true)),
            "{gone}"
        );
        // A changed graph (a patch, a new revision) is not restored from a snapshot of the earlier one.
        let m2 = session.run_value(&json!({"op": "keep"}))["mark"].clone();
        let patched = session.run_value(&json!({"op": "patch", "patch": {"nodes": [{"id": "extra", "kind": "k", "label": "extra"}]}}));
        assert_eq!(patched["ok"], json!(true), "{patched}");
        let changed = session.run_value(&json!({"op": "recall", "mark": m2}));
        assert_eq!(
            (changed["ok"].clone(), changed["changed"].clone()),
            (json!(false), json!(true)),
            "{changed}"
        );
        // A `load` of the same graph continued (`keep: true`) keeps them, one without it drops them.
        let m3 = session.run_value(&json!({"op": "keep"}))["mark"].clone();
        let current = session.run_value(&json!({"op": "export"}))["snapshot"].clone();
        session.run(&json!({"op": "load", "snapshot": current, "keep": true}).to_string());
        assert_eq!(
            session.run_value(&json!({"op": "recall", "mark": m3}))["ok"],
            json!(true)
        );
        session.run(&json!({"op": "load", "snapshot": current}).to_string());
        assert_eq!(
            session.run_value(&json!({"op": "recall", "mark": m3}))["evicted"],
            json!(true)
        );
        // A new import starts without kept snapshots.
        session.run_value(&json!({"op": "import", "graph": fixture(), "limit": 100}));
        let after = session.run_value(&json!({"op": "recall", "mark": m1}));
        assert_eq!(after["evicted"], json!(true));
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

    /// The columns hold what the scene's JSON holds (as `f32`), in the scene's order, and are dropped by a change.
    #[test]
    fn columns_match_the_scene_and_end_with_a_change() {
        let mut session = EngineSession::new();
        let first = session.run_value(&json!({"op": "import", "graph": fixture(), "limit": 100}));
        let scene = &first["scene"];
        assert!(session.columns().is_none());
        let described = session.run_value(&json!({"op": "columns"}));
        assert_eq!(described["ok"], json!(true), "{described}");
        let c = session.columns().expect("columns kept");
        let nodes = scene["nodes"].as_array().unwrap();
        let routes = scene["routes"].as_array().unwrap();
        assert!(!nodes.is_empty() && !routes.is_empty());
        assert_eq!(c.node_ids.len(), nodes.len());
        assert_eq!(c.node_bounds.len(), nodes.len() * 4);
        for (i, n) in nodes.iter().enumerate() {
            assert_eq!(c.node_ids[i], n["id"].as_str().unwrap());
            for (k, key) in ["x", "y", "width", "height"].iter().enumerate() {
                assert_eq!(
                    c.node_bounds[i * 4 + k],
                    n["bounds"][key].as_f64().unwrap() as f32
                );
            }
            let parent = n["parentId"]
                .as_str()
                .and_then(|p| nodes.iter().position(|m| m["id"] == p));
            assert_eq!(
                c.node_parent[i],
                parent.map_or(crate::columns::NONE, |p| p as u32)
            );
            assert_eq!(
                c.node_flags[i] & crate::columns::FLAG_CONTAINER != 0,
                n["container"] == json!(true)
            );
        }
        assert_eq!(c.route_offsets.len(), routes.len() + 1);
        for (i, r) in routes.iter().enumerate() {
            let points = r["points"].as_array().unwrap();
            let (a, b) = (c.route_offsets[i] as usize, c.route_offsets[i + 1] as usize);
            assert_eq!(b - a, points.len());
            for (k, p) in points.iter().enumerate() {
                assert_eq!(c.route_points[(a + k) * 2], p["x"].as_f64().unwrap() as f32);
                assert_eq!(
                    c.route_points[(a + k) * 2 + 1],
                    p["y"].as_f64().unwrap() as f32
                );
            }
            let end = |id: &str| {
                nodes
                    .iter()
                    .position(|m| m["id"] == id)
                    .map_or(crate::columns::NONE, |p| p as u32)
            };
            assert_eq!(c.route_source[i], end(r["source"].as_str().unwrap()));
            assert_eq!(c.route_target[i], end(r["target"].as_str().unwrap()));
        }
        // The description names every column with its length.
        let listed = described["columns"].as_array().unwrap();
        assert_eq!(listed.len(), c.columns().len());
        assert_eq!(listed[0]["name"], json!("nodeBounds"));
        assert_eq!(listed[0]["length"], json!(nodes.len() * 4));
        // The same view gives the same columns again; a change drops them.
        let again = session.run_value(&json!({"op": "columns"}));
        assert_eq!(again, described);
        session.run_value(&json!({"op": "toggle", "nodeId": "root", "scene": false}));
        assert!(session.columns().is_none());
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

/// The part of a `payload` request that a change log keeps and a replica applies: `set`, `unset`, `clear`.
fn payload_part(v: &Value) -> Value {
    let mut part = json!({});
    for key in ["set", "unset", "clear", "dropKeys"] {
        if let Some(x) = v.get(key) {
            part[key] = x.clone();
        }
    }
    part
}
