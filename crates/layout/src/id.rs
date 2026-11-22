//! A shared identifier for the layout's working data (provisional).
//!
//! The placement keeps the ids of nodes, dummies and links in many maps and sets at once (levels, links, chains, lanes,
//! the memos of a pass), and copies them between passes. As `String`, each copy is a heap allocation of its own; as an
//! [`Id`], a copy is a reference count on one allocation, and a key takes 16 bytes instead of 24. It compares, orders
//! and hashes exactly as the string it holds, so every map and set keeps the order it had with `String` and the output
//! does not change.
use std::borrow::Borrow;
use std::fmt;
use std::ops::Deref;
use std::sync::Arc;

/// A string shared by reference count; ordered, compared and hashed as its text.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct Id(Arc<str>);

impl Id {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Deref for Id {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
}

impl Borrow<str> for Id {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for Id {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Default for Id {
    fn default() -> Self {
        Id(Arc::from(""))
    }
}

impl From<&str> for Id {
    fn from(s: &str) -> Self {
        Id(Arc::from(s))
    }
}

impl From<String> for Id {
    fn from(s: String) -> Self {
        Id(Arc::from(s))
    }
}

impl From<&String> for Id {
    fn from(s: &String) -> Self {
        Id(Arc::from(s.as_str()))
    }
}

impl From<&Id> for Id {
    fn from(s: &Id) -> Self {
        s.clone()
    }
}

impl From<Id> for String {
    fn from(id: Id) -> Self {
        id.0.to_string()
    }
}

impl PartialEq<str> for Id {
    fn eq(&self, other: &str) -> bool {
        &*self.0 == other
    }
}

impl PartialEq<&str> for Id {
    fn eq(&self, other: &&str) -> bool {
        &*self.0 == *other
    }
}

impl PartialEq<String> for Id {
    fn eq(&self, other: &String) -> bool {
        &*self.0 == other.as_str()
    }
}

impl PartialEq<Id> for String {
    fn eq(&self, other: &Id) -> bool {
        self.as_str() == &*other.0
    }
}

impl PartialEq<Id> for str {
    fn eq(&self, other: &Id) -> bool {
        self == &*other.0
    }
}

impl PartialEq<Id> for &str {
    fn eq(&self, other: &Id) -> bool {
        *self == &*other.0
    }
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&*self.0, f)
    }
}

/// Hands out one shared [`Id`] per text, so that the copies of an id made from strings share their allocation too.
#[derive(Default)]
pub(crate) struct Interner {
    known: std::collections::BTreeSet<Id>,
}

impl Interner {
    pub fn id(&mut self, s: &str) -> Id {
        if let Some(id) = self.known.get(s) {
            return id.clone();
        }
        let id = Id::from(s);
        self.known.insert(id.clone());
        id
    }

    /// Levels of strings as levels of shared ids.
    pub fn levels(
        &mut self,
        levels: std::collections::BTreeMap<usize, Vec<String>>,
    ) -> std::collections::BTreeMap<usize, Vec<Id>> {
        levels
            .into_iter()
            .map(|(rank, ids)| (rank, ids.iter().map(|s| self.id(s)).collect()))
            .collect()
    }

    /// A set of string pairs as a set of shared-id pairs (the same order: ids order as their text).
    pub fn pairs(
        &mut self,
        pairs: std::collections::BTreeSet<(String, String)>,
    ) -> std::collections::BTreeSet<(Id, Id)> {
        pairs
            .iter()
            .map(|(a, b)| (self.id(a), self.id(b)))
            .collect()
    }

    /// Chains of strings (a pair of ends with the ids between them) as chains of shared ids.
    pub fn chains(
        &mut self,
        chains: std::collections::BTreeMap<(String, String), Vec<String>>,
    ) -> std::collections::BTreeMap<(Id, Id), Vec<Id>> {
        chains
            .iter()
            .map(|((a, b), ids)| {
                (
                    (self.id(a), self.id(b)),
                    ids.iter().map(|s| self.id(s)).collect(),
                )
            })
            .collect()
    }
}

/// Ranks of shared ids as ranks of strings (for the plans kept beyond one placement).
pub(crate) fn rank_strings<'a>(ranks: impl IntoIterator<Item = &'a Vec<Id>>) -> Vec<Vec<String>> {
    ranks
        .into_iter()
        .map(|ids| ids.iter().map(|id| id.to_string()).collect())
        .collect()
}

/// Levels of shared ids as levels of strings (for helpers of rarely used paths that work on text).
pub(crate) fn level_strings(
    levels: &std::collections::BTreeMap<usize, Vec<Id>>,
) -> std::collections::BTreeMap<usize, Vec<String>> {
    levels
        .iter()
        .map(|(rank, ids)| (*rank, ids.iter().map(|id| id.to_string()).collect()))
        .collect()
}

/// Shared-id pairs as string pairs.
pub(crate) fn pair_strings(
    pairs: &std::collections::BTreeSet<(Id, Id)>,
) -> std::collections::BTreeSet<(String, String)> {
    pairs
        .iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect()
}

/// Shared-id chains as string chains.
pub(crate) fn chain_strings(
    chains: &std::collections::BTreeMap<(Id, Id), Vec<Id>>,
) -> std::collections::BTreeMap<(String, String), Vec<String>> {
    chains
        .iter()
        .map(|((a, b), ids)| {
            (
                (a.to_string(), b.to_string()),
                ids.iter().map(|id| id.to_string()).collect(),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn ids_order_and_compare_as_their_text() {
        let words = ["b", "a", "ab", "", "B", "ä", "a0"];
        let mut strings: Vec<String> = words.iter().map(|s| s.to_string()).collect();
        let mut ids: Vec<Id> = words.iter().map(|&s| Id::from(s)).collect();
        strings.sort();
        ids.sort();
        assert_eq!(
            strings,
            ids.iter().map(|i| i.to_string()).collect::<Vec<_>>()
        );
        let set: BTreeSet<Id> = ids.iter().cloned().collect();
        assert!(set.contains("ab"));
        assert_eq!(Id::from("x"), "x");
        assert_eq!(format!("{:?}", Id::from("x")), format!("{:?}", "x"));
    }

    #[test]
    fn an_interner_shares_one_allocation_per_text() {
        let mut i = Interner::default();
        let a = i.id("node");
        let b = i.id(&String::from("node"));
        assert!(Arc::ptr_eq(&a.0, &b.0));
    }
}
