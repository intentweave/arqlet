//! Provisional (ADR 0021): the openCypher front end of the Arqlet query layer.
//!
//! [`check`] parses a query with decypher and runs the **guard**: every construct either runs correctly or is rejected,
//! never silently changed. decypher drops some constructs on the way from its syntax tree to its AST and HIR
//! (`shortestPath`, quantified paths, `WHERE` inside a pattern; upstream issues #132 to #134), so the guard walks the
//! syntax tree, which still holds all of them, before anything is lowered.
//!
//! [`lower`] then turns a checked query into a plan of `arqlet-query`, and [`run`] lowers and executes it. The lowering
//! reads decypher's HIR and takes what the HIR loses (the order of the columns) from the syntax tree.
#![forbid(unsafe_code)]

mod lower;

pub use lower::lower;

use arqlet_query::{execute, GraphStore, Limits, QueryError, ResultSet, Value};
use decypher::syntax::{SyntaxElement, SyntaxKind as K, SyntaxNode};
use std::collections::BTreeMap;

/// Checks, lowers and executes a query over a store. `params` are the values of the `$name` parameters.
pub fn run(
    query: &str,
    store: &dyn GraphStore,
    params: &BTreeMap<String, Value>,
    limits: Limits,
) -> Result<ResultSet, QueryError> {
    execute(&lower(query)?, store, params, limits)
}

/// The longest query accepted, in bytes (provisional). decypher itself refuses expressions nested deeper than 128 levels.
pub const MAX_QUERY_BYTES: usize = 64 * 1024;

/// The functions the first release knows (provisional; the function table is still to be decided, ADR 0021 item 4). A
/// function that is not listed is rejected, not passed on: decypher accepts any name.
pub const FUNCTIONS: &[&str] = &["avg", "collect", "count", "max", "min", "sum"];

/// Checks a query: `Ok` when it is valid openCypher and uses nothing the guard rejects.
///
/// An `Err` of class `Syntax` means the query is not valid Cypher; of class `Unsupported`, that it is valid but outside
/// what this layer runs (a write, a procedure, a construct of the ADR's "Later" column, an unknown function, a query
/// beyond [`MAX_QUERY_BYTES`]). The message names the construct and, for the constructs decypher drops, the upstream issue.
pub fn check(query: &str) -> Result<(), QueryError> {
    if query.len() > MAX_QUERY_BYTES {
        return Err(QueryError::unsupported(format!(
            "the query is {} bytes; the limit is {MAX_QUERY_BYTES}",
            query.len()
        )));
    }
    let parsed = decypher::parse_cst(query);
    if let Some(first) = parsed.errors.first() {
        return Err(QueryError::syntax(first.to_string()));
    }
    guard(&parsed.tree)?;
    // The AST is built from the same tree and can still refuse what the tree accepts.
    decypher::parse(query).map_err(|e| QueryError::syntax(e.to_string()))?;
    Ok(())
}

/// Walks the syntax tree in document order and rejects the first construct outside the first release.
fn guard(tree: &SyntaxNode) -> Result<(), QueryError> {
    for element in tree.descendants_with_tokens() {
        if let Some(reason) = reject(&element) {
            return Err(QueryError::unsupported(reason));
        }
    }
    Ok(())
}

/// Why an element of the syntax tree is not run, or `None` when it is.
fn reject(element: &SyntaxElement) -> Option<String> {
    let kind = element.kind();
    let reason = match kind {
        // Writes and administration: the first release only reads.
        K::CREATE_CLAUSE | K::MERGE_CLAUSE | K::SET_CLAUSE | K::DELETE_CLAUSE | K::REMOVE_CLAUSE | K::FOREACH_CLAUSE => {
            "a write (CREATE, MERGE, SET, DELETE, REMOVE, FOREACH) is outside the first release, which only reads"
        }
        K::LOAD_CSV_CLAUSE => "LOAD CSV is outside the first release",
        K::CREATE_INDEX | K::DROP_INDEX | K::CREATE_CONSTRAINT | K::DROP_CONSTRAINT | K::SCHEMA_COMMAND | K::SHOW_CLAUSE | K::USE_CLAUSE => {
            "schema and administration commands are outside the first release"
        }
        // Procedures and CALL subqueries: only procedures the layer defines will be allowed, and none exists yet.
        K::STANDALONE_CALL | K::IN_QUERY_CALL | K::CALL_SUBQUERY_CLAUSE => {
            "CALL (a procedure or a subquery) is outside the first release; no procedure is defined yet"
        }
        // The ADR's "Later" column.
        K::EXISTS_SUBQUERY | K::COUNT_SUBQUERY | K::COLLECT_SUBQUERY => {
            "EXISTS, COUNT and COLLECT subqueries are not supported yet (ADR 0021, Later)"
        }
        K::QUANTIFIED_PATH_PATTERN | K::RELATIONSHIP_QUANTIFIER => {
            "quantified paths and path patterns are not supported yet; decypher lowers them to a single hop (upstream #134)"
        }
        K::TILDE_EQ => "regular expressions (=~) are not supported yet (ADR 0021, Later)",
        K::WHERE_CLAUSE => {
            let inside_pattern = element.as_node()?.parent().map(|p| p.kind());
            match inside_pattern {
                Some(K::NODE_PATTERN | K::RELATIONSHIP_DETAIL | K::RELATIONSHIP_PATTERN) => {
                    "WHERE inside a node or relationship pattern is not supported yet; decypher drops it (upstream #132)"
                }
                _ => return None,
            }
        }
        K::IDENT => {
            let token = element.as_token()?;
            let text = token.text();
            if text.eq_ignore_ascii_case("shortestPath") || text.eq_ignore_ascii_case("allShortestPaths") {
                "shortestPath and allShortestPaths are not supported yet; decypher drops them (upstream #133)"
            } else {
                return None;
            }
        }
        K::FUNCTION_INVOCATION => {
            let node = element.as_node()?;
            let name = node
                .children()
                .find(|c| c.kind() == K::FUNCTION_NAME)
                .map(|c| c.text().to_string())
                .unwrap_or_default();
            if FUNCTIONS.iter().any(|f| f.eq_ignore_ascii_case(name.trim())) {
                return None;
            }
            return Some(format!("the function `{}` is not in the function table", name.trim()));
        }
        _ => return None,
    };
    Some(reason.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use arqlet_query::ErrorClass;

    fn class(query: &str) -> Option<ErrorClass> {
        check(query).err().map(|e| e.class)
    }

    #[test]
    fn plain_reads_pass() {
        for q in [
            "MATCH (n:Person) RETURN n",
            "MATCH (a)-[r:KNOWS]->(b) WHERE a.age > 3 RETURN a.name, count(b) AS c ORDER BY c DESC LIMIT 5",
            "MATCH (a)-[:KNOWS*1..3]->(b) RETURN DISTINCT b",
            "OPTIONAL MATCH (n) WITH n, count(*) AS k RETURN n, k",
            "UNWIND [1, 2, 3] AS x RETURN x * 2",
        ] {
            assert_eq!(class(q), None, "{q}");
        }
    }

    #[test]
    fn a_variable_length_path_is_not_a_quantified_path() {
        assert_eq!(class("MATCH (a)-[:K*2..4]->(b) RETURN b"), None);
        assert_eq!(class("MATCH (a)-[:K*]->(b) RETURN b"), None);
    }

    #[test]
    fn syntax_errors_are_syntax_errors() {
        for q in ["MATCH (n RETURN n", "MATCH (n) RETRUN n", "RETURN", ""] {
            assert_eq!(class(q), Some(ErrorClass::Syntax), "{q:?}");
        }
    }

    // One test per construct that decypher drops silently: the guard has to see it in the syntax tree.

    #[test]
    fn shortest_path_is_rejected_although_decypher_drops_it() {
        let e = check("MATCH p = shortestPath((a)-[*..5]-(b)) RETURN p").unwrap_err();
        assert_eq!(e.class, ErrorClass::Unsupported);
        assert!(e.message.contains("#133"), "{e}");
        assert_eq!(
            class("MATCH p = allShortestPaths((a)-[*]-(b)) RETURN p"),
            Some(ErrorClass::Unsupported)
        );
        assert_eq!(
            class("MATCH p = SHORTESTPATH((a)-[*]-(b)) RETURN p"),
            Some(ErrorClass::Unsupported)
        );
    }

    #[test]
    fn quantified_paths_are_rejected_although_decypher_lowers_them_to_one_hop() {
        let e = check("MATCH (a)-[:KNOWS]->{1,3}(b) RETURN b").unwrap_err();
        assert_eq!(e.class, ErrorClass::Unsupported);
        assert!(e.message.contains("#134"), "{e}");
        assert_eq!(
            class("MATCH (a)((x)-[:K]->(y)){2}(b) RETURN b"),
            Some(ErrorClass::Unsupported)
        );
    }

    #[test]
    fn where_inside_a_pattern_is_rejected_although_decypher_drops_it() {
        let e = check("MATCH (n WHERE n.age > 3) RETURN n").unwrap_err();
        assert_eq!(e.class, ErrorClass::Unsupported);
        assert!(e.message.contains("#132"), "{e}");
        assert_eq!(
            class("MATCH (a)-[r:K WHERE r.x > 1]->(b) RETURN a"),
            Some(ErrorClass::Unsupported)
        );
    }

    #[test]
    fn where_after_match_is_not_where_inside_a_pattern() {
        assert_eq!(class("MATCH (n) WHERE n.age > 3 RETURN n"), None);
        assert_eq!(class("MATCH (a)-[r:K]->(b) WHERE r.x > 1 RETURN a"), None);
    }

    #[test]
    fn unknown_functions_are_rejected_although_decypher_accepts_them() {
        let e = check("RETURN foo_bar(1) AS x").unwrap_err();
        assert_eq!(e.class, ErrorClass::Unsupported);
        assert!(e.message.contains("foo_bar"), "{e}");
        assert_eq!(
            class("MATCH (n) RETURN COUNT(n), Sum(n.x), collect(n)"),
            None
        );
    }

    // The rest of what the first release does not run.

    #[test]
    fn writes_procedures_and_subqueries_are_rejected() {
        for q in [
            "CREATE (n:Person {name: 'x'})",
            "MATCH (n) DETACH DELETE n",
            "MATCH (n) SET n.x = 1",
            "MERGE (n:Person {id: 1})",
            "MATCH (n) REMOVE n.x",
            "CALL db.index.vector.queryNodes('e', 5, $v) YIELD node RETURN node",
            "MATCH (a) WHERE EXISTS { MATCH (a)-[:K]->(b) } RETURN a",
            "MATCH (n) WHERE n.name =~ 'B.*' RETURN n",
        ] {
            assert_eq!(class(q), Some(ErrorClass::Unsupported), "{q}");
        }
    }

    #[test]
    fn a_query_beyond_the_size_limit_is_unsupported_not_parsed() {
        let long = format!("RETURN {}", "1 + ".repeat(MAX_QUERY_BYTES / 4) + "1");
        let e = check(&long).unwrap_err();
        assert_eq!(e.class, ErrorClass::Unsupported);
        assert!(e.message.contains("limit"), "{e}");
    }

    #[test]
    fn deep_nesting_is_refused_by_the_parser_not_a_stack_overflow() {
        let deep = format!("RETURN {}1{}", "(".repeat(5000), ")".repeat(5000));
        assert!(check(&deep).is_err());
    }
}
