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
//! to (the sessions are lost then).
use arqlet_runtime::engine_session::EngineSession;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::{self, BufRead, Read, Write};

fn main() {
    if std::env::args().any(|a| a == "--sessions") {
        return sessions();
    }
    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();
    println!("{}", arqlet_runtime::dispatch(&input));
}

fn sessions() {
    let mut open: BTreeMap<u64, EngineSession> = BTreeMap::new();
    let mut next = 0u64;
    let stdout = io::stdout();
    for line in io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let answer = match serde_json::from_str::<Value>(&line) {
            Err(e) => json!({"ok": false, "error": e.to_string()}).to_string(),
            Ok(message) => match &message["session"] {
                Value::String(s) if s == "open" => {
                    next += 1;
                    open.insert(next, EngineSession::new());
                    json!({"ok": true, "session": next}).to_string()
                }
                Value::Number(n) => match (n.as_u64(), message["close"].as_bool()) {
                    (Some(id), Some(true)) => {
                        open.remove(&id);
                        json!({"ok": true}).to_string()
                    }
                    (Some(id), _) => match open.get_mut(&id) {
                        Some(session) => session.run_value(&message["request"]).to_string(),
                        None => json!({"ok": false, "error": "Unknown session"}).to_string(),
                    },
                    _ => json!({"ok": false, "error": "Invalid session"}).to_string(),
                },
                Value::Null => arqlet_runtime::dispatch(&message["request"].to_string()),
                _ => json!({"ok": false, "error": "Invalid session"}).to_string(),
            },
        };
        let mut out = stdout.lock();
        if writeln!(out, "{answer}").and_then(|_| out.flush()).is_err() {
            break;
        }
    }
}
