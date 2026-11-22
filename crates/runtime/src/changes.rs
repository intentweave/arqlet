//! Provisional (ADR 0023, step R2): the change log of a session. Every change of the source, structure (`patch`) or content
//! (`payload`) or both at once (`change`), is one numbered group in one sequence; `changesSince` answers the groups after
//! a number, so that a replica that took a snapshot at that number can apply them in order and arrive at the same state.
//! The log is bounded by count and bytes; a replica that has fallen out of it is told to resync (take a new snapshot and
//! the content). View decisions (open, close, filter, …) are not changes of the source and are not logged.
use serde_json::{json, Value};
use std::collections::VecDeque;

/// How many groups and how many bytes of heap (estimated, see `crate::memory`) the log keeps at most.
pub const MAX_GROUPS: usize = 1_000;
pub const MAX_BYTES: usize = 16 << 20;

/// One change group: its number and what it changed (`patch`, `payload`, or both).
#[derive(Clone, Debug)]
struct Group {
    sequence: u64,
    change: Value,
    bytes: usize,
}

/// The change log of a session.
#[derive(Clone, Debug, Default)]
pub struct ChangeLog {
    /// The number of the last change (0 before the first). It never goes back, also not when the log is cleared.
    pub sequence: u64,
    groups: VecDeque<Group>,
    bytes: usize,
    /// The number up to which the log was cleared: changes up to it are not in the log any more.
    cleared_at: u64,
}

impl ChangeLog {
    /// Appends a group and answers its number.
    pub fn push(&mut self, change: Value) -> Result<u64, String> {
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or("Change sequence overflow")?;
        // Bounded by the heap the group holds (estimated), not by its JSON text: a value takes several times its text.
        let bytes = crate::memory::value(&change);
        // A group larger than the whole log (a bulk load of content) is not kept twice: the log forgets everything up to
        // it, so a replica behind it resyncs (takes the snapshot and the content) instead.
        if bytes > MAX_BYTES {
            self.clear();
            return Ok(self.sequence);
        }
        self.groups.push_back(Group {
            sequence: self.sequence,
            change,
            bytes,
        });
        self.bytes += bytes;
        while self.groups.len() > MAX_GROUPS || (self.bytes > MAX_BYTES && self.groups.len() > 1) {
            if let Some(old) = self.groups.pop_front() {
                self.bytes -= old.bytes;
                self.cleared_at = old.sequence;
            }
        }
        Ok(self.sequence)
    }

    /// The heap the log holds (estimated, see `crate::memory`).
    pub fn heap_estimate(&self) -> usize {
        self.groups.capacity() * std::mem::size_of::<Group>()
            + self
                .groups
                .iter()
                .map(|g| crate::memory::value(&g.change))
                .sum::<usize>()
    }

    /// Forgets every group (a new graph: a replica has to resync); the sequence goes on.
    pub fn clear(&mut self) {
        // A new list, so that the room of the old one is given back too.
        self.groups = VecDeque::new();
        self.bytes = 0;
        self.cleared_at = self.sequence;
    }

    /// The groups after `since`, or `resync` when some of them are not in the log any more.
    pub fn since(&self, since: u64) -> Value {
        if since > self.sequence {
            return json!({"ok": false, "error": format!("Unknown change {since}: the log is at {}", self.sequence)});
        }
        if since < self.cleared_at {
            return json!({"ok": true, "sequence": self.sequence, "resync": true});
        }
        let changes: Vec<Value> = self
            .groups
            .iter()
            .filter(|g| g.sequence > since)
            .map(|g| {
                let mut entry = g.change.clone();
                entry["sequence"] = json!(g.sequence);
                entry
            })
            .collect();
        json!({"ok": true, "sequence": self.sequence, "changes": changes})
    }
}
