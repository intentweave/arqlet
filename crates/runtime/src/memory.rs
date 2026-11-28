//! Provisional (ADR 0023, R7): what a session holds, part by part, as heap bytes estimated from the sizes and lengths of its
//! values (`memory`). An estimate, not a count: strings and vectors by their capacity, B-tree maps by the nodes their
//! length needs, JSON values by their parts. The counted heap of `benchmarks/session-memory` is the reference it is checked
//! against. Pure: no allocator, no host API; an adapter adds what only it knows (the WASM heap and linear memory).
use arqlet_model::{Graph, GraphIndex, Rect, Route};
use serde_json::Value;
use std::collections::BTreeMap;
use std::mem::size_of;

/// The bytes of a B-tree map of `len` entries of `entry` bytes each, without what the entries point to: Rust's B-tree keeps
/// up to 11 entries in a node, about two thirds full on average, plus the internal nodes.
pub fn btree(len: usize, entry: usize) -> usize {
    if len == 0 {
        return 0;
    }
    let leaves = len.div_ceil(8);
    let leaf = 11 * entry + 16;
    // An internal node per 8 children or so, each with 12 child pointers beside its entries.
    let internal = leaves.saturating_sub(1).div_ceil(7);
    leaves * leaf + internal * (leaf + 12 * size_of::<usize>())
}

fn opt_string(s: &Option<String>) -> usize {
    s.as_ref().map_or(0, |s| s.capacity())
}

/// The heap of a JSON value beyond the value itself.
pub fn value(v: &Value) -> usize {
    match v {
        Value::String(s) => s.capacity(),
        Value::Array(items) => {
            items.capacity() * size_of::<Value>() + items.iter().map(value).sum::<usize>()
        }
        Value::Object(map) => {
            btree(map.len(), size_of::<String>() + size_of::<Value>())
                + map
                    .iter()
                    .map(|(k, v)| k.capacity() + value(v))
                    .sum::<usize>()
        }
        _ => 0,
    }
}

/// The heap of a graph: its lists and every string in them (ports, buses and memberships included, roughly).
pub fn graph(g: &Graph) -> usize {
    let nodes = g.nodes.capacity() * size_of::<arqlet_model::Node>()
        + g.nodes
            .iter()
            .map(|n| {
                n.id.capacity() + n.kind.capacity() + n.label.capacity() + opt_string(&n.parent_id)
            })
            .sum::<usize>();
    let edges = g.edges.capacity() * size_of::<arqlet_model::Edge>()
        + g.edges
            .iter()
            .map(|e| {
                e.id.capacity() + e.kind.capacity() + e.source.capacity() + opt_string(&e.target)
            })
            .sum::<usize>();
    let ports = g.ports.capacity() * size_of::<arqlet_model::Port>()
        + g.ports
            .iter()
            .map(|p| p.id.capacity() + p.node_id.capacity() + opt_string(&p.row))
            .sum::<usize>();
    let edge_ports = g.edge_ports.capacity() * size_of::<arqlet_model::EdgePorts>()
        + g.edge_ports
            .iter()
            .map(|b| b.edge_id.capacity() + opt_string(&b.source_port) + opt_string(&b.target_port))
            .sum::<usize>();
    let buses = g.buses.capacity() * size_of::<arqlet_model::Bus>()
        + g.buses
            .iter()
            .map(|b| {
                b.id.capacity()
                    + b.edges.capacity() * size_of::<String>()
                    + b.edges.iter().map(|e| e.capacity()).sum::<usize>()
                    + b.taps.capacity() * size_of::<arqlet_model::BusTap>()
                    + b.taps
                        .iter()
                        .map(|t| t.edge.capacity() + t.node.capacity() + t.kind.capacity())
                        .sum::<usize>()
            })
            .sum::<usize>();
    let memberships = g
        .memberships
        .dimensions
        .iter()
        .map(|d| {
            d.id.capacity()
                + d.label.capacity()
                + d.groups
                    .iter()
                    .map(|gr| {
                        size_of::<arqlet_model::MembershipGroup>()
                            + gr.id.capacity()
                            + gr.label.capacity()
                            + opt_string(&gr.parent)
                            + gr.members.capacity() * size_of::<String>()
                            + gr.members.iter().map(|m| m.capacity()).sum::<usize>()
                    })
                    .sum::<usize>()
        })
        .sum::<usize>();
    nodes + edges + ports + edge_ports + buses + memberships
}

/// The heap of a graph's index: the id map (each id copied once more) and the per-node and per-edge vectors.
pub fn index(i: &GraphIndex) -> usize {
    btree(i.node.len(), size_of::<String>() + size_of::<usize>())
        + i.node.keys().map(|k| k.capacity()).sum::<usize>()
        + i.parent.capacity() * size_of::<Option<usize>>()
        + i.container.capacity()
        + i.ends.capacity() * size_of::<(Option<usize>, Option<usize>)>()
}

/// The rest of a snapshot (labels, resolved ports, border crossings, conflicts, diagnostics, bus lines, the options and the
/// view state): the length of its JSON text, a rough stand-in for the heap of these many small values.
pub fn snapshot_rest(s: &arqlet_model::Snapshot) -> usize {
    let len = |v: serde_json::Result<String>| v.map_or(0, |t| t.len());
    len(serde_json::to_string(&s.labels))
        + len(serde_json::to_string(&s.resolved_ports))
        + len(serde_json::to_string(&s.boundary_crossings))
        + len(serde_json::to_string(&s.geometry_issues))
        + len(serde_json::to_string(&s.route_conflicts))
        + len(serde_json::to_string(&s.layout_diagnostics))
        + len(serde_json::to_string(&s.routing_issues))
        + len(serde_json::to_string(&s.bus_lines))
        + len(serde_json::to_string(&s.hub_refs))
        + len(serde_json::to_string(&s.options))
        + len(serde_json::to_string(&s.collapsed_ids))
        + len(serde_json::to_string(&s.optimization))
}

/// The layered plans a snapshot keeps for later layouts to continue from (`layeredPlans`): the length of their JSON text.
/// On a large first view they are the largest part of the snapshot after the source graph.
pub fn layered_plans(s: &arqlet_model::Snapshot) -> usize {
    serde_json::to_string(&s.layered_plans).map_or(0, |t| t.len())
}

pub fn boxes(b: &BTreeMap<String, Rect>) -> usize {
    btree(b.len(), size_of::<String>() + size_of::<Rect>())
        + b.keys().map(|k| k.capacity()).sum::<usize>()
}

pub fn routes(r: &[Route], capacity: usize) -> usize {
    capacity * size_of::<Route>()
        + r.iter()
            .map(|r| {
                r.id.capacity()
                    + r.points.capacity() * size_of::<arqlet_model::Point>()
                    + opt_string(&r.bundle_of)
            })
            .sum::<usize>()
}

/// The heap of a property table (id → key → value).
pub fn properties(p: &crate::content::Properties) -> usize {
    let per_map = size_of::<String>() + size_of::<BTreeMap<String, Value>>();
    btree(p.len(), per_map)
        + p.iter()
            .map(|(id, props)| {
                id.capacity()
                    + btree(props.len(), size_of::<String>() + size_of::<Value>())
                    + props
                        .iter()
                        .map(|(k, v)| k.capacity() + value(v))
                        .sum::<usize>()
            })
            .sum::<usize>()
}

pub fn columns(c: &crate::columns::SceneColumns) -> usize {
    let strings = |v: &Vec<String>| {
        v.capacity() * size_of::<String>() + v.iter().map(|s| s.capacity()).sum::<usize>()
    };
    strings(&c.node_ids)
        + strings(&c.node_labels)
        + strings(&c.node_kinds)
        + strings(&c.route_ids)
        + c.node_bounds.capacity() * 4
        + (c.node_parent.capacity()
            + c.node_flags.capacity()
            + c.node_kind.capacity()
            + c.node_shape.capacity()
            + c.node_child_count.capacity()
            + c.node_hidden_count.capacity()
            + c.route_source.capacity()
            + c.route_target.capacity()
            + c.route_weight.capacity()
            + c.route_flags.capacity())
            * 4
}

/// The answer to a session's `memory` or `release` with what only an adapter knows: the live heap of the whole module it
/// counts (every session and the answer being written) as `heapBytes`, and in WASM the size of the linear memory, which
/// WebAssembly never gives back, as `linearMemoryBytes`. Any other answer is passed through unchanged.
pub fn with_adapter_heap(
    request: &str,
    answer: String,
    heap_bytes: usize,
    linear_memory_bytes: Option<u64>,
) -> String {
    with_adapter_heap_peak(request, answer, heap_bytes, None, linear_memory_bytes)
}

/// Whether an answer is one that `with_adapter_heap` completes: a session's `memory` or `release`.
pub fn is_memory_answer(request: &str, answer: &str) -> bool {
    (request.contains("\"memory\"") || request.contains("\"release\""))
        && answer.contains("\"estimated\":true")
        && answer.ends_with('}')
}

/// `with_adapter_heap` that also reports the adapter's highest live heap since the previous such answer
/// (`heapPeakBytes`, provisional, ADR 0024 C3), where the adapter counts it.
pub fn with_adapter_heap_peak(
    request: &str,
    answer: String,
    heap_bytes: usize,
    heap_peak_bytes: Option<usize>,
    linear_memory_bytes: Option<u64>,
) -> String {
    if !is_memory_answer(request, &answer) {
        return answer;
    }
    let mut out = answer;
    out.pop();
    out.push_str(&format!(",\"heapBytes\":{heap_bytes}"));
    if let Some(peak) = heap_peak_bytes {
        out.push_str(&format!(",\"heapPeakBytes\":{peak}"));
    }
    if let Some(linear) = linear_memory_bytes {
        out.push_str(&format!(",\"linearMemoryBytes\":{linear}"));
    }
    out.push('}');
    out
}
