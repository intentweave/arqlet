//! Provisional (ADR 0019 step 3, ADR 0023 R7): the sessions of a long-running backend process (`layout-command
//! --sessions`), with a limit in bytes over all of them. After every message the session that ran it is weighed again
//! (`EngineSession::heap_estimate`); while the sessions together weigh more than the limit, the least recently used one
//! other than the one that just ran is dropped, and a later message to it is answered as an unknown session, as after a
//! restart of the process. The store is synchronous and holds no thread, clock or I/O: the host reads the lines and
//! bounds the time of a message.
use crate::engine_session::EngineSession;
use serde_json::{json, Value};
use std::collections::BTreeMap;

struct Kept {
    session: EngineSession,
    /// The weight after its last message.
    bytes: usize,
    /// The message number of its last use (larger is more recent).
    used: u64,
}

/// The open sessions of a backend process.
#[derive(Default)]
pub struct SessionStore {
    open: BTreeMap<u64, Kept>,
    next: u64,
    clock: u64,
    /// The most bytes the sessions may weigh together; `None` for no limit.
    limit: Option<usize>,
    /// How many sessions were dropped for the limit so far.
    evicted: u64,
}

impl SessionStore {
    /// A store with a limit in bytes over all its sessions (`None`: no limit).
    pub fn with_limit(limit: Option<usize>) -> Self {
        SessionStore {
            limit,
            ..Default::default()
        }
    }

    /// The sessions' weight together, as last measured.
    pub fn bytes(&self) -> usize {
        self.open.values().map(|k| k.bytes).sum()
    }

    pub fn len(&self) -> usize {
        self.open.len()
    }

    pub fn is_empty(&self) -> bool {
        self.open.is_empty()
    }

    /// Answers one message of the line protocol (see `layout-command`): `{"session": "open"}`, `{"session": id, "request":
    /// …}`, `{"session": id, "close": true}`, `{"request": …}` without a session, `{"sessions": "list"}`.
    pub fn handle(&mut self, message: &Value) -> String {
        self.clock += 1;
        // The ids of the open sessions (a host that keeps its own keys learns which ones the limit dropped).
        if message["sessions"].as_str() == Some("list") {
            let ids: Vec<u64> = self.open.keys().copied().collect();
            return json!({"ok": true, "sessions": ids, "bytes": self.bytes(), "evicted": self.evicted})
                .to_string();
        }
        match &message["session"] {
            Value::String(s) if s == "open" => {
                self.next += 1;
                self.open.insert(
                    self.next,
                    Kept {
                        session: EngineSession::new(),
                        bytes: 0,
                        used: self.clock,
                    },
                );
                json!({"ok": true, "session": self.next}).to_string()
            }
            Value::Number(n) => match (n.as_u64(), message["close"].as_bool()) {
                (Some(id), Some(true)) => {
                    self.open.remove(&id);
                    json!({"ok": true}).to_string()
                }
                (Some(id), _) => {
                    let Some(kept) = self.open.get_mut(&id) else {
                        return json!({"ok": false, "error": "Unknown session"}).to_string();
                    };
                    let answer = kept.session.run_value(&message["request"]);
                    kept.bytes = kept.session.heap_estimate();
                    kept.used = self.clock;
                    let dropped = self.keep_within_limit(id);
                    let mut answer = answer;
                    if dropped > 0 {
                        // Says that other sessions were dropped for the limit (they answer as unknown from now on).
                        answer["evictedSessions"] = json!(dropped);
                    }
                    answer.to_string()
                }
                _ => json!({"ok": false, "error": "Invalid session"}).to_string(),
            },
            Value::Null => crate::dispatch(&message["request"].to_string()),
            _ => json!({"ok": false, "error": "Invalid session"}).to_string(),
        }
    }

    /// Drops the least recently used sessions other than `current` while the store weighs more than its limit; answers how
    /// many were dropped.
    fn keep_within_limit(&mut self, current: u64) -> usize {
        let Some(limit) = self.limit else { return 0 };
        let mut dropped = 0;
        while self.bytes() > limit {
            let oldest = self
                .open
                .iter()
                // Only sessions that hold something: dropping an empty one (just opened, its import still to come) frees
                // nothing and takes it from the client that opened it.
                .filter(|(id, k)| **id != current && k.bytes > 0)
                .min_by_key(|(id, k)| (k.used, **id))
                .map(|(id, _)| *id);
            match oldest {
                Some(id) => {
                    self.open.remove(&id);
                    self.evicted += 1;
                    dropped += 1;
                }
                // Only the current session (and empty ones) are left: it stays, however heavy (the host's time limit
                // and its own `release` are what bound it).
                None => break,
            }
        }
        dropped
    }

    /// How many sessions were dropped for the limit so far.
    pub fn evicted(&self) -> u64 {
        self.evicted
    }
}
