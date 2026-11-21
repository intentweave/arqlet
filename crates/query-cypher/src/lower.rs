//! Lowering: from decypher's HIR into the plan IR of `arqlet-query` (provisional, ADR 0021 item 2).
//!
//! The executor never sees decypher. This module reads the HIR, which already resolves variables and scopes, and writes
//! the plan. Whatever it cannot lower it refuses with a message that names it; it never guesses.
//!
//! What the HIR does not keep is taken from the syntax tree instead: the order of the columns. The HIR splits an
//! aggregating `RETURN` into grouping keys and aggregates and drops the order the query wrote them in, so the names of the
//! columns are read from the `RETURN` clause and matched to the aliases the HIR created. A name that does not match is a
//! refusal.

use arqlet_query::plan::{
    AggFunc, Aggregate, BinOp, CmpOp, Column, Expand, Expr, Op, Plan, SortKey,
};
use arqlet_query::Direction;
use arqlet_query::{QueryError, Value};
use decypher::hir::arena::ExprId;
use decypher::hir::arena::HirArenas;
use decypher::hir::expr::{BinaryOp, ComparisonOperator, ExprKind, Literal, UnaryOp};
use decypher::hir::ops::{AggregateItem, Operation, SortDirection};
use decypher::hir::pattern::{
    GraphPattern, LabelExpr, NodePattern, RelationshipDirection, RelationshipLength,
};
use decypher::syntax::{SyntaxKind as K, SyntaxNode};
use std::collections::{BTreeMap, BTreeSet};

/// Checks a query (the parser and the guard), then lowers it into a plan.
pub fn lower(query: &str) -> Result<Plan, QueryError> {
    crate::check(query)?;
    let hir = decypher::analyze(query).map_err(|e| QueryError::syntax(e.to_string()))?;
    let tree = decypher::parse_cst(query).tree;
    let slots = hir.arenas.bindings.iter().count();
    let mut lowering = Lowering {
        arenas: &hir.arenas,
        next_slot: slots,
        bound: BTreeSet::new(),
        ops: Vec::new(),
        aliases: BTreeMap::new(),
        distinct_return: false,
    };
    for part in &hir.parts {
        lowering.aliases.clear();
        for op in &part.operations {
            lowering.op(op)?;
        }
    }
    let columns = lowering.columns(&tree)?;
    if lowering.distinct_return {
        let slots = columns.iter().map(|c| c.slot).collect();
        lowering.ops.push(Op::Distinct(slots));
    }
    Ok(Plan {
        slots: lowering.next_slot,
        ops: lowering.ops,
        columns,
    })
}

struct Lowering<'a> {
    arenas: &'a HirArenas,
    /// The next slot for something the HIR gives no binding (an anonymous node).
    next_slot: usize,
    /// The slots bound so far: a node pattern for a bound slot checks, one for a fresh slot scans.
    bound: BTreeSet<usize>,
    ops: Vec<Op>,
    /// The aliases of the current part's projections: name to slot, for the columns.
    aliases: BTreeMap<String, usize>,
    /// `RETURN DISTINCT`: the HIR keeps the flag on the Return operation, not on the projection.
    distinct_return: bool,
}

/// The label and the `{key: value}` constraints of a node pattern.
type Constraints = (Option<String>, Vec<(String, Expr)>);

fn not_yet(what: &str, stage: &str) -> QueryError {
    QueryError::unsupported(format!("{what} is not built yet ({stage} of ADR 0021)"))
}

impl Lowering<'_> {
    fn op(&mut self, op: &Operation) -> Result<(), QueryError> {
        match op {
            Operation::Match(m) => {
                if !m.pattern.path_bindings.is_empty() {
                    return Err(QueryError::unsupported(
                        "a path variable is undecided (ADR 0021, open)",
                    ));
                }
                self.pattern(&m.pattern)?;
                for predicate in &m.predicates {
                    let e = self.expr(*predicate)?;
                    self.ops.push(Op::Filter(e));
                }
                Ok(())
            }
            Operation::OptionalMatch(m) => {
                if !m.pattern.path_bindings.is_empty() {
                    return Err(QueryError::unsupported(
                        "a path variable is undecided (ADR 0021, open)",
                    ));
                }
                // Its operations run as a sub-plan per row; its WHERE belongs to it, not to the rows around it.
                let outer = std::mem::take(&mut self.ops);
                let result = self.pattern(&m.pattern).and_then(|()| {
                    for predicate in &m.predicates {
                        let e = self.expr(*predicate)?;
                        self.ops.push(Op::Filter(e));
                    }
                    Ok(())
                });
                let inner = std::mem::replace(&mut self.ops, outer);
                result?;
                self.ops.push(Op::Optional(inner));
                Ok(())
            }
            Operation::Filter(f) => {
                let e = self.expr(f.predicate)?;
                self.ops.push(Op::Filter(e));
                Ok(())
            }
            Operation::Unwind(u) => {
                let expr = self.expr(u.expression)?;
                let slot = u.variable.0;
                self.bound.insert(slot);
                self.ops.push(Op::Unwind { expr, slot });
                Ok(())
            }
            Operation::Project(p) => {
                let mut items = Vec::new();
                for item in &p.items {
                    let slot = item.alias.0;
                    items.push((self.expr(item.expression)?, slot));
                    self.bound.insert(slot);
                    self.aliases
                        .insert(self.arenas.bindings.get(item.alias).name.clone(), slot);
                }
                self.ops.push(Op::Project {
                    items,
                    distinct: p.distinct,
                });
                Ok(())
            }
            Operation::Aggregate(a) => {
                let mut keys = Vec::new();
                for key in &a.grouping_keys {
                    let slot = key.alias.0;
                    keys.push((self.expr(key.expression)?, slot));
                }
                let mut aggregates = Vec::new();
                for item in &a.aggregates {
                    aggregates.push(self.aggregate(item)?);
                }
                // After an aggregation a row holds only the keys and the aggregates.
                self.bound.clear();
                for (_, slot) in &keys {
                    self.bound.insert(*slot);
                }
                for aggregate in &aggregates {
                    self.bound.insert(aggregate.slot);
                }
                for key in &a.grouping_keys {
                    self.aliases.insert(
                        self.arenas.bindings.get(key.alias).name.clone(),
                        key.alias.0,
                    );
                }
                for item in &a.aggregates {
                    self.aliases.insert(
                        self.arenas.bindings.get(item.alias).name.clone(),
                        item.alias.0,
                    );
                }
                self.ops.push(Op::Aggregate { keys, aggregates });
                Ok(())
            }
            Operation::Sort(s) => {
                let mut keys = Vec::new();
                for item in &s.items {
                    keys.push(SortKey {
                        expr: self.expr(item.expression)?,
                        descending: matches!(item.direction, SortDirection::Descending),
                    });
                }
                self.ops.push(Op::Sort(keys));
                Ok(())
            }
            Operation::Skip(s) => {
                let e = self.expr(s.count)?;
                self.ops.push(Op::Skip(e));
                Ok(())
            }
            Operation::Limit(l) => {
                let e = self.expr(l.count)?;
                self.ops.push(Op::Limit(e));
                Ok(())
            }
            Operation::Return(r) => {
                self.distinct_return = r.distinct;
                Ok(())
            }
            Operation::Union(_) => Err(not_yet("UNION", "stage M4")),
            other => Err(QueryError::unsupported(format!(
                "the operation {} is not supported",
                format!("{other:?}")
                    .split(['(', ' ', '{'])
                    .next()
                    .unwrap_or("?")
            ))),
        }
    }

    fn fresh(&mut self) -> usize {
        self.next_slot += 1;
        self.next_slot - 1
    }

    /// The label (the kind) and the property constraints of a node pattern.
    fn node_constraints(&self, node: &NodePattern) -> Result<Constraints, QueryError> {
        if node.label_expression.is_some() {
            return Err(QueryError::unsupported(
                "a label expression in a pattern is undecided (ADR 0021, open)",
            ));
        }
        let label = match node.labels.as_slice() {
            [] => None,
            [one] => Some(self.arenas.labels.name_of(*one).unwrap_or("").to_string()),
            _ => {
                return Err(QueryError::unsupported(
                    "a node has one label, its kind; a pattern with several labels is undecided (ADR 0021, open)",
                ))
            }
        };
        Ok((label, self.props(node.properties)?))
    }

    /// The `{key: value}` constraints of a node or relationship pattern.
    fn props(&self, map: Option<ExprId>) -> Result<Vec<(String, Expr)>, QueryError> {
        let mut props = Vec::new();
        if let Some(map) = map {
            match &self.arenas.expressions.get(map).kind {
                ExprKind::Map(entries) => {
                    for (key, value) in entries {
                        let name = self
                            .arenas
                            .property_keys
                            .name_of(*key)
                            .unwrap_or("")
                            .to_string();
                        props.push((name, self.expr(*value)?));
                    }
                }
                _ => {
                    return Err(QueryError::unsupported(
                        "a pattern's properties must be a literal map",
                    ))
                }
            }
        }
        Ok(props)
    }

    /// One graph pattern (all the comma-separated parts of one `MATCH`): a scan or a check for a node that no
    /// relationship reaches, and for every relationship an expansion from an end that is bound, in the order written. A
    /// relationship is used once within the pattern, so each expansion names the relationships before it.
    fn pattern(&mut self, pattern: &GraphPattern) -> Result<(), QueryError> {
        let node_slots: Vec<usize> = pattern
            .nodes
            .iter()
            .map(|n| match n.binding {
                Some(b) => b.0,
                None => self.fresh(),
            })
            .collect();
        let was_bound: Vec<bool> = node_slots.iter().map(|s| self.bound.contains(s)).collect();
        let mut touched = vec![false; pattern.nodes.len()];
        let mut rel_slots: Vec<usize> = Vec::new();

        for rel in &pattern.relationships {
            if rel.type_expression.is_some() {
                return Err(QueryError::unsupported(
                    "a relationship type expression is undecided (ADR 0021, open)",
                ));
            }
            let hops = match rel.length {
                RelationshipLength::Single => None,
                // decypher gives `None` for a bound that was not written: `*` and `*..3` start at 1 (checked).
                RelationshipLength::Variable { min, max } => Some((min.unwrap_or(1), max)),
            };
            let (l, r) = (rel.left, rel.right);
            // Expand from the end that is bound; if neither is, scan the left one first.
            let from_left = if self.bound.contains(&node_slots[l]) {
                true
            } else if self.bound.contains(&node_slots[r]) {
                false
            } else {
                self.start(&pattern.nodes[l], node_slots[l])?;
                touched[l] = true;
                true
            };
            let (from, to) = if from_left { (l, r) } else { (r, l) };
            if !touched[from] {
                if was_bound[from] {
                    self.check(&pattern.nodes[from], node_slots[from])?;
                }
                touched[from] = true;
            }
            let direction = match (rel.direction, from_left) {
                (RelationshipDirection::LeftToRight, true)
                | (RelationshipDirection::RightToLeft, false) => Direction::Out,
                (RelationshipDirection::RightToLeft, true)
                | (RelationshipDirection::LeftToRight, false) => Direction::In,
                _ => Direction::Both,
            };
            let types = rel
                .types
                .iter()
                .map(|t| {
                    self.arenas
                        .relationship_types
                        .name_of(*t)
                        .unwrap_or("")
                        .to_string()
                })
                .collect();
            let (to_label, to_props) = self.node_constraints(&pattern.nodes[to])?;
            let rel_slot = match rel.binding {
                Some(b) => b.0,
                None => self.fresh(),
            };
            let to_bound = self.bound.contains(&node_slots[to]);
            self.ops.push(Op::Expand(Expand {
                from: node_slots[from],
                rel: rel_slot,
                to: node_slots[to],
                direction,
                types,
                props: self.props(rel.properties)?,
                to_bound,
                to_label,
                to_props,
                distinct_from: rel_slots.clone(),
                hops,
            }));
            touched[to] = true;
            self.bound.insert(node_slots[to]);
            self.bound.insert(rel_slot);
            rel_slots.push(rel_slot);
        }

        for (i, node) in pattern.nodes.iter().enumerate() {
            if touched[i] {
                continue;
            }
            if self.bound.contains(&node_slots[i]) {
                self.check(node, node_slots[i])?;
            } else {
                self.start(node, node_slots[i])?;
            }
        }
        Ok(())
    }

    fn start(&mut self, node: &NodePattern, slot: usize) -> Result<(), QueryError> {
        let (label, props) = self.node_constraints(node)?;
        self.bound.insert(slot);
        self.ops.push(Op::ScanNodes { slot, label, props });
        Ok(())
    }

    fn check(&mut self, node: &NodePattern, slot: usize) -> Result<(), QueryError> {
        let (label, props) = self.node_constraints(node)?;
        if label.is_some() || !props.is_empty() {
            self.ops.push(Op::CheckNode { slot, label, props });
        }
        Ok(())
    }

    fn aggregate(&mut self, item: &AggregateItem) -> Result<Aggregate, QueryError> {
        let name = self
            .arenas
            .functions
            .name_of(item.function)
            .unwrap_or("")
            .to_ascii_lowercase();
        let func = match name.as_str() {
            "count" => AggFunc::Count,
            "sum" => AggFunc::Sum,
            "min" => AggFunc::Min,
            "max" => AggFunc::Max,
            "avg" => AggFunc::Avg,
            "collect" => AggFunc::Collect,
            other => {
                return Err(QueryError::unsupported(format!(
                    "the aggregate `{other}` is not supported"
                )))
            }
        };
        let arg = match item.args.as_slice() {
            [] if func == AggFunc::Count => None,
            [one] => Some(self.expr(*one)?),
            _ => {
                return Err(QueryError::unsupported(format!(
                    "`{name}` takes one argument (or `*` for count)"
                )))
            }
        };
        Ok(Aggregate {
            func,
            arg,
            distinct: item.distinct,
            slot: item.alias.0,
        })
    }

    fn expr(&self, id: ExprId) -> Result<Expr, QueryError> {
        let kind = &self.arenas.expressions.get(id).kind;
        Ok(match kind {
            ExprKind::Literal(l) => Expr::Literal(match l {
                Literal::Null => Value::Null,
                Literal::Boolean(b) => Value::Bool(*b),
                Literal::Integer(i) => Value::Int(*i),
                Literal::Float(f) => Value::Float(*f),
                Literal::String(s) => Value::String(s.clone()),
            }),
            ExprKind::Binding(b) => Expr::Slot(b.0),
            ExprKind::Property { base, key } => Expr::Property(
                Box::new(self.expr(*base)?),
                self.arenas
                    .property_keys
                    .name_of(*key)
                    .unwrap_or("")
                    .to_string(),
            ),
            ExprKind::Parameter(p) => {
                Expr::Param(self.arenas.parameters.name_of(*p).unwrap_or("").to_string())
            }
            ExprKind::List(items) => Expr::List(
                items
                    .iter()
                    .map(|i| self.expr(*i))
                    .collect::<Result<_, _>>()?,
            ),
            ExprKind::Unary { op, expr } => match op {
                UnaryOp::Negate => Expr::Neg(Box::new(self.expr(*expr)?)),
                UnaryOp::Plus => self.expr(*expr)?,
                UnaryOp::Not => Expr::Not(Box::new(self.expr(*expr)?)),
            },
            ExprKind::Binary { op, left, right } => self.binary(*op, *left, *right)?,
            ExprKind::Comparison { left, operators } => {
                if let [(
                    op @ (ComparisonOperator::StartsWith
                    | ComparisonOperator::EndsWith
                    | ComparisonOperator::Contains),
                    right,
                )] = operators.as_slice()
                {
                    let bin = match op {
                        ComparisonOperator::StartsWith => BinOp::StartsWith,
                        ComparisonOperator::EndsWith => BinOp::EndsWith,
                        _ => BinOp::Contains,
                    };
                    return Ok(Expr::Binary(
                        bin,
                        Box::new(self.expr(*left)?),
                        Box::new(self.expr(*right)?),
                    ));
                }
                let mut chain = Vec::new();
                for (op, right) in operators {
                    chain.push((cmp(*op)?, self.expr(*right)?));
                }
                Expr::Compare(Box::new(self.expr(*left)?), chain)
            }
            ExprKind::IsNull { operand, negated } => {
                Expr::IsNull(Box::new(self.expr(*operand)?), *negated)
            }
            ExprKind::In { lhs, rhs } => {
                Expr::In(Box::new(self.expr(*lhs)?), Box::new(self.expr(*rhs)?))
            }
            ExprKind::Case(c) => Expr::Case {
                scrutinee: match c.scrutinee {
                    Some(s) => Some(Box::new(self.expr(s)?)),
                    None => None,
                },
                alternatives: c
                    .alternatives
                    .iter()
                    .map(|a| Ok((self.expr(a.when)?, self.expr(a.then)?)))
                    .collect::<Result<_, QueryError>>()?,
                default: match c.default {
                    Some(d) => Some(Box::new(self.expr(d)?)),
                    None => None,
                },
            },
            ExprKind::LabelCheck { base, expression } => match expression {
                LabelExpr::Name(l) => Expr::HasLabel(
                    Box::new(self.expr(*base)?),
                    self.arenas.labels.name_of(*l).unwrap_or("").to_string(),
                ),
                _ => {
                    return Err(QueryError::unsupported(
                        "a label expression is undecided (ADR 0021, open)",
                    ))
                }
            },
            ExprKind::NodeLabels { base, labels } => {
                let mut all: Option<Expr> = None;
                for l in labels {
                    let has = Expr::HasLabel(
                        Box::new(self.expr(*base)?),
                        self.arenas.labels.name_of(*l).unwrap_or("").to_string(),
                    );
                    all = Some(match all {
                        None => has,
                        Some(prev) => Expr::Binary(BinOp::And, Box::new(prev), Box::new(has)),
                    });
                }
                all.unwrap_or(Expr::Literal(Value::Bool(true)))
            }
            other => {
                return Err(QueryError::unsupported(format!(
                    "the expression {} is not built yet",
                    format!("{other:?}")
                        .split(['(', ' ', '{'])
                        .next()
                        .unwrap_or("?")
                )))
            }
        })
    }

    fn binary(&self, op: BinaryOp, left: ExprId, right: ExprId) -> Result<Expr, QueryError> {
        let (l, r) = (Box::new(self.expr(left)?), Box::new(self.expr(right)?));
        let compare = |op| Expr::Compare(l.clone(), vec![(op, *r.clone())]);
        Ok(match op {
            BinaryOp::Add => Expr::Binary(BinOp::Add, l, r),
            BinaryOp::Subtract => Expr::Binary(BinOp::Sub, l, r),
            BinaryOp::Multiply => Expr::Binary(BinOp::Mul, l, r),
            BinaryOp::Divide => Expr::Binary(BinOp::Div, l, r),
            BinaryOp::Modulo => Expr::Binary(BinOp::Mod, l, r),
            BinaryOp::And => Expr::Binary(BinOp::And, l, r),
            BinaryOp::Or => Expr::Binary(BinOp::Or, l, r),
            BinaryOp::Xor => Expr::Binary(BinOp::Xor, l, r),
            BinaryOp::StartsWith => Expr::Binary(BinOp::StartsWith, l, r),
            BinaryOp::EndsWith => Expr::Binary(BinOp::EndsWith, l, r),
            BinaryOp::Contains => Expr::Binary(BinOp::Contains, l, r),
            BinaryOp::In => Expr::In(l, r),
            BinaryOp::Eq => compare(CmpOp::Eq),
            BinaryOp::Ne => compare(CmpOp::Ne),
            BinaryOp::Lt => compare(CmpOp::Lt),
            BinaryOp::Gt => compare(CmpOp::Gt),
            BinaryOp::Le => compare(CmpOp::Le),
            BinaryOp::Ge => compare(CmpOp::Ge),
            BinaryOp::Power => return Err(QueryError::unsupported(
                "the power operator ^ needs PortableMath and is not built yet (ADR 0021 item 6)",
            )),
            other => {
                return Err(QueryError::unsupported(format!(
                    "the operator {other:?} is not supported"
                )))
            }
        })
    }

    /// The columns of the answer, in the order the last `RETURN` writes them, matched to the aliases of the last part.
    fn columns(&self, tree: &SyntaxNode) -> Result<Vec<Column>, QueryError> {
        let names = return_names(tree)?;
        names
            .into_iter()
            .map(|name| match self.aliases.get(&name) {
                Some(slot) => Ok(Column { name, slot: *slot }),
                None => Err(QueryError::unsupported(format!(
                    "cannot place the column `{name}`: the HIR names no alias for it"
                ))),
            })
            .collect()
    }
}

fn cmp(op: ComparisonOperator) -> Result<CmpOp, QueryError> {
    Ok(match op {
        ComparisonOperator::Eq => CmpOp::Eq,
        ComparisonOperator::Ne => CmpOp::Ne,
        ComparisonOperator::Lt => CmpOp::Lt,
        ComparisonOperator::Gt => CmpOp::Gt,
        ComparisonOperator::Le => CmpOp::Le,
        ComparisonOperator::Ge => CmpOp::Ge,
        other => {
            return Err(QueryError::unsupported(format!(
                "the operator {other:?} cannot be chained"
            )))
        }
    })
}

/// The names of the columns of the last `RETURN`, in the order written: the alias after `AS`, else the text of the item.
fn return_names(tree: &SyntaxNode) -> Result<Vec<String>, QueryError> {
    let clause = tree
        .descendants()
        .filter(|n| n.kind() == K::RETURN_CLAUSE)
        .last()
        .ok_or_else(|| QueryError::unsupported("a query without RETURN has no columns"))?;
    let items = clause
        .descendants()
        .find(|n| n.kind() == K::PROJECTION_ITEMS)
        .ok_or_else(|| QueryError::unsupported("the RETURN clause has no items"))?;
    if items.children_with_tokens().any(|c| c.kind() == K::STAR) {
        return Err(QueryError::unsupported("RETURN * is not supported yet"));
    }
    let mut names = Vec::new();
    for item in items.children().filter(|n| n.kind() == K::PROJECTION_ITEM) {
        let mut after_as = false;
        let mut alias = None;
        for child in item.children_with_tokens() {
            if child.kind() == K::KW_AS {
                after_as = true;
            } else if after_as && child.kind() == K::VARIABLE {
                alias = child.as_node().map(|n| n.text().to_string());
            }
        }
        names.push(unquote(&alias.unwrap_or_else(|| item.text().to_string())));
    }
    Ok(names)
}

/// A name without its backticks (`` `my col` `` is `my col`); other text as it is, trimmed.
fn unquote(text: &str) -> String {
    let text = text.trim();
    match text.strip_prefix('`').and_then(|t| t.strip_suffix('`')) {
        Some(inner) => inner.replace("``", "`"),
        None => text.to_string(),
    }
}
