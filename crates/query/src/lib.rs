//! Provisional (ADR 0021): the neutral part of the Arqlet Cypher query layer.
//!
//! It holds the error classes, the values, the plan IR a front end lowers a query into ([`plan`]), the seam to the graph
//! ([`store`]) and the executor that runs a plan over a store ([`exec`]). The front end for openCypher is
//! `arqlet-query-cypher`.
//!
//! The crate has no dependency of its own and must stay free of the layout engine: `tools/check-query-isolation.mjs`
//! fails the build when a layout, routing, scene, view, topology, ELK, reference or runtime crate appears in its tree.
#![forbid(unsafe_code)]

pub mod exec;
mod functions;
pub mod plan;
pub mod store;
pub mod value;

pub use exec::{execute, Limits, ResultSet};
pub use functions::MAX_VALUE_BYTES;
pub use plan::Plan;
pub use store::{Direction, EdgeRef, GraphStore};
pub use value::{EdgeId, NodeId, Value};

use std::fmt;

/// What kind of refusal an answer is. The names are those of `examples/cypher/cases.jsonl` (provisional).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ErrorClass {
    /// The query is not valid Cypher.
    Syntax,
    /// The query is valid Cypher that this layer does not run: a construct outside the first release, a construct of the
    /// ADR's "Later" column, a function that is not in the function table, a construct not built yet, or a query beyond
    /// a size limit. Never a silent change of the query.
    Unsupported,
    /// The query is valid and runs, and a value stops it: an integer overflow, a division by zero, a type that does not
    /// fit an operator, a negative `LIMIT`, a parameter that was not supplied.
    Runtime,
    /// A limit of the execution stopped the query (ADR 0021 item 11): the work budget or the row limit. The answer says
    /// so; it is never a shortened result presented as complete.
    Limit,
}

impl ErrorClass {
    /// The name used in the fixtures (`syntax`, `unsupported`) and, for the classes of execution, `runtime` and `limit`.
    pub const fn as_str(self) -> &'static str {
        match self {
            ErrorClass::Syntax => "syntax",
            ErrorClass::Unsupported => "unsupported",
            ErrorClass::Runtime => "runtime",
            ErrorClass::Limit => "limit",
        }
    }
}

/// A refused query: its class and a message that names the construct.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryError {
    pub class: ErrorClass,
    pub message: String,
}

impl QueryError {
    pub fn syntax(message: impl Into<String>) -> Self {
        QueryError {
            class: ErrorClass::Syntax,
            message: message.into(),
        }
    }

    pub fn unsupported(message: impl Into<String>) -> Self {
        QueryError {
            class: ErrorClass::Unsupported,
            message: message.into(),
        }
    }

    pub fn runtime(message: impl Into<String>) -> Self {
        QueryError {
            class: ErrorClass::Runtime,
            message: message.into(),
        }
    }

    pub fn limit(message: impl Into<String>) -> Self {
        QueryError {
            class: ErrorClass::Limit,
            message: message.into(),
        }
    }
}

impl fmt::Display for QueryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.class.as_str(), self.message)
    }
}

impl std::error::Error for QueryError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_class_names_are_those_of_the_fixtures() {
        assert_eq!(ErrorClass::Syntax.as_str(), "syntax");
        assert_eq!(ErrorClass::Unsupported.as_str(), "unsupported");
    }

    #[test]
    fn an_error_reads_as_its_class_and_message() {
        let e = QueryError::unsupported("CREATE is outside the first release");
        assert_eq!(e.class, ErrorClass::Unsupported);
        assert_eq!(
            e.to_string(),
            "unsupported: CREATE is outside the first release"
        );
    }
}
