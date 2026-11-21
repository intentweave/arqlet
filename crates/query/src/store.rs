//! The seam between the executor and the graph it reads (provisional, ADR 0021 item 3).

use crate::value::{EdgeId, NodeId, Value};

/// Which edges of a node a step follows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// Edges that start at the node: `(n)-[r]->()`.
    Out,
    /// Edges that end at the node: `(n)<-[r]-()`.
    In,
    /// Both: `(n)-[r]-()`. An edge that starts and ends at the node (a self-loop) is listed once.
    Both,
}

/// An edge seen from one of its ends: the edge and the node at its other end. For a self-loop the other end is the node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EdgeRef {
    pub edge: EdgeId,
    pub other: NodeId,
}

/// What the executor reads of a graph. Everything is read in place and returned in a fixed order, never in hash order.
///
/// Nodes are `0..node_count()` and edges `0..edge_count()`, in orders the store defines (for an Arqlet graph, the order of
/// the export). An edge has a source, a target and one type (its `kind`); an edge whose target the graph does not have is
/// not in the store at all, as the project rule says (ADR 0021 M0). The revision a read names comes with a later stage.
pub trait GraphStore {
    /// The number of nodes.
    fn node_count(&self) -> usize;

    /// The single label of a node (its `kind`).
    fn node_label(&self, node: NodeId) -> &str;

    /// A property of a node: a virtual one (`id`, `kind`, `label`, `parentId`) or one of the payload. `Value::Null` when
    /// the node has none.
    fn node_property(&self, node: NodeId, key: &str) -> Value;

    /// The number of edges.
    fn edge_count(&self) -> usize;

    /// The type of an edge (its `kind`).
    fn edge_type(&self, edge: EdgeId) -> &str;

    /// A property of an edge: a virtual one (`id`, `kind`, `weight`) or one of the payload. `Value::Null` when it has none.
    fn edge_property(&self, edge: EdgeId, key: &str) -> Value;

    /// The edges of a node in a direction, in the order of their ids, each once. The ends of an edge are the source (where it
    /// starts) and the target (where it ends); a parallel edge is an edge of its own.
    fn edges_of(&self, node: NodeId, direction: Direction) -> Vec<EdgeRef>;
}
