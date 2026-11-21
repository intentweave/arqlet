//! The executor (provisional, ADR 0021): runs a [`Plan`] over a [`GraphStore`], one operation after the other, on rows.
//!
//! It is synchronous, pure and deterministic. It never iterates a hash map to produce output: groups come out in the
//! order they were first seen, ties in a sort keep the order of the input, and nodes are visited in the store's order.
//! Hash maps are used only to find a group; their order is never read.
//!
//! Semantics fixed here (ADR 0021 item 6): three-valued logic; `null` ordered last when ascending and first when
//! descending; integer overflow and integer division by zero are errors (`Runtime`); `sum` and `avg` add in input
//! order; `collect` keeps input order; a result that would be `NaN` is an error until its rules are decided in the
//! fixtures. Mixed-type ordering and the comparison of an integer with a float beyond 2^53 are provisional.

use crate::plan::{AggFunc, Aggregate, BinOp, CmpOp, Column, Expr, Op, Plan, SortKey};
use crate::store::GraphStore;
use crate::value::{NodeId, Value};
use crate::QueryError;
use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap, HashSet};

/// The limits of one execution (ADR 0021 item 11). The defaults are provisional: they are set by measurement in M2.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// The work budget in deterministic steps: rows considered or produced by any operation. Never milliseconds, so that
    /// an answer does not depend on the machine.
    pub max_steps: u64,
    /// The most rows any operation may hold.
    pub max_rows: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_steps: 20_000_000,
            max_rows: 2_000_000,
        }
    }
}

/// The answer of a query: its columns, and one vector of values per row, in the order of the columns.
#[derive(Clone, Debug, PartialEq)]
pub struct ResultSet {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
}

type Row = Vec<Value>;

/// Runs `plan` over `store`. `params` are the values of the `$name` parameters.
pub fn execute(
    plan: &Plan,
    store: &dyn GraphStore,
    params: &BTreeMap<String, Value>,
    limits: Limits,
) -> Result<ResultSet, QueryError> {
    let mut run = Run {
        store,
        params,
        limits,
        steps: 0,
        slots: plan.slots,
    };
    run.run(plan)
}

struct Run<'a> {
    store: &'a dyn GraphStore,
    params: &'a BTreeMap<String, Value>,
    limits: Limits,
    steps: u64,
    slots: usize,
}

impl Run<'_> {
    fn run(&mut self, plan: &Plan) -> Result<ResultSet, QueryError> {
        let mut rows: Vec<Row> = vec![vec![Value::Null; self.slots]];
        for op in &plan.ops {
            rows = self.apply(op, rows)?;
            if rows.len() > self.limits.max_rows {
                return Err(QueryError::limit(format!(
                    "stopped: more than {} rows (the row limit) after {} steps",
                    self.limits.max_rows, self.steps
                )));
            }
        }
        Ok(ResultSet {
            columns: plan.columns.iter().map(|c| c.name.clone()).collect(),
            rows: rows
                .iter()
                .map(|row| {
                    plan.columns
                        .iter()
                        .map(|Column { slot, .. }| row[*slot].clone())
                        .collect()
                })
                .collect(),
        })
    }

    fn tick(&mut self, n: usize) -> Result<(), QueryError> {
        self.steps = self.steps.saturating_add(n as u64);
        if self.steps > self.limits.max_steps {
            return Err(QueryError::limit(format!(
                "stopped after {} steps (the work budget is {})",
                self.steps, self.limits.max_steps
            )));
        }
        Ok(())
    }

    fn apply(&mut self, op: &Op, rows: Vec<Row>) -> Result<Vec<Row>, QueryError> {
        match op {
            Op::ScanNodes { slot, label, props } => {
                let mut out = Vec::new();
                for row in &rows {
                    for i in 0..self.store.node_count() {
                        self.tick(1)?;
                        let node = NodeId(i as u32);
                        if self.node_matches(node, label, props, row)? {
                            let mut next = row.clone();
                            next[*slot] = Value::Node(node);
                            out.push(next);
                        }
                    }
                }
                Ok(out)
            }
            Op::CheckNode { slot, label, props } => {
                let mut out = Vec::new();
                for row in rows {
                    self.tick(1)?;
                    let keep = match &row[*slot] {
                        Value::Node(node) => self.node_matches(*node, label, props, &row)?,
                        _ => false,
                    };
                    if keep {
                        out.push(row);
                    }
                }
                Ok(out)
            }
            Op::Filter(expr) => {
                let mut out = Vec::new();
                for row in rows {
                    self.tick(1)?;
                    if truth(&self.eval(expr, &row)?, "WHERE")? == Some(true) {
                        out.push(row);
                    }
                }
                Ok(out)
            }
            Op::Unwind { expr, slot } => {
                let mut out = Vec::new();
                for row in &rows {
                    match self.eval(expr, row)? {
                        Value::Null => {}
                        Value::List(items) => {
                            for item in items {
                                self.tick(1)?;
                                let mut next = row.clone();
                                next[*slot] = item;
                                out.push(next);
                            }
                        }
                        other => {
                            return Err(QueryError::runtime(format!(
                                "UNWIND expects a list, not a {}",
                                other.kind()
                            )))
                        }
                    }
                }
                Ok(out)
            }
            Op::Project { items, distinct } => {
                let mut out = Vec::with_capacity(rows.len());
                let mut seen = HashSet::new();
                for row in &rows {
                    self.tick(1)?;
                    let mut next = row.clone();
                    for (expr, slot) in items {
                        next[*slot] = self.eval(expr, row)?;
                    }
                    if *distinct {
                        let key = group_key(items.iter().map(|(_, slot)| &next[*slot]));
                        if !seen.insert(key) {
                            continue;
                        }
                    }
                    out.push(next);
                }
                Ok(out)
            }
            Op::Aggregate { keys, aggregates } => self.aggregate(keys, aggregates, rows),
            Op::Sort(sort_keys) => self.sort(sort_keys, rows),
            Op::Skip(expr) => {
                let n = self.count(expr, "SKIP")?;
                Ok(rows.into_iter().skip(n).collect())
            }
            Op::Limit(expr) => {
                let n = self.count(expr, "LIMIT")?;
                Ok(rows.into_iter().take(n).collect())
            }
        }
    }

    /// The non-negative integer of a `SKIP` or `LIMIT`.
    fn count(&mut self, expr: &Expr, what: &str) -> Result<usize, QueryError> {
        let empty = vec![Value::Null; self.slots];
        match self.eval(expr, &empty)? {
            Value::Int(n) if n >= 0 => Ok(usize::try_from(n).unwrap_or(usize::MAX)),
            Value::Int(_) => Err(QueryError::runtime(format!(
                "{what} takes a non-negative integer"
            ))),
            other => Err(QueryError::runtime(format!(
                "{what} takes an integer, not a {}",
                other.kind()
            ))),
        }
    }

    fn node_matches(
        &self,
        node: NodeId,
        label: &Option<String>,
        props: &[(String, Expr)],
        row: &Row,
    ) -> Result<bool, QueryError> {
        if let Some(label) = label {
            if self.store.node_label(node) != label {
                return Ok(false);
            }
        }
        for (key, expr) in props {
            let wanted = self.eval(expr, row)?;
            if equals(&self.store.node_property(node, key), &wanted) != Some(true) {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn sort(&mut self, sort_keys: &[SortKey], rows: Vec<Row>) -> Result<Vec<Row>, QueryError> {
        let mut keyed = Vec::with_capacity(rows.len());
        for row in rows {
            self.tick(1)?;
            let mut values = Vec::with_capacity(sort_keys.len());
            for key in sort_keys {
                values.push(self.eval(&key.expr, &row)?);
            }
            keyed.push((values, row));
        }
        // A stable sort: rows that compare equal keep the order they came in.
        keyed.sort_by(|(a, _), (b, _)| {
            for (i, key) in sort_keys.iter().enumerate() {
                let c = null_last(&a[i], &b[i]);
                let c = if key.descending { c.reverse() } else { c };
                if c != Ordering::Equal {
                    return c;
                }
            }
            Ordering::Equal
        });
        Ok(keyed.into_iter().map(|(_, row)| row).collect())
    }

    fn aggregate(
        &mut self,
        keys: &[(Expr, usize)],
        aggregates: &[Aggregate],
        rows: Vec<Row>,
    ) -> Result<Vec<Row>, QueryError> {
        struct Group {
            keys: Vec<Value>,
            accs: Vec<Acc>,
        }
        let mut groups: Vec<Group> = Vec::new();
        let mut index: HashMap<String, usize> = HashMap::new(); // only to find a group; never iterated
        let new_accs = || {
            aggregates
                .iter()
                .map(|a| Acc::new(a.func, a.distinct))
                .collect::<Vec<_>>()
        };
        for row in &rows {
            self.tick(1)?;
            let mut key_values = Vec::with_capacity(keys.len());
            for (expr, _) in keys {
                key_values.push(self.eval(expr, row)?);
            }
            let id = group_key(key_values.iter());
            let at = match index.get(&id) {
                Some(&at) => at,
                None => {
                    groups.push(Group {
                        keys: key_values,
                        accs: new_accs(),
                    });
                    index.insert(id, groups.len() - 1);
                    groups.len() - 1
                }
            };
            for (acc, agg) in groups[at].accs.iter_mut().zip(aggregates) {
                let value = match &agg.arg {
                    Some(expr) => Some(self.eval(expr, row)?),
                    None => None,
                };
                acc.add(value)?;
            }
        }
        // Without keys an empty input still gives one row: `count(*)` over nothing is 0.
        if keys.is_empty() && groups.is_empty() {
            groups.push(Group {
                keys: Vec::new(),
                accs: new_accs(),
            });
        }
        let mut out = Vec::with_capacity(groups.len());
        for group in groups {
            let mut row = vec![Value::Null; self.slots];
            for ((_, slot), value) in keys.iter().zip(group.keys) {
                row[*slot] = value;
            }
            for (agg, acc) in aggregates.iter().zip(group.accs) {
                row[agg.slot] = acc.finish();
            }
            out.push(row);
        }
        Ok(out)
    }

    fn eval(&self, expr: &Expr, row: &Row) -> Result<Value, QueryError> {
        Ok(match expr {
            Expr::Literal(v) => v.clone(),
            Expr::Param(name) => self.params.get(name).cloned().ok_or_else(|| {
                QueryError::runtime(format!("the parameter ${name} was not supplied"))
            })?,
            Expr::Slot(slot) => row[*slot].clone(),
            Expr::Property(base, key) => match self.eval(base, row)? {
                Value::Null => Value::Null,
                Value::Node(node) => self.store.node_property(node, key),
                other => {
                    return Err(QueryError::runtime(format!(
                        "cannot read the property `{key}` of a {}",
                        other.kind()
                    )))
                }
            },
            Expr::List(items) => Value::List(
                items
                    .iter()
                    .map(|e| self.eval(e, row))
                    .collect::<Result<_, _>>()?,
            ),
            Expr::Not(x) => match truth(&self.eval(x, row)?, "NOT")? {
                None => Value::Null,
                Some(b) => Value::Bool(!b),
            },
            Expr::Neg(x) => match self.eval(x, row)? {
                Value::Null => Value::Null,
                Value::Int(i) => Value::Int(
                    i.checked_neg()
                        .ok_or_else(|| QueryError::runtime("integer overflow"))?,
                ),
                Value::Float(f) => float(-f)?,
                other => {
                    return Err(QueryError::runtime(format!(
                        "cannot negate a {}",
                        other.kind()
                    )))
                }
            },
            Expr::Binary(op, a, b) => self.binary(*op, a, b, row)?,
            Expr::Compare(left, chain) => {
                let mut left = self.eval(left, row)?;
                let mut result = Some(true);
                for (op, right) in chain {
                    let right = self.eval(right, row)?;
                    result = and(result, compare_op(*op, &left, &right));
                    left = right;
                }
                bool_or_null(result)
            }
            Expr::IsNull(x, negated) => Value::Bool(self.eval(x, row)?.is_null() != *negated),
            Expr::In(x, list) => {
                let needle = self.eval(x, row)?;
                match self.eval(list, row)? {
                    Value::Null => Value::Null,
                    Value::List(items) => {
                        let mut result = Some(false);
                        for item in &items {
                            match equals(&needle, item) {
                                Some(true) => {
                                    result = Some(true);
                                    break;
                                }
                                None => result = None,
                                Some(false) => {}
                            }
                        }
                        bool_or_null(result)
                    }
                    other => {
                        return Err(QueryError::runtime(format!(
                            "IN expects a list, not a {}",
                            other.kind()
                        )))
                    }
                }
            }
            Expr::HasLabel(x, label) => match self.eval(x, row)? {
                Value::Null => Value::Null,
                Value::Node(node) => Value::Bool(self.store.node_label(node) == label),
                other => {
                    return Err(QueryError::runtime(format!(
                        "a {} has no label",
                        other.kind()
                    )))
                }
            },
            Expr::Case {
                scrutinee,
                alternatives,
                default,
            } => {
                let scrutinee = match scrutinee {
                    Some(s) => Some(self.eval(s, row)?),
                    None => None,
                };
                for (when, then) in alternatives {
                    let when = self.eval(when, row)?;
                    let hit = match &scrutinee {
                        Some(s) => equals(s, &when) == Some(true),
                        None => truth(&when, "CASE WHEN")? == Some(true),
                    };
                    if hit {
                        return self.eval(then, row);
                    }
                }
                match default {
                    Some(d) => self.eval(d, row)?,
                    None => Value::Null,
                }
            }
        })
    }

    fn binary(&self, op: BinOp, a: &Expr, b: &Expr, row: &Row) -> Result<Value, QueryError> {
        match op {
            BinOp::And | BinOp::Or | BinOp::Xor => {
                let left = truth(&self.eval(a, row)?, "a logical operator")?;
                // The right side is not evaluated when the left decides: `false AND x` and `true OR x`.
                match (op, left) {
                    (BinOp::And, Some(false)) => return Ok(Value::Bool(false)),
                    (BinOp::Or, Some(true)) => return Ok(Value::Bool(true)),
                    _ => {}
                }
                let right = truth(&self.eval(b, row)?, "a logical operator")?;
                Ok(bool_or_null(match op {
                    BinOp::And => and(left, right),
                    BinOp::Or => or(left, right),
                    _ => match (left, right) {
                        (Some(l), Some(r)) => Some(l != r),
                        _ => None,
                    },
                }))
            }
            BinOp::StartsWith | BinOp::EndsWith | BinOp::Contains => {
                let (l, r) = (self.eval(a, row)?, self.eval(b, row)?);
                Ok(match (&l, &r) {
                    (Value::String(l), Value::String(r)) => Value::Bool(match op {
                        BinOp::StartsWith => l.starts_with(r.as_str()),
                        BinOp::EndsWith => l.ends_with(r.as_str()),
                        _ => l.contains(r.as_str()),
                    }),
                    // A string operator on anything else, or on null, is null.
                    _ => Value::Null,
                })
            }
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod => {
                let (l, r) = (self.eval(a, row)?, self.eval(b, row)?);
                arithmetic(op, l, r)
            }
        }
    }
}

// ---- three-valued logic -------------------------------------------------------------------------------------------

fn truth(v: &Value, what: &str) -> Result<Option<bool>, QueryError> {
    match v {
        Value::Null => Ok(None),
        Value::Bool(b) => Ok(Some(*b)),
        other => Err(QueryError::runtime(format!(
            "{what} expects a boolean, not a {}",
            other.kind()
        ))),
    }
}

fn bool_or_null(v: Option<bool>) -> Value {
    v.map_or(Value::Null, Value::Bool)
}

fn and(a: Option<bool>, b: Option<bool>) -> Option<bool> {
    match (a, b) {
        (Some(false), _) | (_, Some(false)) => Some(false),
        (Some(true), Some(true)) => Some(true),
        _ => None,
    }
}

fn or(a: Option<bool>, b: Option<bool>) -> Option<bool> {
    match (a, b) {
        (Some(true), _) | (_, Some(true)) => Some(true),
        (Some(false), Some(false)) => Some(false),
        _ => None,
    }
}

// ---- equality and ordering ----------------------------------------------------------------------------------------

fn as_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Int(i) => Some(*i as f64),
        Value::Float(f) => Some(*f),
        _ => None,
    }
}

/// The equality of Cypher: `None` (null) when either side is null, an integer equals the float of the same value, a
/// value of another kind is not equal.
fn equals(a: &Value, b: &Value) -> Option<bool> {
    match (a, b) {
        (Value::Null, _) | (_, Value::Null) => None,
        (Value::Int(x), Value::Int(y)) => Some(x == y),
        (Value::Int(_) | Value::Float(_), Value::Int(_) | Value::Float(_)) => {
            Some(as_f64(a) == as_f64(b))
        }
        (Value::Bool(x), Value::Bool(y)) => Some(x == y),
        (Value::String(x), Value::String(y)) => Some(x == y),
        (Value::Node(x), Value::Node(y)) => Some(x == y),
        (Value::Edge(x), Value::Edge(y)) => Some(x == y),
        (Value::List(x), Value::List(y)) => {
            if x.len() != y.len() {
                return Some(false);
            }
            let mut result = Some(true);
            for (p, q) in x.iter().zip(y) {
                match equals(p, q) {
                    Some(false) => return Some(false),
                    None => result = None,
                    Some(true) => {}
                }
            }
            result
        }
        _ => Some(false),
    }
}

/// The ordering of two values for `<`, `>`, `<=`, `>=`: defined for two numbers, two strings and two booleans; `None`
/// (null) for anything else.
fn order(a: &Value, b: &Value) -> Option<Ordering> {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => Some(x.cmp(y)),
        (Value::Int(_) | Value::Float(_), Value::Int(_) | Value::Float(_)) => {
            as_f64(a)?.partial_cmp(&as_f64(b)?)
        }
        (Value::String(x), Value::String(y)) => Some(x.as_str().cmp(y.as_str())),
        (Value::Bool(x), Value::Bool(y)) => Some(x.cmp(y)),
        _ => None,
    }
}

fn compare_op(op: CmpOp, a: &Value, b: &Value) -> Option<bool> {
    match op {
        CmpOp::Eq => equals(a, b),
        CmpOp::Ne => equals(a, b).map(|e| !e),
        _ => order(a, b).map(|o| match op {
            CmpOp::Lt => o == Ordering::Less,
            CmpOp::Gt => o == Ordering::Greater,
            CmpOp::Le => o != Ordering::Greater,
            _ => o != Ordering::Less,
        }),
    }
}

/// A total order for `ORDER BY`, `min` and `max`: by the kind of value first (a provisional rank), then within a kind.
fn total_order(a: &Value, b: &Value) -> Ordering {
    fn rank(v: &Value) -> u8 {
        match v {
            Value::List(_) => 1,
            Value::String(_) => 2,
            Value::Bool(_) => 3,
            Value::Int(_) | Value::Float(_) => 4,
            Value::Node(_) => 5,
            Value::Edge(_) => 6,
            Value::Null => 7,
        }
    }
    match rank(a).cmp(&rank(b)) {
        Ordering::Equal => {}
        other => return other,
    }
    match (a, b) {
        (Value::List(x), Value::List(y)) => {
            for (p, q) in x.iter().zip(y) {
                match null_last(p, q) {
                    Ordering::Equal => {}
                    other => return other,
                }
            }
            x.len().cmp(&y.len())
        }
        (Value::Node(x), Value::Node(y)) => x.cmp(y),
        (Value::Edge(x), Value::Edge(y)) => x.cmp(y),
        _ => order(a, b).unwrap_or(Ordering::Equal),
    }
}

/// `total_order` with null greater than everything: last when ascending, first when descending.
fn null_last(a: &Value, b: &Value) -> Ordering {
    match (a.is_null(), b.is_null()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => total_order(a, b),
    }
}

/// A string that is equal for values that group together: null with null, 1 with 1.0, and so on.
fn group_key<'a>(values: impl Iterator<Item = &'a Value>) -> String {
    fn push(v: &Value, out: &mut String) {
        match v {
            Value::Null => out.push_str("n;"),
            Value::Bool(b) => out.push_str(if *b { "t;" } else { "f;" }),
            Value::Int(i) => out.push_str(&format!("i{i};")),
            Value::Float(f) if f.fract() == 0.0 && f.abs() < 9.0e15 => {
                out.push_str(&format!("i{};", *f as i64))
            }
            Value::Float(f) => out.push_str(&format!("d{:016x};", f.to_bits())),
            Value::String(s) => out.push_str(&format!("s{}:{s};", s.len())),
            Value::List(items) => {
                out.push('[');
                for item in items {
                    push(item, out);
                }
                out.push_str("];");
            }
            Value::Node(n) => out.push_str(&format!("N{};", n.0)),
            Value::Edge(e) => out.push_str(&format!("E{};", e.0)),
        }
    }
    let mut out = String::new();
    for v in values {
        push(v, &mut out);
    }
    out
}

// ---- arithmetic ----------------------------------------------------------------------------------------------------

/// A float result; `NaN` is an error until its rules are decided in the fixtures (ADR 0021 item 6).
fn float(f: f64) -> Result<Value, QueryError> {
    if f.is_nan() {
        Err(QueryError::runtime(
            "the result is NaN; the rules for NaN are not decided yet (ADR 0021 item 6)",
        ))
    } else {
        Ok(Value::Float(f))
    }
}

fn overflow() -> QueryError {
    QueryError::runtime("integer overflow")
}

fn arithmetic(op: BinOp, l: Value, r: Value) -> Result<Value, QueryError> {
    if l.is_null() || r.is_null() {
        return Ok(Value::Null);
    }
    match (&l, &r) {
        (Value::Int(x), Value::Int(y)) => {
            let (x, y) = (*x, *y);
            let v = match op {
                BinOp::Add => x.checked_add(y),
                BinOp::Sub => x.checked_sub(y),
                BinOp::Mul => x.checked_mul(y),
                BinOp::Div | BinOp::Mod if y == 0 => {
                    return Err(QueryError::runtime("division by zero"))
                }
                BinOp::Div => x.checked_div(y),
                _ => x.checked_rem(y),
            };
            v.map(Value::Int).ok_or_else(overflow)
        }
        (Value::Int(_) | Value::Float(_), Value::Int(_) | Value::Float(_)) => {
            let (x, y) = (as_f64(&l).unwrap_or(0.0), as_f64(&r).unwrap_or(0.0));
            float(match op {
                BinOp::Add => x + y,
                BinOp::Sub => x - y,
                BinOp::Mul => x * y,
                BinOp::Div => x / y,
                _ => x % y,
            })
        }
        (Value::String(x), Value::String(y)) if op == BinOp::Add => {
            Ok(Value::String(format!("{x}{y}")))
        }
        _ => Err(QueryError::runtime(format!(
            "cannot apply the operator to a {} and a {}",
            l.kind(),
            r.kind()
        ))),
    }
}

// ---- aggregates -----------------------------------------------------------------------------------------------------

enum State {
    Count(i64),
    Sum {
        int: i64,
        float: f64,
        is_float: bool,
    },
    Extreme(Option<Value>),
    Avg {
        sum: f64,
        n: u64,
    },
    Collect(Vec<Value>),
}

struct Acc {
    func: AggFunc,
    state: State,
    seen: Option<HashSet<String>>, // for DISTINCT
}

impl Acc {
    fn new(func: AggFunc, distinct: bool) -> Acc {
        Acc {
            func,
            state: match func {
                AggFunc::Count => State::Count(0),
                AggFunc::Sum => State::Sum {
                    int: 0,
                    float: 0.0,
                    is_float: false,
                },
                AggFunc::Min | AggFunc::Max => State::Extreme(None),
                AggFunc::Avg => State::Avg { sum: 0.0, n: 0 },
                AggFunc::Collect => State::Collect(Vec::new()),
            },
            seen: distinct.then(HashSet::new),
        }
    }

    /// `None` is `count(*)`: a row without a value.
    fn add(&mut self, value: Option<Value>) -> Result<(), QueryError> {
        let Some(value) = value else {
            if let State::Count(n) = &mut self.state {
                *n += 1;
            }
            return Ok(());
        };
        if value.is_null() {
            return Ok(()); // an aggregate over a column ignores null
        }
        if let Some(seen) = &mut self.seen {
            if !seen.insert(group_key(std::iter::once(&value))) {
                return Ok(());
            }
        }
        match &mut self.state {
            State::Count(n) => *n += 1,
            State::Sum {
                int,
                float,
                is_float,
            } => match value {
                Value::Int(i) if !*is_float => *int = int.checked_add(i).ok_or_else(overflow)?,
                Value::Int(i) => *float += i as f64,
                Value::Float(f) => {
                    if !*is_float {
                        *is_float = true;
                        *float = *int as f64;
                    }
                    *float += f;
                }
                other => {
                    return Err(QueryError::runtime(format!(
                        "cannot sum a {}",
                        other.kind()
                    )))
                }
            },
            State::Avg { sum, n } => match as_f64(&value) {
                Some(f) => {
                    *sum += f;
                    *n += 1;
                }
                None => {
                    return Err(QueryError::runtime(format!(
                        "cannot average a {}",
                        value.kind()
                    )))
                }
            },
            State::Extreme(best) => {
                let better = match best {
                    None => true,
                    Some(b) => {
                        let o = total_order(&value, b);
                        if self.func == AggFunc::Min {
                            o == Ordering::Less
                        } else {
                            o == Ordering::Greater
                        }
                    }
                };
                if better {
                    *best = Some(value);
                }
            }
            State::Collect(items) => items.push(value),
        }
        Ok(())
    }

    fn finish(self) -> Value {
        match self.state {
            State::Count(n) => Value::Int(n),
            State::Sum {
                int,
                float,
                is_float,
            } => {
                if is_float {
                    Value::Float(float)
                } else {
                    Value::Int(int)
                }
            }
            State::Avg { sum, n } => {
                if n == 0 {
                    Value::Null
                } else {
                    Value::Float(sum / n as f64)
                }
            }
            State::Extreme(best) => best.unwrap_or(Value::Null),
            State::Collect(items) => Value::List(items),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::Aggregate;

    /// Three nodes: a (kind K, n = 1), b (kind K, n = 2), c (kind L, no n).
    struct Tiny;

    impl GraphStore for Tiny {
        fn node_count(&self) -> usize {
            3
        }
        fn node_label(&self, node: NodeId) -> &str {
            ["K", "K", "L"][node.0 as usize]
        }
        fn node_property(&self, node: NodeId, key: &str) -> Value {
            match (node.0, key) {
                (_, "id") => Value::String(["a", "b", "c"][node.0 as usize].to_string()),
                (0, "n") => Value::Int(1),
                (1, "n") => Value::Int(2),
                _ => Value::Null,
            }
        }
    }

    fn lit(v: Value) -> Expr {
        Expr::Literal(v)
    }

    fn run(plan: &Plan) -> Result<ResultSet, QueryError> {
        execute(plan, &Tiny, &BTreeMap::new(), Limits::default())
    }

    /// `UNWIND` of the literals, projected into slot 1 by `expr` over slot 0.
    fn over(items: Vec<Value>, expr: Expr) -> Plan {
        Plan {
            slots: 2,
            ops: vec![
                Op::Unwind {
                    expr: lit(Value::List(items)),
                    slot: 0,
                },
                Op::Project {
                    items: vec![(expr, 1)],
                    distinct: false,
                },
            ],
            columns: vec![Column {
                name: "r".into(),
                slot: 1,
            }],
        }
    }

    fn col(plan: &Plan) -> Vec<Value> {
        run(plan)
            .unwrap()
            .rows
            .into_iter()
            .map(|mut r| r.remove(0))
            .collect()
    }

    #[test]
    fn three_valued_logic_follows_cypher() {
        let t = lit(Value::Bool(true));
        let f = lit(Value::Bool(false));
        let n = lit(Value::Null);
        let and =
            |a: &Expr, b: &Expr| Expr::Binary(BinOp::And, Box::new(a.clone()), Box::new(b.clone()));
        let or =
            |a: &Expr, b: &Expr| Expr::Binary(BinOp::Or, Box::new(a.clone()), Box::new(b.clone()));
        let eval = |e: Expr| col(&over(vec![Value::Int(0)], e)).remove(0);
        assert_eq!(eval(and(&n, &f)), Value::Bool(false));
        assert_eq!(eval(and(&n, &t)), Value::Null);
        assert_eq!(eval(or(&n, &t)), Value::Bool(true));
        assert_eq!(eval(or(&n, &f)), Value::Null);
        assert_eq!(eval(Expr::Not(Box::new(n.clone()))), Value::Null);
    }

    #[test]
    fn comparing_with_null_is_null_and_a_filter_drops_the_row() {
        let plan = Plan {
            slots: 1,
            ops: vec![
                Op::ScanNodes {
                    slot: 0,
                    label: None,
                    props: vec![],
                },
                Op::Filter(Expr::Compare(
                    Box::new(Expr::Property(Box::new(Expr::Slot(0)), "n".into())),
                    vec![(CmpOp::Gt, lit(Value::Int(1)))],
                )),
            ],
            columns: vec![Column {
                name: "n".into(),
                slot: 0,
            }],
        };
        // a: 1 > 1 is false, b: 2 > 1 is true, c: null > 1 is null.
        assert_eq!(run(&plan).unwrap().rows, vec![vec![Value::Node(NodeId(1))]]);
    }

    #[test]
    fn in_follows_the_null_rules() {
        let list = |v: Vec<Value>| Box::new(lit(Value::List(v)));
        let x = |v: Value| Box::new(lit(v));
        let eval = |e: Expr| col(&over(vec![Value::Int(0)], e)).remove(0);
        assert_eq!(
            eval(Expr::In(
                x(Value::Int(1)),
                list(vec![Value::Int(1), Value::Null])
            )),
            Value::Bool(true)
        );
        assert_eq!(
            eval(Expr::In(
                x(Value::Int(2)),
                list(vec![Value::Int(1), Value::Null])
            )),
            Value::Null
        );
        assert_eq!(
            eval(Expr::In(x(Value::Int(2)), list(vec![Value::Int(1)]))),
            Value::Bool(false)
        );
        assert_eq!(
            eval(Expr::In(x(Value::Null), list(vec![]))),
            Value::Bool(false)
        );
        assert_eq!(
            eval(Expr::In(x(Value::Null), list(vec![Value::Int(1)]))),
            Value::Null
        );
    }

    #[test]
    fn an_integer_equals_the_float_of_the_same_value() {
        assert_eq!(equals(&Value::Int(1), &Value::Float(1.0)), Some(true));
        assert_eq!(
            equals(&Value::Int(1), &Value::String("1".into())),
            Some(false)
        );
        assert_eq!(equals(&Value::Null, &Value::Null), None);
    }

    #[test]
    fn integer_overflow_and_division_by_zero_are_errors_not_wraps() {
        let add = Expr::Binary(
            BinOp::Add,
            Box::new(Expr::Slot(0)),
            Box::new(lit(Value::Int(1))),
        );
        let e = run(&over(vec![Value::Int(i64::MAX)], add)).unwrap_err();
        assert_eq!(e.class, crate::ErrorClass::Runtime);
        let div = Expr::Binary(
            BinOp::Div,
            Box::new(lit(Value::Int(1))),
            Box::new(Expr::Slot(0)),
        );
        assert!(run(&over(vec![Value::Int(0)], div))
            .unwrap_err()
            .message
            .contains("division by zero"));
        let fdiv = Expr::Binary(
            BinOp::Div,
            Box::new(lit(Value::Float(0.0))),
            Box::new(lit(Value::Float(0.0))),
        );
        assert!(run(&over(vec![Value::Int(0)], fdiv))
            .unwrap_err()
            .message
            .contains("NaN"));
    }

    #[test]
    fn integer_division_truncates_and_arithmetic_mixes_integers_and_floats() {
        let d = |a: i64, b: i64| {
            Expr::Binary(
                BinOp::Div,
                Box::new(lit(Value::Int(a))),
                Box::new(lit(Value::Int(b))),
            )
        };
        assert_eq!(
            col(&over(vec![Value::Int(0)], d(7, 2))),
            vec![Value::Int(3)]
        );
        assert_eq!(
            col(&over(vec![Value::Int(0)], d(-7, 2))),
            vec![Value::Int(-3)]
        );
        let m = Expr::Binary(
            BinOp::Mul,
            Box::new(lit(Value::Int(2))),
            Box::new(lit(Value::Float(1.5))),
        );
        assert_eq!(col(&over(vec![Value::Int(0)], m)), vec![Value::Float(3.0)]);
    }

    #[test]
    fn order_by_puts_null_last_ascending_and_first_descending_and_keeps_ties_in_order() {
        let sorted = |descending: bool| {
            let plan = Plan {
                slots: 1,
                ops: vec![
                    Op::ScanNodes {
                        slot: 0,
                        label: None,
                        props: vec![],
                    },
                    Op::Sort(vec![SortKey {
                        expr: Expr::Property(Box::new(Expr::Slot(0)), "n".into()),
                        descending,
                    }]),
                ],
                columns: vec![Column {
                    name: "n".into(),
                    slot: 0,
                }],
            };
            run(&plan)
                .unwrap()
                .rows
                .into_iter()
                .map(|mut r| r.remove(0))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            sorted(false),
            vec![
                Value::Node(NodeId(0)),
                Value::Node(NodeId(1)),
                Value::Node(NodeId(2))
            ]
        );
        assert_eq!(
            sorted(true),
            vec![
                Value::Node(NodeId(2)),
                Value::Node(NodeId(1)),
                Value::Node(NodeId(0))
            ]
        );
    }

    #[test]
    fn aggregates_over_nothing_and_over_groups() {
        let count_all = |label: Option<&str>| Plan {
            slots: 2,
            ops: vec![
                Op::ScanNodes {
                    slot: 0,
                    label: label.map(str::to_string),
                    props: vec![],
                },
                Op::Aggregate {
                    keys: vec![],
                    aggregates: vec![Aggregate {
                        func: AggFunc::Count,
                        arg: None,
                        distinct: false,
                        slot: 1,
                    }],
                },
            ],
            columns: vec![Column {
                name: "c".into(),
                slot: 1,
            }],
        };
        assert_eq!(col(&count_all(None)), vec![Value::Int(3)]);
        // No node has this label: without keys there is still one row, with 0.
        assert_eq!(col(&count_all(Some("Z"))), vec![Value::Int(0)]);
        // Grouped by label: K twice, L once, in the order first seen.
        let grouped = Plan {
            slots: 3,
            ops: vec![
                Op::ScanNodes {
                    slot: 0,
                    label: None,
                    props: vec![],
                },
                Op::Aggregate {
                    keys: vec![(Expr::Property(Box::new(Expr::Slot(0)), "n".into()), 1)],
                    aggregates: vec![Aggregate {
                        func: AggFunc::Count,
                        arg: None,
                        distinct: false,
                        slot: 2,
                    }],
                },
            ],
            columns: vec![
                Column {
                    name: "n".into(),
                    slot: 1,
                },
                Column {
                    name: "c".into(),
                    slot: 2,
                },
            ],
        };
        assert_eq!(
            run(&grouped).unwrap().rows,
            vec![
                vec![Value::Int(1), Value::Int(1)],
                vec![Value::Int(2), Value::Int(1)],
                vec![Value::Null, Value::Int(1)],
            ]
        );
    }

    #[test]
    fn sum_adds_integers_exactly_and_overflow_is_an_error() {
        let sum = |values: Vec<Value>| {
            let plan = Plan {
                slots: 2,
                ops: vec![
                    Op::Unwind {
                        expr: lit(Value::List(values)),
                        slot: 0,
                    },
                    Op::Aggregate {
                        keys: vec![],
                        aggregates: vec![Aggregate {
                            func: AggFunc::Sum,
                            arg: Some(Expr::Slot(0)),
                            distinct: false,
                            slot: 1,
                        }],
                    },
                ],
                columns: vec![Column {
                    name: "s".into(),
                    slot: 1,
                }],
            };
            run(&plan).map(|r| r.rows[0][0].clone())
        };
        assert_eq!(sum(vec![Value::Int(1), Value::Int(2)]), Ok(Value::Int(3)));
        assert_eq!(
            sum(vec![Value::Int(1), Value::Float(0.5)]),
            Ok(Value::Float(1.5))
        );
        assert_eq!(sum(vec![]), Ok(Value::Int(0)));
        assert!(sum(vec![Value::Int(i64::MAX), Value::Int(1)]).is_err());
    }

    #[test]
    fn the_work_budget_stops_a_query_and_says_so() {
        let plan = Plan {
            slots: 2,
            ops: vec![
                Op::ScanNodes {
                    slot: 0,
                    label: None,
                    props: vec![],
                },
                Op::ScanNodes {
                    slot: 1,
                    label: None,
                    props: vec![],
                },
            ],
            columns: vec![],
        };
        let tight = Limits {
            max_steps: 5,
            max_rows: 1_000,
        };
        let e = execute(&plan, &Tiny, &BTreeMap::new(), tight).unwrap_err();
        assert_eq!(e.class, crate::ErrorClass::Limit);
        assert!(e.message.contains("stopped after"), "{e}");
        let rows = Limits {
            max_steps: 1_000,
            max_rows: 4,
        };
        assert_eq!(
            execute(&plan, &Tiny, &BTreeMap::new(), rows)
                .unwrap_err()
                .class,
            crate::ErrorClass::Limit
        );
    }

    #[test]
    fn a_negative_limit_and_a_missing_parameter_are_errors() {
        let plan = |limit: Expr| Plan {
            slots: 1,
            ops: vec![Op::Limit(limit)],
            columns: vec![],
        };
        assert!(run(&plan(lit(Value::Int(-1)))).is_err());
        assert!(run(&plan(Expr::Param("n".into())))
            .unwrap_err()
            .message
            .contains("$n"));
        let params = BTreeMap::from([("n".to_string(), Value::Int(0))]);
        assert!(execute(
            &plan(Expr::Param("n".into())),
            &Tiny,
            &params,
            Limits::default()
        )
        .unwrap()
        .rows
        .is_empty());
    }
}
