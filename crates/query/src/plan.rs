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
    /// `OPTIONAL MATCH`: runs the operations on every row; a row for which they give nothing stays once, with what they
    /// would have bound left null.
    Optional(Vec<Op>),
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
    /// `RETURN DISTINCT`: keeps the first row of each distinct combination of the values in these slots.
    Distinct(Vec<Slot>),
    /// `ORDER BY`: a stable sort.
    Sort(Vec<SortKey>),
    /// `SKIP`.
    Skip(Expr),
    /// `LIMIT`.
    Limit(Expr),
    /// `UNION` and `UNION ALL`: the whole query. Runs every branch, a plan of its own with its own slots, and hands on the
    /// columns of their rows, the branches in the order written: column `i` of a branch goes into slot `i`. With
    /// `distinct`, a row equal to an earlier one is left out. The rows it gets are ignored.
    Union { branches: Vec<Plan>, distinct: bool },
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
    /// The variable in `rel` is bound already: only the edge (or, with `hops`, the list of edges) it holds fits.
    pub rel_bound: bool,
    pub to_label: Option<String>,
    pub to_props: Vec<(String, Expr)>,
    pub distinct_from: Vec<Slot>,
    /// `None` for one hop. `Some((min, max))` for `*min..max` (`max` `None` is unbounded, capped by the limits): `rel`
    /// then holds the list of the edges of the path, and no edge is used twice within a path.
    pub hops: Option<(u32, Option<u32>)>,
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
    /// A scalar function with its arguments, as many as [`Function::arity`] allows.
    Call(Function, Vec<Expr>),
    /// `{key: value, …}`; every key once.
    Map(Vec<(String, Expr)>),
    /// `base[index]`: an item of a list (from the end when negative), the value of a key of a map, or a property of a
    /// node or relationship.
    Index(Box<Expr>, Box<Expr>),
    /// `list[from..to]`: `from` included, `to` not; either may be left out and negative counts from the end.
    Slice(Box<Expr>, Option<Box<Expr>>, Option<Box<Expr>>),
    /// `[x IN list WHERE filter | map]`: `slot` is `x`, which holds each item in turn and exists inside `filter` and
    /// `map` only.
    Comprehension {
        slot: Slot,
        list: Box<Expr>,
        filter: Option<Box<Expr>>,
        map: Option<Box<Expr>>,
    },
}

/// A scalar function (ADR 0021, "The function table").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Function {
    /// `id(x)` and `elementId(x)`: the store's property `id` of a node or relationship.
    Id,
    /// `type(r)`.
    Type,
    /// `labels(n)`: the list of the node's one label.
    Labels,
    /// `size(x)`: the characters of a string, the items of a list.
    Size,
    /// `coalesce(a, …)`: the first argument that is not null; those after it are not evaluated.
    Coalesce,
    ToString,
    ToUpper,
    ToLower,
    Trim,
    LTrim,
    RTrim,
    Replace,
    Substring,
    Left,
    Right,
    Split,
    /// `reverse(x)`: of a string or a list.
    Reverse,
}

impl Function {
    /// The name in messages, as openCypher writes it (names are matched without regard to case).
    pub const fn name(self) -> &'static str {
        match self {
            Function::Id => "id",
            Function::Type => "type",
            Function::Labels => "labels",
            Function::Size => "size",
            Function::Coalesce => "coalesce",
            Function::ToString => "toString",
            Function::ToUpper => "toUpper",
            Function::ToLower => "toLower",
            Function::Trim => "trim",
            Function::LTrim => "ltrim",
            Function::RTrim => "rtrim",
            Function::Replace => "replace",
            Function::Substring => "substring",
            Function::Left => "left",
            Function::Right => "right",
            Function::Split => "split",
            Function::Reverse => "reverse",
        }
    }

    /// The fewest and the most arguments it takes.
    pub const fn arity(self) -> (usize, usize) {
        match self {
            Function::Coalesce => (1, usize::MAX),
            Function::Replace => (3, 3),
            Function::Substring => (2, 3),
            Function::Left | Function::Right | Function::Split => (2, 2),
            _ => (1, 1),
        }
    }
}

impl Expr {
    /// Calls `f` on every expression directly inside this one.
    pub fn for_each_child_mut(&mut self, f: &mut dyn FnMut(&mut Expr)) {
        match self {
            Expr::Literal(_) | Expr::Param(_) | Expr::Slot(_) => {}
            Expr::Property(x, _)
            | Expr::Not(x)
            | Expr::Neg(x)
            | Expr::IsNull(x, _)
            | Expr::HasLabel(x, _) => f(x),
            Expr::List(items) | Expr::Call(_, items) => items.iter_mut().for_each(f),
            Expr::Binary(_, a, b) | Expr::In(a, b) => {
                f(a);
                f(b);
            }
            Expr::Map(entries) => entries.iter_mut().for_each(|(_, value)| f(value)),
            Expr::Index(a, b) => {
                f(a);
                f(b);
            }
            Expr::Slice(list, from, to) => {
                f(list);
                if let Some(x) = from {
                    f(x);
                }
                if let Some(x) = to {
                    f(x);
                }
            }
            Expr::Comprehension {
                list, filter, map, ..
            } => {
                f(list);
                if let Some(x) = filter {
                    f(x);
                }
                if let Some(x) = map {
                    f(x);
                }
            }
            Expr::Compare(left, chain) => {
                f(left);
                chain.iter_mut().for_each(|(_, right)| f(right));
            }
            Expr::Case {
                scrutinee,
                alternatives,
                default,
            } => {
                if let Some(s) = scrutinee {
                    f(s);
                }
                for (when, then) in alternatives {
                    f(when);
                    f(then);
                }
                if let Some(d) = default {
                    f(d);
                }
            }
        }
    }
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
