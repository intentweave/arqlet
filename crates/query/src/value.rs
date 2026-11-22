//! Values and identities (provisional).

/// A node of a store: an index into the store's fixed node order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(pub u32);

/// An edge of a store: an index into the store's fixed edge order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EdgeId(pub u32);

/// A value a query computes or a store holds. Integers are `i64`, floats `f64`; a node or an edge is its identity.
///
/// `PartialEq` is the structural equality of Rust (for tests); the equality of Cypher, with `null` and with integers
/// against floats, is `exec`'s.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    List(Vec<Value>),
    Node(NodeId),
    Edge(EdgeId),
}

impl Value {
    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }

    /// The name of the kind of value, for messages.
    pub fn kind(&self) -> &'static str {
        match self {
            Value::Null => "null",
            Value::Bool(_) => "boolean",
            Value::Int(_) => "integer",
            Value::Float(_) => "float",
            Value::String(_) => "string",
            Value::List(_) => "list",
            Value::Node(_) => "node",
            Value::Edge(_) => "relationship",
        }
    }
}
