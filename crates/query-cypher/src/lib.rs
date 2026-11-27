//! Provisional (ADR 0021): the openCypher front end of the Arqlet query layer.
//!
//! [`check`] parses a query with decypher and runs the **guard**: every construct either runs correctly or is rejected,
//! never silently changed. The guard walks the syntax tree for what is outside the first release (writes, procedures,
//! subqueries, unknown functions, `RETURN *`), then the HIR's patterns for what the executor does not run yet (path selectors such as
//! `shortestPath`, quantified paths, `WHERE` inside a pattern, path variables). The lowering never sees those.
//!
//! [`lower`] then turns a checked query into a plan of `arqlet-query`, and [`run`] lowers and executes it. The lowering
//! reads decypher's HIR only, the order of the columns included.
#![forbid(unsafe_code)]

mod lower;

pub use lower::lower;

use arqlet_query::{execute, GraphStore, Limits, QueryError, ResultSet, Value};
use decypher::hir::ops::Operation;
use decypher::hir::pattern::{GraphPattern, PathSelector};
use decypher::hir::HirQuery;
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

/// The functions the layer knows (provisional, ADR 0021, "The function table"): the aggregates and the scalar functions.
/// A function that is not listed is rejected, not passed on: decypher accepts any name.
pub const FUNCTIONS: &[&str] = &[
    "avg",
    "coalesce",
    "collect",
    "count",
    "elementid",
    "id",
    "labels",
    "left",
    "ltrim",
    "max",
    "min",
    "replace",
    "reverse",
    "right",
    "rtrim",
    "size",
    "split",
    "substring",
    "sum",
    "tolower",
    "tostring",
    "toupper",
    "trim",
    "type",
];

/// Checks a query: `Ok` when it is valid openCypher and uses nothing the guard rejects.
///
/// An `Err` of class `Syntax` means the query is not valid Cypher; of class `Unsupported`, that it is valid but outside
/// what this layer runs (a write, a procedure, a construct of the ADR's "Later" column, an unknown function, a query
/// beyond [`MAX_QUERY_BYTES`]). The message names the construct.
pub fn check(query: &str) -> Result<(), QueryError> {
    checked(query).map(|_| ())
}

/// [`check`], returning the HIR for the lowering.
fn checked(query: &str) -> Result<HirQuery, QueryError> {
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
    // The AST and the HIR are built from the same tree and can still refuse what the tree accepts.
    let hir = decypher::analyze(query).map_err(|e| QueryError::syntax(e.to_string()))?;
    guard_patterns(&hir)?;
    Ok(hir)
}

/// Rejects the first pattern of a `MATCH` or `OPTIONAL MATCH` that uses what the executor does not run yet.
fn guard_patterns(hir: &HirQuery) -> Result<(), QueryError> {
    for op in hir.parts.iter().flat_map(|p| &p.operations) {
        match op {
            Operation::Match(m) | Operation::OptionalMatch(m) => pattern_support(&m.pattern)?,
            Operation::Union(u) => u.branches.iter().try_for_each(guard_patterns)?,
            _ => {}
        }
    }
    Ok(())
}

/// Why a pattern is not run, as an error, or `Ok` when it is.
fn pattern_support(pattern: &GraphPattern) -> Result<(), QueryError> {
    let reason = if pattern
        .paths
        .iter()
        .any(|p| p.selector != PathSelector::All)
    {
        "path selectors (shortestPath, allShortestPaths, ANY, SHORTEST) are not supported yet (ADR 0021, Later)"
    } else if !pattern.quantified.is_empty() {
        "quantified paths and path patterns are not supported yet (ADR 0021, Later)"
    } else if pattern.nodes.iter().any(|n| n.predicate.is_some())
        || pattern.relationships.iter().any(|r| r.predicate.is_some())
    {
        "WHERE inside a node or relationship pattern is not supported yet (ADR 0021, Later)"
    } else if pattern.paths.iter().any(|p| p.binding.is_some()) {
        "a path variable is undecided (ADR 0021, open)"
    } else {
        return Ok(());
    };
    Err(QueryError::unsupported(reason))
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
        K::TILDE_EQ => "regular expressions (=~) are not supported yet (ADR 0021, Later)",
        K::STAR => {
            let items = element.parent()?;
            if items.kind() == K::PROJECTION_ITEMS && items.ancestors().any(|a| a.kind() == K::RETURN_CLAUSE) {
                "RETURN * is not supported yet"
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

    // One test per pattern construct the executor does not run yet: the guard reads it from the HIR.

    fn unsupported_naming(query: &str, construct: &str) {
        let e = check(query).unwrap_err();
        assert_eq!(e.class, ErrorClass::Unsupported, "{query}: {e}");
        assert!(e.message.contains(construct), "{query}: {e}");
    }

    #[test]
    fn path_selectors_are_rejected() {
        for q in [
            "MATCH p = shortestPath((a)-[*..5]-(b)) RETURN p",
            "MATCH p = allShortestPaths((a)-[*]-(b)) RETURN p",
            "MATCH p = SHORTESTPATH((a)-[*]-(b)) RETURN p",
            "MATCH ANY SHORTEST (a)-[*]-(b) RETURN b",
            "MATCH SHORTEST 2 (a)-[*]-(b) RETURN b",
            "MATCH SHORTEST 1 GROUPS (a)-[*]-(b) RETURN b",
            "MATCH ANY (a)-->(b) RETURN b",
            "OPTIONAL MATCH ALL SHORTEST (a)-[*]-(b) RETURN b",
        ] {
            unsupported_naming(q, "path selectors");
        }
    }

    #[test]
    fn the_selector_all_is_a_plain_pattern() {
        assert_eq!(class("MATCH ALL (a)-->(b) RETURN b"), None);
    }

    #[test]
    fn quantified_paths_are_rejected() {
        for q in [
            "MATCH (a)-[:KNOWS]->{1,3}(b) RETURN b",
            "MATCH (a)-[:KNOWS]->{,3}(b) RETURN b",
            "MATCH (a)-[:KNOWS]->+(b) RETURN b",
            "MATCH (a)-[:KNOWS]->*(b) RETURN b",
            "MATCH (a)((x)-[:K]->(y)){2}(b) RETURN b",
            "OPTIONAL MATCH (a)((x)-[:K]->(y)){1,2}(b) RETURN b",
        ] {
            unsupported_naming(q, "quantified paths");
        }
    }

    #[test]
    fn where_inside_a_pattern_is_rejected() {
        for q in [
            "MATCH (n WHERE n.age > 3) RETURN n",
            "MATCH (a)-[r:K WHERE r.x > 1]->(b) RETURN a",
            "OPTIONAL MATCH (a)-->(b WHERE b.x = 1) RETURN a",
        ] {
            unsupported_naming(q, "WHERE inside");
        }
    }

    #[test]
    fn a_path_variable_is_rejected() {
        unsupported_naming("MATCH p = (a)-->(b) RETURN p", "path variable");
    }

    #[test]
    fn a_pattern_in_a_union_branch_is_guarded() {
        unsupported_naming(
            "MATCH (a) RETURN a UNION MATCH (a WHERE a.x = 1) RETURN a",
            "WHERE inside",
        );
    }

    #[test]
    fn where_after_match_is_not_where_inside_a_pattern() {
        assert_eq!(class("MATCH (n) WHERE n.age > 3 RETURN n"), None);
        assert_eq!(class("MATCH (a)-[r:K]->(b) WHERE r.x > 1 RETURN a"), None);
    }

    #[test]
    fn unknown_functions_are_rejected() {
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
    fn return_star_is_rejected() {
        assert_eq!(class("MATCH (n) RETURN *"), Some(ErrorClass::Unsupported));
        assert_eq!(class("MATCH (n) RETURN count(*) AS c"), None);
    }

    #[test]
    fn columns_follow_the_return_in_written_order() {
        let names = |q: &str| -> Vec<String> {
            lower(q)
                .unwrap()
                .columns
                .into_iter()
                .map(|c| c.name)
                .collect()
        };
        assert_eq!(
            names("MATCH (n) RETURN count(*) AS c, n.x, n.y AS `a``b`"),
            ["c", "n.x", "a`b"]
        );
        assert_eq!(
            names("MATCH (`my node`) RETURN `my node`.`a b`"),
            ["`my node`.`a b`"]
        );
        // A name ends at the item's last token: the whitespace and comments that follow are not part of it.
        assert_eq!(names("MATCH (a) RETURN a.age + 1 LIMIT 1"), ["a.age + 1"]);
        assert_eq!(
            names("MATCH (a) RETURN a.age > 3 /* c */ , count(a) + 1\n"),
            ["a.age > 3", "count(a) + 1"]
        );
        assert!(lower("MATCH (a) RETURN a.age + 1 , a.age + 1   ").is_err());
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
