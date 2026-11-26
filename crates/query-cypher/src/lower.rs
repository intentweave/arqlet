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
    AggFunc, Aggregate, BinOp, CmpOp, Column, Expand, Expr, Function, Op, Plan, SortKey,
};
use arqlet_query::Direction;
use arqlet_query::{QueryError, Value};
use decypher::hir::arena::ExprId;
use decypher::hir::arena::{BindingId, HirArenas};
use decypher::hir::expr::{BinaryOp, ComparisonOperator, ExprKind, Literal, UnaryOp};
use decypher::hir::ops::{AggregateItem, Operation, SortDirection, UnionOp};
use decypher::hir::pattern::{
    GraphPattern, LabelExpr, NodePattern, RelationshipDirection, RelationshipLength,
};
use decypher::hir::HirQuery;
use decypher::syntax::{SyntaxKind as K, SyntaxNode};
use std::collections::{BTreeMap, BTreeSet};

/// Checks a query (the parser and the guard), then lowers it into a plan.
pub fn lower(query: &str) -> Result<Plan, QueryError> {
    crate::check(query)?;
    let hir = decypher::analyze(query).map_err(|e| QueryError::syntax(e.to_string()))?;
    let tree = decypher::parse_cst(query).tree;
    // The `RETURN` clauses in the order written: one for each part of a `UNION`, the last one for any other query.
    let returns: Vec<SyntaxNode> = tree
        .descendants()
        .filter(|n| n.kind() == K::RETURN_CLAUSE)
        .collect();
    if let [part] = hir.parts.as_slice() {
        if let [Operation::Union(u)] = part.operations.as_slice() {
            return union(u, &returns);
        }
    }
    single(&hir, returns.last())
}

/// `UNION [ALL]`: every branch is a query of its own, with its own variables.
fn union(u: &UnionOp, returns: &[SyntaxNode]) -> Result<Plan, QueryError> {
    if u.branches.len() != returns.len() {
        return Err(QueryError::unsupported(
            "cannot match the parts of this UNION to their RETURN clauses",
        ));
    }
    let branches = u
        .branches
        .iter()
        .zip(returns)
        .map(|(query, clause)| single(query, Some(clause)))
        .collect::<Result<Vec<Plan>, _>>()?;
    let names = |p: &Plan| p.columns.iter().map(|c| c.name.clone()).collect::<Vec<_>>();
    let Some(first) = branches.first().map(names) else {
        return Err(QueryError::unsupported("a UNION without parts"));
    };
    // decypher refuses different names; the same names in another order it accepts, and rows are matched by position.
    if branches.iter().any(|b| names(b) != first) {
        return Err(QueryError::unsupported(
            "the parts of a UNION must return the same columns in the same order",
        ));
    }
    let columns: Vec<Column> = first
        .into_iter()
        .enumerate()
        .map(|(slot, name)| Column { name, slot })
        .collect();
    Ok(Plan {
        slots: columns.len(),
        ops: vec![Op::Union {
            branches,
            distinct: u.distinct,
        }],
        columns,
    })
}

/// A query without `UNION`, with the `RETURN` clause that names its columns.
fn single(hir: &HirQuery, returning: Option<&SyntaxNode>) -> Result<Plan, QueryError> {
    let slots = hir.arenas.bindings.iter().count();
    let mut lowering = Lowering {
        arenas: &hir.arenas,
        next_slot: slots,
        bound: BTreeSet::new(),
        ops: Vec::new(),
        aliases: BTreeMap::new(),
        grouped: Vec::new(),
        distinct_return: false,
    };
    for part in &hir.parts {
        lowering.aliases.clear();
        for op in in_clause_order(&part.operations) {
            lowering.op(op)?;
        }
    }
    let columns = lowering.columns(returning)?;
    if lowering.distinct_return {
        let slots = columns.iter().map(|c| c.slot).collect();
        lowering.ops.push(Op::Distinct(slots));
    }
    let plan = Plan {
        slots: lowering.next_slot,
        ops: lowering.ops,
        columns,
    };
    available(&plan, &hir.arenas)?;
    Ok(plan)
}

/// The operations of a part of the query in the order they apply. The HIR puts the `WHERE` of a `WITH` ahead of the
/// `ORDER BY`, `SKIP` and `LIMIT` of the same clause, but it filters what they leave.
fn in_clause_order(operations: &[Operation]) -> Vec<&Operation> {
    let cuts = |op: &Operation| {
        matches!(
            op,
            Operation::Sort(_) | Operation::Skip(_) | Operation::Limit(_)
        )
    };
    let mut ordered = Vec::with_capacity(operations.len());
    let mut filters = Vec::new();
    for (i, op) in operations.iter().enumerate() {
        if matches!(op, Operation::Filter(_)) && operations[i + 1..].iter().any(cuts) {
            filters.push(op);
            continue;
        }
        if !cuts(op) {
            ordered.append(&mut filters);
        }
        ordered.push(op);
    }
    ordered.append(&mut filters);
    ordered
}

/// Refuses a plan in which an expression reads a variable that holds nothing at that point: it would read null and answer
/// something the query does not say. After an aggregation only the grouping keys and the aggregates are there.
fn available(plan: &Plan, arenas: &HirArenas) -> Result<(), QueryError> {
    let mut there = BTreeSet::new();
    check_ops(&plan.ops, &mut there, arenas)?;
    for column in &plan.columns {
        read(column.slot, &there, arenas)?;
    }
    Ok(())
}

fn read(slot: usize, there: &BTreeSet<usize>, arenas: &HirArenas) -> Result<(), QueryError> {
    if there.contains(&slot) {
        return Ok(());
    }
    let name = arenas
        .bindings
        .iter()
        .find(|(id, _)| id.0 == slot)
        .map_or("?", |(_, b)| b.name.as_str());
    Err(QueryError::unsupported(format!(
        "the variable `{name}` has no value here: a pattern cannot read a variable it binds itself, and after an \
         aggregation only the grouping keys and the aggregates are there (name what is needed in a WITH first)"
    )))
}

fn check_expr(expr: &Expr, there: &BTreeSet<usize>, arenas: &HirArenas) -> Result<(), QueryError> {
    if let Expr::Slot(slot) = expr {
        return read(*slot, there, arenas);
    }
    // The variable of a comprehension is there inside its filter and its map, not in its list.
    if let Expr::Comprehension {
        slot,
        list,
        filter,
        map,
    } = expr
    {
        check_expr(list, there, arenas)?;
        let mut inside = there.clone();
        inside.insert(*slot);
        for e in [filter, map].into_iter().flatten() {
            check_expr(e, &inside, arenas)?;
        }
        return Ok(());
    }
    let mut result = Ok(());
    expr.clone().for_each_child_mut(&mut |child| {
        if result.is_ok() {
            result = check_expr(child, there, arenas);
        }
    });
    result
}

fn check_ops(
    ops: &[Op],
    there: &mut BTreeSet<usize>,
    arenas: &HirArenas,
) -> Result<(), QueryError> {
    for op in ops {
        match op {
            Op::ScanNodes { slot, props, .. } => {
                for (_, e) in props {
                    check_expr(e, there, arenas)?;
                }
                there.insert(*slot);
            }
            Op::CheckNode { slot, props, .. } => {
                read(*slot, there, arenas)?;
                for (_, e) in props {
                    check_expr(e, there, arenas)?;
                }
            }
            Op::Expand(x) => {
                read(x.from, there, arenas)?;
                if x.to_bound {
                    read(x.to, there, arenas)?;
                }
                if x.rel_bound {
                    read(x.rel, there, arenas)?;
                }
                for (_, e) in x.props.iter().chain(&x.to_props) {
                    check_expr(e, there, arenas)?;
                }
                there.insert(x.rel);
                there.insert(x.to);
            }
            Op::Optional(inner) => check_ops(inner, there, arenas)?,
            Op::Filter(e) => check_expr(e, there, arenas)?,
            Op::Unwind { expr, slot } => {
                check_expr(expr, there, arenas)?;
                there.insert(*slot);
            }
            Op::Project { items, .. } => {
                for (e, _) in items {
                    check_expr(e, there, arenas)?;
                }
                there.extend(items.iter().map(|(_, slot)| *slot));
            }
            Op::Aggregate { keys, aggregates } => {
                for (e, _) in keys {
                    check_expr(e, there, arenas)?;
                }
                for e in aggregates.iter().filter_map(|a| a.arg.as_ref()) {
                    check_expr(e, there, arenas)?;
                }
                *there = keys
                    .iter()
                    .map(|(_, slot)| *slot)
                    .chain(aggregates.iter().map(|a| a.slot))
                    .collect();
            }
            Op::Distinct(slots) => {
                for slot in slots {
                    read(*slot, there, arenas)?;
                }
            }
            Op::Sort(keys) => {
                for key in keys {
                    check_expr(&key.expr, there, arenas)?;
                }
            }
            Op::Skip(e) | Op::Limit(e) => check_expr(e, &BTreeSet::new(), arenas)?,
            // Each branch is a plan of its own, checked when it was lowered.
            Op::Union { .. } => {}
        }
    }
    Ok(())
}

/// Replaces every part of `expr` that is a grouping expression by the slot that holds its value after the aggregation.
/// The HIR does this in some places only (not inside a list or a function call).
fn regroup(mut expr: Expr, grouped: &[(Expr, usize)]) -> Expr {
    if let Some((_, slot)) = grouped.iter().find(|(key, _)| *key == expr) {
        return Expr::Slot(*slot);
    }
    expr.for_each_child_mut(&mut |child| {
        let taken = std::mem::replace(child, Expr::Literal(Value::Null));
        *child = regroup(taken, grouped);
    });
    expr
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
    /// The grouping expressions of the last aggregation, as they read before it, with the slot of each.
    grouped: Vec<(Expr, usize)>,
    /// `RETURN DISTINCT`: the HIR keeps the flag on the Return operation, not on the projection.
    distinct_return: bool,
}

/// The label and the `{key: value}` constraints of a node pattern.
type Constraints = (Option<String>, Vec<(String, Expr)>);

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
                let e = self.after(f.predicate)?;
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
                    items.push((self.after(item.expression)?, slot));
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
                self.grouped = keys.clone();
                self.ops.push(Op::Aggregate { keys, aggregates });
                Ok(())
            }
            Operation::Sort(s) => {
                let mut keys = Vec::new();
                for item in &s.items {
                    keys.push(SortKey {
                        expr: self.after(item.expression)?,
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
            Operation::Union(_) => Err(QueryError::unsupported(
                "a UNION inside a query is not supported",
            )),
            other => Err(QueryError::unsupported(format!(
                "the operation {} is not supported",
                format!("{other:?}")
                    .split(['(', ' ', '{'])
                    .next()
                    .unwrap_or("?")
            ))),
        }
    }

    /// An expression of a clause that may follow an aggregation.
    fn after(&self, id: ExprId) -> Result<Expr, QueryError> {
        Ok(regroup(self.expr(id)?, &self.grouped))
    }

    /// Refuses a comprehension variable that is named like a variable outside it. After a projection the HIR replaces
    /// text that reads like a projected item by that item's column, also inside a comprehension whose own variable the
    /// text means, and the query would answer from the column. Comprehensions may share a name among themselves.
    fn own_name(&self, variable: BindingId) -> Result<(), QueryError> {
        let name = &self.arenas.bindings.get(variable).name;
        let comprehensions: BTreeSet<usize> = self
            .arenas
            .expressions
            .iter()
            .filter_map(|(_, e)| match &e.kind {
                ExprKind::ListComprehension(c) => Some(c.variable.0),
                _ => None,
            })
            .collect();
        let taken = self
            .arenas
            .bindings
            .iter()
            .any(|(id, b)| b.name == *name && !comprehensions.contains(&id.0));
        if taken {
            return Err(QueryError::unsupported(format!(
                "the comprehension variable `{name}` has the name of a variable outside it, which the parser can \
                 mistake for it; give it another name"
            )));
        }
        Ok(())
    }

    fn optional(&self, id: Option<ExprId>) -> Result<Option<Box<Expr>>, QueryError> {
        id.map(|id| self.expr(id).map(Box::new)).transpose()
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
        let mut touched = vec![false; pattern.nodes.len()];
        let mut rel_slots: Vec<usize> = Vec::new();
        // A node no relationship reaches is taken where it is written, before the relationships written after it, so
        // that their properties can read it.
        let mut alone = vec![true; pattern.nodes.len()];
        for rel in &pattern.relationships {
            alone[rel.left] = false;
            alone[rel.right] = false;
        }

        for rel in &pattern.relationships {
            for i in 0..rel.left.min(rel.right) {
                if alone[i] && !touched[i] {
                    self.node(&pattern.nodes[i], node_slots[i])?;
                    touched[i] = true;
                }
            }
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
            // A start that was bound already, by an earlier clause or by an earlier part of this pattern, keeps the
            // label and the properties written here.
            if !touched[from] {
                self.check(&pattern.nodes[from], node_slots[from])?;
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
            let rel_bound = self.bound.contains(&rel_slot);
            self.ops.push(Op::Expand(Expand {
                from: node_slots[from],
                rel: rel_slot,
                to: node_slots[to],
                direction,
                types,
                props: self.props(rel.properties)?,
                to_bound,
                rel_bound,
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
            if !touched[i] {
                self.node(node, node_slots[i])?;
            }
        }
        Ok(())
    }

    /// A node of a pattern on its own: checked when its variable is bound, else scanned.
    fn node(&mut self, node: &NodePattern, slot: usize) -> Result<(), QueryError> {
        if self.bound.contains(&slot) {
            self.check(node, slot)
        } else {
            self.start(node, slot)
        }
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
            ExprKind::FunctionCall {
                function,
                args,
                distinct,
            } => self.call(
                self.arenas.functions.name_of(*function).unwrap_or(""),
                args,
                *distinct,
            )?,
            ExprKind::Map(entries) => {
                let mut out: Vec<(String, Expr)> = Vec::new();
                for (key, value) in entries {
                    let key = self.arenas.property_keys.name_of(*key).unwrap_or("");
                    if out.iter().any(|(k, _)| k == key) {
                        return Err(QueryError::unsupported(format!(
                            "a map with the key `{key}` twice is not decided (ADR 0021)"
                        )));
                    }
                    out.push((key.to_string(), self.expr(*value)?));
                }
                Expr::Map(out)
            }
            ExprKind::ListIndex { list, index } => {
                Expr::Index(Box::new(self.expr(*list)?), Box::new(self.expr(*index)?))
            }
            ExprKind::ListSlice { list, start, end } => Expr::Slice(
                Box::new(self.expr(*list)?),
                self.optional(*start)?,
                self.optional(*end)?,
            ),
            ExprKind::ListComprehension(c) => {
                self.own_name(c.variable)?;
                Expr::Comprehension {
                    slot: c.variable.0,
                    list: Box::new(self.expr(c.collection)?),
                    filter: self.optional(c.filter)?,
                    map: self.optional(c.map)?,
                }
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

    /// A scalar function call. Aggregates are not expressions here: the HIR takes them out of a projection, so one that
    /// is left stands where none may stand.
    fn call(&self, name: &str, args: &[ExprId], distinct: bool) -> Result<Expr, QueryError> {
        let lower = name.to_ascii_lowercase();
        if matches!(
            lower.as_str(),
            "avg" | "collect" | "count" | "max" | "min" | "sum"
        ) {
            return Err(QueryError::syntax(format!(
                "the aggregate `{name}` is only allowed in RETURN and WITH"
            )));
        }
        let function = scalar(&lower).ok_or_else(|| {
            QueryError::unsupported(format!(
                "the function `{name}` is not in the function table"
            ))
        })?;
        if distinct {
            return Err(QueryError::syntax(format!(
                "DISTINCT is for aggregates, not for `{name}`"
            )));
        }
        let (fewest, most) = function.arity();
        if args.len() < fewest || args.len() > most {
            return Err(QueryError::syntax(format!(
                "`{}` does not take {} arguments",
                function.name(),
                args.len()
            )));
        }
        Ok(Expr::Call(
            function,
            args.iter()
                .map(|a| self.expr(*a))
                .collect::<Result<_, _>>()?,
        ))
    }

    /// The columns of the answer, in the order the `RETURN` writes them, matched to the aliases of the last part.
    fn columns(&self, returning: Option<&SyntaxNode>) -> Result<Vec<Column>, QueryError> {
        let names = return_names(returning)?;
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

/// The scalar function of a name in lower case.
fn scalar(name: &str) -> Option<Function> {
    Some(match name {
        "id" | "elementid" => Function::Id,
        "type" => Function::Type,
        "labels" => Function::Labels,
        "size" => Function::Size,
        "coalesce" => Function::Coalesce,
        "tostring" => Function::ToString,
        "toupper" => Function::ToUpper,
        "tolower" => Function::ToLower,
        "trim" => Function::Trim,
        "ltrim" => Function::LTrim,
        "rtrim" => Function::RTrim,
        "replace" => Function::Replace,
        "substring" => Function::Substring,
        "left" => Function::Left,
        "right" => Function::Right,
        "split" => Function::Split,
        "reverse" => Function::Reverse,
        _ => return None,
    })
}

/// The names of the columns of a `RETURN`, in the order written: the alias after `AS`, else the text of the item.
fn return_names(returning: Option<&SyntaxNode>) -> Result<Vec<String>, QueryError> {
    let clause = returning
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_function_of_the_table_is_an_aggregate_or_has_a_lowering() {
        for name in crate::FUNCTIONS {
            let aggregate = matches!(*name, "avg" | "collect" | "count" | "max" | "min" | "sum");
            assert_ne!(aggregate, scalar(name).is_some(), "{name}");
        }
        for name in ["id", "elementid", "type", "labels", "size", "coalesce"] {
            assert!(crate::FUNCTIONS.contains(&name), "{name}");
        }
    }
}
