//! Provisional (ADR 0024 C2, ADR 0030): the sessions of one process, usable from several threads at once. Sessions run in
//! parallel with each other and one at a time each: the store's map, weights and least-recently-used order sit behind one
//! short lock (look up, account, choose what to drop), and every session behind a lock of its own, under which its request
//! runs. A computation never runs under the store's lock.
//!
//! As `SessionStore` (ADR 0023 R7), the store has a limit in bytes over all sessions: after every request the session
//! that ran it is weighed again (`EngineSession::heap_estimate`), and while the sessions together weigh more than the
//! limit, the least recently used one is dropped and answered as unknown afterwards. Unlike there, a session that is
//! running a request (or about to) is never dropped; with sessions growing at the same time the limit is therefore soft,
//! and a host bounds the overshoot by how many heavy requests it lets run at once (ADR 0024, item 3).
//!
//! The store holds no thread, clock or I/O: the host (the native Node adapter, `crates/node`, or a native server) decides
//! which thread runs a request. Each session's answers are the same as in a serial run, for any number of threads: no
//! session reads another, and the layout does not depend on the thread it runs on. Calls on one session are serialized
//! by its lock but not ordered: a host that needs them in order (every host does) sends the next one after the answer to
//! the previous one, as `@arqlet/node` does.
use crate::engine_session::EngineSession;
use serde_json::json;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

/// One session and what the store knows about it.
struct Kept {
    session: Arc<Mutex<EngineSession>>,
    /// The weight after its last request.
    bytes: usize,
    /// The request number of its last use (larger is more recent).
    used: u64,
    /// Requests that have taken the session and not given it back yet; a session with any is never dropped.
    active: usize,
}

#[derive(Default)]
struct Book {
    open: BTreeMap<u64, Kept>,
    next: u64,
    clock: u64,
    evicted: u64,
}

impl Book {
    fn bytes(&self) -> usize {
        self.open.values().map(|k| k.bytes).sum()
    }
}

/// Why a request found no session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Missing {
    /// Never opened, closed, or dropped for the limit.
    Unknown,
    /// A request panicked while it held the session; the session was dropped, since its state may be half changed.
    Failed,
}

impl Missing {
    /// The answer a host gives for it (the same text as the WASM adapter's and `SessionStore`'s for an unknown session).
    pub fn answer(self) -> String {
        match self {
            Missing::Unknown => json!({"ok": false, "error": "Unknown session"}).to_string(),
            Missing::Failed => {
                json!({"ok": false, "error": "The session failed and was closed"}).to_string()
            }
        }
    }
}

/// The open sessions of a process, shared between threads (`Sync`). Handles start at 1 and are not reused.
#[derive(Default)]
pub struct SharedSessionStore {
    book: Mutex<Book>,
    /// The most bytes the sessions may weigh together; `None` for no limit.
    limit: Option<usize>,
}

impl SharedSessionStore {
    /// A store with a limit in bytes over all its sessions (`None`: no limit).
    pub fn with_limit(limit: Option<usize>) -> Self {
        SharedSessionStore {
            book: Mutex::default(),
            limit,
        }
    }

    // The book is changed only in short steps that leave it consistent, so a panic elsewhere never leaves it half written:
    // a poisoned lock is taken over.
    fn book(&self) -> MutexGuard<'_, Book> {
        self.book.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Opens an empty session and answers its handle.
    pub fn open(&self) -> u64 {
        let mut book = self.book();
        book.clock += 1;
        book.next += 1;
        let (id, used) = (book.next, book.clock);
        book.open.insert(
            id,
            Kept {
                session: Arc::default(),
                bytes: 0,
                used,
                active: 0,
            },
        );
        id
    }

    /// Closes a session; answers whether it was open. A request running on it finishes, and its answer is still given;
    /// later ones are answered as unknown.
    pub fn close(&self, id: u64) -> bool {
        self.book().open.remove(&id).is_some()
    }

    /// The handles of the open sessions, in ascending order.
    pub fn ids(&self) -> Vec<u64> {
        self.book().open.keys().copied().collect()
    }

    /// The sessions' weight together, as last measured.
    pub fn bytes(&self) -> usize {
        self.book().bytes()
    }

    /// How many sessions were dropped for the limit so far.
    pub fn evicted(&self) -> u64 {
        self.book().evicted
    }

    /// `{"ok": true, "sessions": [ids], "bytes": n, "evicted": n}`, as `SessionStore` answers `{"sessions": "list"}`.
    pub fn list(&self) -> String {
        let book = self.book();
        let ids: Vec<u64> = book.open.keys().copied().collect();
        json!({"ok": true, "sessions": ids, "bytes": book.bytes(), "evicted": book.evicted})
            .to_string()
    }

    /// Runs `work` on session `id` under the session's own lock, weighs the session afterwards and drops idle sessions
    /// beyond the limit. Answers what `work` answered and how many sessions were dropped, or why there was no session.
    /// Blocks while another request holds the same session.
    pub fn with_session<R>(
        &self,
        id: u64,
        work: impl FnOnce(&mut EngineSession) -> R,
    ) -> Result<(R, usize), Missing> {
        let session = {
            let mut book = self.book();
            book.clock += 1;
            let clock = book.clock;
            let kept = book.open.get_mut(&id).ok_or(Missing::Unknown)?;
            kept.active += 1;
            kept.used = clock;
            Arc::clone(&kept.session)
        };
        // The session's lock: one request at a time. A request that panicked poisoned it; that session is given up.
        let outcome = match session.lock() {
            Ok(mut guard) => {
                // A panic in `work` unwinds through here and poisons the lock; the store's book stays consistent, and
                // `active` is given back below only on the normal path, so the next request finds the poisoned lock and
                // drops the session.
                let answer = work(&mut guard);
                Ok((answer, guard.heap_estimate()))
            }
            Err(_) => Err(Missing::Failed),
        };
        let mut book = self.book();
        book.clock += 1;
        let clock = book.clock;
        match outcome {
            Err(missing) => {
                // Only this handle's entry, and only if it is still the same session (handles are never reused).
                if book
                    .open
                    .get(&id)
                    .is_some_and(|k| Arc::ptr_eq(&k.session, &session))
                {
                    book.open.remove(&id);
                }
                Err(missing)
            }
            Ok((answer, bytes)) => {
                if let Some(kept) = book.open.get_mut(&id) {
                    kept.active -= 1;
                    kept.bytes = bytes;
                    kept.used = clock;
                }
                let dropped = self.keep_within_limit(&mut book, id);
                Ok((answer, dropped))
            }
        }
    }

    /// Runs one request given as JSON text on session `id` (`EngineSession::run`) and answers as JSON text. When the
    /// request made other sessions go for the limit, the answer says how many (`evictedSessions`), as `SessionStore`'s.
    pub fn run(&self, id: u64, request: &str) -> String {
        match self.with_session(id, |session| session.run(request)) {
            Ok((answer, 0)) => answer,
            Ok((answer, dropped)) => with_field(answer, "evictedSessions", dropped),
            Err(missing) => missing.answer(),
        }
    }

    /// Drops the least recently used idle sessions other than `current` while the store weighs more than its limit;
    /// answers how many were dropped.
    fn keep_within_limit(&self, book: &mut Book, current: u64) -> usize {
        let Some(limit) = self.limit else { return 0 };
        let mut dropped = 0;
        while book.bytes() > limit {
            let oldest = book
                .open
                .iter()
                // Only idle sessions that hold something: a running one is never dropped (ADR 0024, item 2), and dropping
                // an empty one (just opened, its import still to come) frees nothing.
                .filter(|(id, k)| **id != current && k.active == 0 && k.bytes > 0)
                .min_by_key(|(id, k)| (k.used, **id))
                .map(|(id, _)| *id);
            match oldest {
                Some(id) => {
                    book.open.remove(&id);
                    book.evicted += 1;
                    dropped += 1;
                }
                // Only running, empty or the current session are left: they stay (the limit is soft while they run).
                None => break,
            }
        }
        dropped
    }
}

/// Adds `"name": value` to a JSON object given as text (the answer is otherwise passed through byte for byte).
fn with_field(mut answer: String, name: &str, value: usize) -> String {
    if answer.ends_with('}') && answer.len() > 2 {
        answer.pop();
        answer.push_str(&format!(",\"{name}\":{value}}}"));
    }
    answer
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graph(n: usize) -> String {
        let nodes: Vec<String> = (0..n)
            .map(|i| format!(r#"{{"id":"n{i}","kind":"service","label":"Node {i}"}}"#))
            .collect();
        let edges: Vec<String> = (0..n)
            .map(|i| {
                format!(
                    r#"{{"id":"e{i}","kind":"calls","source":"n{i}","target":"n{}"}}"#,
                    (i * 7 + 3) % n
                )
            })
            .collect();
        format!(
            r#"{{"nodes":[{}],"edges":[{}]}}"#,
            nodes.join(","),
            edges.join(",")
        )
    }

    // A few requests on one session: import, a direction change, the scene, a bookmark of the view.
    fn script(n: usize) -> Vec<String> {
        vec![
            format!(r#"{{"op":"import","graph":{},"limit":200}}"#, graph(n)),
            r#"{"op":"direction","direction":"lr"}"#.into(),
            r#"{"op":"scene"}"#.into(),
            r#"{"op":"intent"}"#.into(),
        ]
    }

    #[test]
    fn answers_like_a_single_session() {
        let store = SharedSessionStore::default();
        let id = store.open();
        let mut alone = EngineSession::new();
        for request in script(12) {
            assert_eq!(store.run(id, &request), alone.run(&request));
        }
        assert!(store.close(id));
        assert_eq!(
            store.run(id, "{\"op\":\"scene\"}"),
            Missing::Unknown.answer()
        );
        assert!(!store.close(id));
    }

    #[test]
    fn sessions_in_parallel_answer_as_in_series() {
        let sizes = [8, 13, 21, 34, 9, 17];
        let serial: Vec<Vec<String>> = sizes
            .iter()
            .map(|&n| {
                let mut s = EngineSession::new();
                script(n).iter().map(|r| s.run(r)).collect()
            })
            .collect();
        let store = SharedSessionStore::default();
        let ids: Vec<u64> = sizes.iter().map(|_| store.open()).collect();
        let parallel: Vec<Vec<String>> = std::thread::scope(|scope| {
            let handles: Vec<_> = sizes
                .iter()
                .zip(&ids)
                .map(|(&n, &id)| {
                    let store = &store;
                    scope.spawn(move || script(n).iter().map(|r| store.run(id, r)).collect())
                })
                .collect();
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });
        assert_eq!(serial, parallel);
    }

    #[test]
    fn a_running_session_is_never_dropped() {
        let store = SharedSessionStore::with_limit(Some(1));
        let (a, b) = (store.open(), store.open());
        let import = &script(10)[0];
        store.run(a, import);
        // While `a` holds its lock (it is running), `b` grows past the limit: `a` stays.
        let (_, dropped) = store
            .with_session(a, |_| {
                let answer = store.run(b, import);
                assert!(!answer.contains("evictedSessions"), "{answer}");
            })
            .unwrap();
        // `a` finished: now it is the current one, and `b` (idle, least recently used) goes.
        assert_eq!(dropped, 1);
        assert_eq!(store.ids(), vec![a]);
        assert_eq!(store.evicted(), 1);
        assert_eq!(
            store.run(b, "{\"op\":\"scene\"}"),
            Missing::Unknown.answer()
        );
    }

    #[test]
    fn the_least_recently_used_idle_session_goes_first() {
        let import = script(10)[0].clone();
        let probe = SharedSessionStore::default();
        let p = probe.open();
        probe.run(p, &import);
        let one = probe.bytes();
        // Room for two sessions of this size.
        let store = SharedSessionStore::with_limit(Some(2 * one + one / 2));
        let (a, b, c) = (store.open(), store.open(), store.open());
        store.run(a, &import);
        store.run(b, &import);
        store.run(a, "{\"op\":\"intent\"}");
        let answer = store.run(c, &import);
        assert!(answer.contains("\"evictedSessions\":1"), "{answer}");
        assert_eq!(store.ids(), vec![a, c]);
        assert!(store.list().contains("\"evicted\":1"));
    }

    #[test]
    fn a_session_whose_request_panicked_is_closed() {
        let store = SharedSessionStore::default();
        let (a, b) = (store.open(), store.open());
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            store.with_session(a, |_| panic!("deliberate")).ok();
        }));
        assert!(result.is_err());
        assert_eq!(store.with_session(a, |_| ()).unwrap_err(), Missing::Failed);
        assert_eq!(store.ids(), vec![b]);
        assert!(store.run(b, &script(8)[0]).contains("\"ok\":true"));
    }
}
