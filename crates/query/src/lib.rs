//! Provisional (ADR 0021): the neutral part of the Arqlet Cypher query layer.
//!
//! Today it holds the error classes that the front end (`arqlet-query-cypher`) and, later, the executor answer with. The
//! plan IR, the store seam and the executor follow in stages M1 to M4 of the ADR.
//!
//! The crate has no dependency of its own and must stay free of the layout engine: `tools/check-query-isolation.mjs`
//! fails the build when a layout, routing, scene, view, topology, ELK, reference or runtime crate appears in its tree.
#![forbid(unsafe_code)]

use std::fmt;

/// What kind of refusal an answer is. The names are those of `examples/cypher/cases.jsonl` (provisional).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ErrorClass {
    /// The query is not valid Cypher.
    Syntax,
    /// The query is valid Cypher that this layer does not run: a construct outside the first release, a construct of the
    /// ADR's "Later" column, a function that is not in the function table, or a query beyond a limit. Never a silent
    /// change of the query.
    Unsupported,
}

impl ErrorClass {
    /// The name used in the fixtures: `syntax` or `unsupported`.
    pub const fn as_str(self) -> &'static str {
        match self {
            ErrorClass::Syntax => "syntax",
            ErrorClass::Unsupported => "unsupported",
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
