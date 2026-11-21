//! The seam between the executor and the graph it reads (provisional, ADR 0021 item 3).

use crate::value::{NodeId, Value};

/// What the executor reads of a graph. Everything is read in place and returned in a fixed order, never in hash order.
///
/// Nodes are `0..node_count()` in an order the store defines (for an Arqlet graph, the order of the export). This first
/// version holds the node part; relationships and the revision a read names come with the stages that need them.
pub trait GraphStore {
    /// The number of nodes.
    fn node_count(&self) -> usize;

    /// The single label of a node (its `kind`).
    fn node_label(&self, node: NodeId) -> &str;

    /// A property of a node: a virtual one (`id`, `kind`, `label`, `parentId`) or one of the payload. `Value::Null` when
    /// the node has none.
    fn node_property(&self, node: NodeId, key: &str) -> Value;
}
