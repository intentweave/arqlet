//! The plan IR (provisional, ADR 0021 item 2): what the front end lowers a query into and the executor runs.
//!
//! A plan is a list of operations over rows. A row is a vector of values, one per slot; a variable of the query is a slot.
//! The IR knows no parser: another front end could produce it.

use crate::store::Direction;
use crate::value::Value;

/// The position of a variable (or of a computed column) in a row.
pub type Slot = usize;

/// A query lowered: the operations in order, how many slots a row has, and the columns of the answer in the order the
/// query names them.
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub slots: usize,
    pub ops: Vec<Op>,
    pub columns: Vec<Column>,
}

/// A column of the answer.
#[derive(Clone, Debug, PartialEq)]
pub struct Column {
    pub name: String,
    pub slot: Slot,
}

/// One step of a plan. Every operation turns the rows it gets into the rows it hands on.
#[derive(Clone, Debug, PartialEq)]
pub enum Op {
    /// `MATCH (n:Label {key: value})` for a variable not bound yet: for every row, one row per node that has the label and
    /// the properties, with the node in `slot`.
    ScanNodes {
        slot: Slot,
        label: Option<String>,
        props: Vec<(String, Expr)>,
    },
    /// The same pattern for a variable that is bound already: keeps the rows whose node has the label and the properties.
    CheckNode {
        slot: Slot,
        label: Option<String>,
        props: Vec<(String, Expr)>,
    },
    /// A relationship of a pattern: from the node in `from`, one row per edge that fits.
    Expand(Expand),
    /// `WHERE`: keeps the rows for which the expression is true (not false, not null).
    Filter(Expr),
    /// `UNWIND`: one row per element of the list.
    Unwind { expr: Expr, slot: Slot },
    /// `WITH` and `RETURN` without aggregation: computes the items into their slots. The rows keep their other slots, so
    /// that an `ORDER BY` after it can still name what the projection left out.
    Project {
        items: Vec<(Expr, Slot)>,
        distinct: bool,
    },
    /// `WITH` and `RETURN` with aggregation: one row per distinct value of the keys, holding only the keys and the
    /// aggregates.
    Aggregate {
        keys: Vec<(Expr, Slot)>,
        aggregates: Vec<Aggregate>,
    },
    /// `ORDER BY`: a stable sort.
    Sort(Vec<SortKey>),
    /// `SKIP`.
    Skip(Expr),
    /// `LIMIT`.
    Limit(Expr),
}

/// One relationship of a pattern, `(from)-[rel:TYPE {key: value}]->(to)`.
///
/// For every row it takes the edges of the node in `from` in `direction`, keeps those of one of `types` (any type when
/// empty) that have `props`, and are not one of the edges already in `distinct_from` (a relationship is used once within
/// the pattern of one `MATCH`), and whose other end is the node in `to` when `to_bound`, or else has `to_label` and
/// `to_props` and is bound to `to`. The edge goes into `rel`.
#[derive(Clone, Debug, PartialEq)]
pub struct Expand {
    pub from: Slot,
    pub rel: Slot,
    pub to: Slot,
    pub direction: Direction,
    pub types: Vec<String>,
    pub props: Vec<(String, Expr)>,
    pub to_bound: bool,
    pub to_label: Option<String>,
    pub to_props: Vec<(String, Expr)>,
    pub distinct_from: Vec<Slot>,
}

/// One key of an `ORDER BY`.
#[derive(Clone, Debug, PartialEq)]
pub struct SortKey {
    pub expr: Expr,
    pub descending: bool,
}

/// An aggregate function.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AggFunc {
    Count,
    Sum,
    Min,
    Max,
    Avg,
    Collect,
}

/// One aggregate of an aggregation. `arg` is `None` for `count(*)`.
#[derive(Clone, Debug, PartialEq)]
pub struct Aggregate {
    pub func: AggFunc,
    pub arg: Option<Expr>,
    pub distinct: bool,
    pub slot: Slot,
}

/// An expression.
#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Literal(Value),
    /// `$name`.
    Param(String),
    Slot(Slot),
    /// `base.key`.
    Property(Box<Expr>, String),
    List(Vec<Expr>),
    Not(Box<Expr>),
    Neg(Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    /// A chain of comparisons, `a < b <= c`: every link has to hold.
    Compare(Box<Expr>, Vec<(CmpOp, Expr)>),
    /// `x IS NULL` (`negated: false`) and `x IS NOT NULL` (`negated: true`).
    IsNull(Box<Expr>, bool),
    /// `x IN list`.
    In(Box<Expr>, Box<Expr>),
    /// `n:Label`.
    HasLabel(Box<Expr>, String),
    Case {
        scrutinee: Option<Box<Expr>>,
        alternatives: Vec<(Expr, Expr)>,
        default: Option<Box<Expr>>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    And,
    Or,
    Xor,
    StartsWith,
    EndsWith,
    Contains,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CmpOp {
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
}
