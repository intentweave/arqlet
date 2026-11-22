//! One request on stdin, its answer on stdout (`dispatch`).
//!
//! With `--sessions` (provisional, ADR 0019 step 3, for a backend): a long-running process that keeps sessions whose
//! snapshot stays here. It reads one JSON message per line and answers each with one line:
//! - `{"session": "open"}` → `{"ok": true, "session": <id>}`;
//! - `{"session": <id>, "request": {…}}` → the session's answer (see `arqlet_runtime::engine_session`);
//! - `{"session": <id>, "close": true}` → `{"ok": true}`;
//! - `{"request": {…}}` → `dispatch`, without a session.
//!
//! The host serializes the messages, bounds their time and the number of sessions, and restarts the process if it has
//! to (the sessions are lost then). With `--session-bytes N` the process drops the least recently used sessions while the
//! sessions together weigh more than `N` bytes (estimated); a message to a dropped one is answered as an unknown session.
use serde_json::{json, Value};
use std::io::{self, BufRead, Read, Write};

fn main() {
    // A clock for phase timings (`timings: true` on a request); the layout never reads it.
    static ORIGIN: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    ORIGIN.get_or_init(std::time::Instant::now);
    arqlet_model::clock::install(|| {
        ORIGIN
            .get()
            .map_or(0., |o| o.elapsed().as_secs_f64() * 1000.)
    });
    if std::env::args().any(|a| a == "--sessions") {
        return sessions();
    }
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();
    println!("{}", arqlet_runtime::dispatch(&input));
}

fn sessions() {
    // Provisional (ADR 0023, R7): `--session-bytes N` limits the sessions' weight together; the least recently used ones
    // are dropped beyond it (and answered as unknown afterwards).
    let args: Vec<String> = std::env::args().collect();
    let limit = args
        .iter()
        .position(|a| a == "--session-bytes")
        .and_then(|i| args.get(i + 1))
        .and_then(|n| n.parse::<usize>().ok());
    let mut store = arqlet_runtime::session_store::SessionStore::with_limit(limit);
    let stdout = io::stdout();
    for line in io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let answer = match serde_json::from_str::<Value>(&line) {
            Err(e) => json!({"ok": false, "error": e.to_string()}).to_string(),
            Ok(message) => store.handle(&message),
        };
        let mut out = stdout.lock();
        if writeln!(out, "{answer}").and_then(|_| out.flush()).is_err() {
            break;
        }
    }
}
