//! Placing the end of an edge by hand: the user drags a port along the border of its node, and the engine records the new side
//! and offset as an explicit port of the graph. The same record is read back by `intent`, so an undo restores it. Provisional.
use super::{check_current, recompute_layout};
use arqlet_model::{EdgePorts, Port, PortSide, Snapshot};

/// The id of the port a user placed for one end of an edge.
pub fn user_port_id(edge_id: &str, endpoint: &str) -> String {
    format!("user:{edge_id}:{endpoint}")
}

/// Places the `endpoint` (`source` or `target`) of `edge_id` at `side` and `offset` (0 to 1 along the side; on a circle the
/// position within the 90 degrees of the side), or, with `placement` of `None`, hands the end back to the layout. An end
/// that was bound to a port the graph declares is bound to the user's port instead; the declared port stays.
pub fn set_port(
    snapshot: &Snapshot,
    expected: u64,
    layout: u64,
    edge_id: &str,
    endpoint: &str,
    placement: Option<(PortSide, f64)>,
) -> Result<Snapshot, String> {
    check_current(snapshot, expected, layout)?;
    if endpoint != "source" && endpoint != "target" {
        return Err(format!(
            "The end of an edge is source or target, not {endpoint}"
        ));
    }
    let mut source = snapshot
        .source_graph
        .clone()
        .unwrap_or_else(|| snapshot.graph.clone());
    let edge = source
        .edges
        .iter()
        .find(|e| e.id == edge_id)
        .ok_or_else(|| format!("Unknown edge in port placement: {edge_id}"))?;
    let node_id = if endpoint == "source" {
        Some(edge.source.clone())
    } else {
        edge.target.clone()
    }
    .ok_or_else(|| format!("The edge {edge_id} has no {endpoint}"))?;
    let id = user_port_id(edge_id, endpoint);
    source.ports.retain(|p| p.id != id);
    let position = source.edge_ports.iter().position(|b| b.edge_id == edge_id);
    let set = |binding: &mut EdgePorts, value: Option<String>| {
        if endpoint == "source" {
            binding.source_port = value;
        } else {
            binding.target_port = value;
        }
    };
    match placement {
        Some((side, offset)) => {
            if !offset.is_finite() || !(0. ..=1.).contains(&offset) {
                return Err(format!("A port offset is between 0 and 1, not {offset}"));
            }
            source.ports.push(Port {
                id: id.clone(),
                node_id,
                side,
                offset,
                row: None,
            });
            match position {
                Some(at) => set(&mut source.edge_ports[at], Some(id)),
                None => {
                    let mut binding = EdgePorts {
                        edge_id: edge_id.to_string(),
                        source_port: None,
                        target_port: None,
                    };
                    set(&mut binding, Some(id));
                    source.edge_ports.push(binding);
                }
            }
        }
        None => {
            if let Some(at) = position {
                let bound = if endpoint == "source" {
                    source.edge_ports[at].source_port.clone()
                } else {
                    source.edge_ports[at].target_port.clone()
                };
                // Only the user's own binding is handed back; a port the graph declares stays as it is.
                if bound.as_deref() == Some(id.as_str()) {
                    set(&mut source.edge_ports[at], None);
                }
                let binding = &source.edge_ports[at];
                if binding.source_port.is_none() && binding.target_port.is_none() {
                    source.edge_ports.remove(at);
                }
            }
        }
    }
    let mut changed = snapshot.clone();
    changed.source_graph = Some(source);
    recompute_layout(&changed, &snapshot.options, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{history, import_view};

    fn base() -> Snapshot {
        let text = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../examples/fixtures/small.json"),
        )
        .unwrap();
        import_view(&text, 100).unwrap()
    }

    fn end_of(snapshot: &Snapshot, edge: &str, endpoint: &str) -> (PortSide, f64, f64) {
        let port = snapshot
            .resolved_ports
            .iter()
            .find(|p| p.edge_id == edge && p.endpoint == endpoint)
            .unwrap();
        (port.side, port.position.x, port.position.y)
    }

    #[test]
    fn a_placed_port_is_where_it_was_put_and_can_be_handed_back() {
        let s0 = base();
        let edge = s0
            .graph
            .edges
            .iter()
            .find(|e| e.target.is_some())
            .unwrap()
            .id
            .clone();
        let s1 = set_port(
            &s0,
            s0.revision,
            s0.layout_version,
            &edge,
            "source",
            Some((PortSide::Top, 0.25)),
        )
        .unwrap();
        let (side, x, y) = end_of(&s1, &edge, "source");
        assert_eq!(side, PortSide::Top);
        let node = s1
            .graph
            .edges
            .iter()
            .find(|e| e.id == edge)
            .unwrap()
            .source
            .clone();
        let abs = |s: &Snapshot, id: &str| {
            let mut r = s.boxes[id].clone();
            let mut at = s
                .graph
                .nodes
                .iter()
                .find(|n| n.id == id)
                .unwrap()
                .parent_id
                .clone();
            while let Some(p) = at {
                r.x += s.boxes[&p].x;
                r.y += s.boxes[&p].y;
                at = s
                    .graph
                    .nodes
                    .iter()
                    .find(|n| n.id == p)
                    .unwrap()
                    .parent_id
                    .clone();
            }
            r
        };
        let r = abs(&s1, &node);
        assert!((y - r.y).abs() < 1e-6, "on the top border: {y} vs {}", r.y);
        assert!(
            (x - (r.x + 0.25 * r.width)).abs() < 1e-6,
            "a quarter along: {x}"
        );
        // The graph now declares the port, and the intent carries it.
        assert!(history::intent_of(&s1)
            .ports
            .iter()
            .any(|p| p.id == user_port_id(&edge, "source")));
        // Handing the end back removes the port and the binding.
        let s2 = set_port(&s1, s1.revision, s1.layout_version, &edge, "source", None).unwrap();
        assert!(history::intent_of(&s2).ports.is_empty());
        assert!(history::intent_of(&s2).edge_ports.is_empty());
        // Undo through the intent restores the placement.
        let back = history::restore_intent(
            &s2,
            s2.revision,
            s2.layout_version,
            &history::intent_of(&s1),
        )
        .unwrap();
        assert_eq!(end_of(&back, &edge, "source").0, PortSide::Top);
    }

    #[test]
    fn bad_requests_are_refused_by_name() {
        let s0 = base();
        let edge = s0.graph.edges[0].id.clone();
        let place = |edge: &str, end: &str, offset: f64| {
            set_port(
                &s0,
                s0.revision,
                s0.layout_version,
                edge,
                end,
                Some((PortSide::Left, offset)),
            )
        };
        assert!(place("nowhere", "source", 0.5)
            .unwrap_err()
            .contains("Unknown edge"));
        assert!(place(&edge, "middle", 0.5)
            .unwrap_err()
            .contains("source or target"));
        assert!(place(&edge, "source", 1.5)
            .unwrap_err()
            .contains("between 0 and 1"));
        assert!(set_port(
            &s0,
            s0.revision,
            s0.layout_version + 1,
            &edge,
            "source",
            None
        )
        .unwrap_err()
        .contains("Stale"));
    }
}
