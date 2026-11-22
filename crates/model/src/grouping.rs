//! Provisional (ADR 0022 §2): the grouping priority — which structure owns a node in the view.
//!
//! A view ranks the structures it shows: the parent hierarchy (`parent`) and any membership dimensions. A node's owner is
//! its group in the **first criterion that assigns it one**; every later criterion holds it softly. `parent` assigns every
//! node (its container, or the root), so listing it anywhere guarantees an owner. In a dimension, a node in several groups
//! that are not one path is owned by the group its criterion's value order names first. A node that no listed criterion
//! assigns has no owner and is reported.
//!
//! Pure and deterministic: one reverse index per dimension, no pass over all nodes per group.
use crate::{Graph, GroupingCriterion, Memberships};
use std::collections::{BTreeMap, BTreeSet};

/// The criterion name of the parent hierarchy. Reserved: no dimension may use it.
pub const PARENT: &str = "parent";

/// The prefix of the view containers a group that owns nodes becomes: `membership:<dimension>/<group>`.
pub const CONTAINER_PREFIX: &str = "membership:";

/// The id of the view container of a group.
pub fn container_id(dimension: &str, group: &str) -> String {
    format!("{CONTAINER_PREFIX}{dimension}/{group}")
}

/// Whether a priority is the default view: empty, or the parent hierarchy alone. Such a view is today's view.
pub fn is_default(priority: &[GroupingCriterion]) -> bool {
    priority.is_empty() || (priority.len() == 1 && priority[0].by == PARENT)
}

/// Whether `dimension` can own nodes under `priority`: it is listed, and before `parent` (which assigns every node, so
/// nothing after it ever owns). Only such a dimension shapes the view's tree; a dimension held softly is only drawn.
pub fn can_own(priority: &[GroupingCriterion], dimension: &str) -> bool {
    for c in priority {
        if c.by == PARENT {
            return false;
        }
        if c.by == dimension {
            return true;
        }
    }
    false
}

/// What owns a node in the view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Owner {
    /// The parent hierarchy: the node's own container, or the root (`None`).
    Parent(Option<String>),
    /// A group of a membership dimension.
    Group { dimension: String, group: String },
    /// No listed criterion assigns the node (ADR 0022 §2, rule 3): it is placed at the root and reported.
    Unassigned,
}

/// The owners of every node of a graph under a priority.
#[derive(Clone, Debug, Default)]
pub struct Ownership {
    /// The owner of every node, by node id.
    pub owners: BTreeMap<String, Owner>,
    /// The rank (index in the priority) of the criterion that owns each assigned node.
    pub rank: BTreeMap<String, usize>,
    /// Nodes in several groups of their owning dimension that are not one path: the value order chose (rule 2).
    pub multiple: BTreeSet<String>,
    /// Nodes no listed criterion assigns (rule 3).
    pub unassigned: BTreeSet<String>,
}

/// One dimension prepared for ownership: its groups' parents and order, and node → groups in declared order.
struct Prepared<'a> {
    parent: BTreeMap<&'a str, Option<&'a str>>,
    groups_of: BTreeMap<&'a str, Vec<&'a str>>,
}

impl<'a> Prepared<'a> {
    fn new(memberships: &'a Memberships, dimension: &str) -> Option<Self> {
        let d = memberships.dimension(dimension)?;
        let mut parent = BTreeMap::new();
        let mut groups_of: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for g in &d.groups {
            parent.insert(g.id.as_str(), g.parent.as_deref());
            for m in &g.members {
                let list = groups_of.entry(m.as_str()).or_default();
                if !list.contains(&g.id.as_str()) {
                    list.push(g.id.as_str());
                }
            }
        }
        Some(Self { parent, groups_of })
    }

    /// `group` and its ancestors in the dimension, innermost first. Bounded by the number of groups, so a cycle (refused by
    /// validation) cannot loop.
    fn chain(&self, group: &'a str) -> Vec<&'a str> {
        let mut out = vec![group];
        let mut at = group;
        while let Some(Some(p)) = self.parent.get(at) {
            if out.len() > self.parent.len() || out.contains(p) {
                break;
            }
            out.push(p);
            at = p;
        }
        out
    }
}

/// The owner of every node of `graph` under `priority` (empty means `[parent]`).
pub fn owners(graph: &Graph, priority: &[GroupingCriterion]) -> Ownership {
    let default = [GroupingCriterion {
        by: PARENT.into(),
        ..GroupingCriterion::default()
    }];
    let priority: &[GroupingCriterion] = if priority.is_empty() {
        &default
    } else {
        priority
    };
    let prepared: Vec<Option<Prepared>> = priority
        .iter()
        .map(|c| {
            if c.by == PARENT {
                None
            } else {
                Prepared::new(&graph.memberships, &c.by)
            }
        })
        .collect();
    // The value order of each dimension criterion: listed values first, then the declared order.
    let order: Vec<BTreeMap<&str, usize>> = priority
        .iter()
        .map(|c| {
            let mut rank = BTreeMap::new();
            for v in &c.values {
                let next = rank.len();
                rank.entry(v.as_str()).or_insert(next);
            }
            if let Some(d) = graph.memberships.dimension(&c.by) {
                for g in &d.groups {
                    let next = rank.len();
                    rank.entry(g.id.as_str()).or_insert(next);
                }
            }
            rank
        })
        .collect();

    let mut out = Ownership::default();
    for node in &graph.nodes {
        let mut owner = Owner::Unassigned;
        for (rank, c) in priority.iter().enumerate() {
            if c.by == PARENT {
                owner = Owner::Parent(node.parent_id.clone());
                out.rank.insert(node.id.clone(), rank);
                break;
            }
            let Some(d) = prepared[rank].as_ref() else {
                continue; // a dimension the graph does not declare assigns nothing
            };
            let Some(groups) = d.groups_of.get(node.id.as_str()) else {
                continue;
            };
            // One path (a group and its ancestors): the deepest owns. Otherwise the value order decides.
            let deepest = groups
                .iter()
                .copied()
                .max_by_key(|g| d.chain(g).len())
                .expect("a node listed in the index has a group");
            let path = d.chain(deepest);
            let chosen = if groups.iter().all(|g| path.contains(g)) {
                deepest
            } else {
                out.multiple.insert(node.id.clone());
                groups
                    .iter()
                    .copied()
                    .min_by_key(|g| order[rank].get(g).copied().unwrap_or(usize::MAX))
                    .expect("several groups")
            };
            owner = Owner::Group {
                dimension: c.by.clone(),
                group: chosen.to_string(),
            };
            out.rank.insert(node.id.clone(), rank);
            break;
        }
        if owner == Owner::Unassigned {
            out.unassigned.insert(node.id.clone());
        }
        out.owners.insert(node.id.clone(), owner);
    }
    out
}

/// The refusals of ADR 0022 §5 that need the graph: a criterion naming a dimension the graph does not declare, and a value
/// that is not a group of its dimension. (What the options alone decide is `LayoutOptions::validate_grouping`.)
pub fn validate(priority: &[GroupingCriterion], memberships: &Memberships) -> Result<(), String> {
    for c in priority {
        if c.by == PARENT {
            continue;
        }
        let d = memberships.dimension(&c.by).ok_or_else(|| {
            format!(
                "groupingPriority names {:?}, which is neither parent nor a declared dimension",
                c.by
            )
        })?;
        for v in &c.values {
            if !d.groups.iter().any(|g| &g.id == v) {
                return Err(format!(
                    "groupingPriority: {} declares no group {v:?}",
                    c.by
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn graph() -> Graph {
        serde_json::from_value(json!({
            "nodes": [
                {"id": "sales", "kind": "d", "label": "Sales", "parentId": null},
                {"id": "eng", "kind": "d", "label": "Eng", "parentId": null},
                {"id": "anna", "kind": "p", "label": "Anna", "parentId": "sales"},
                {"id": "dora", "kind": "p", "label": "Dora", "parentId": "eng"},
                {"id": "gus", "kind": "p", "label": "Gus", "parentId": "eng"}
            ],
            "edges": [],
            "memberships": {"dimensions": [
                {"id": "project", "groups": [
                    {"id": "apollo", "members": ["anna", "dora"]},
                    {"id": "zeus", "members": ["dora"]},
                    {"id": "core", "parent": "apollo", "members": ["anna"]}
                ]},
                {"id": "site", "groups": [{"id": "munich", "members": ["gus"]}]}
            ]}
        }))
        .unwrap()
    }

    fn by(name: &str, values: &[&str]) -> GroupingCriterion {
        GroupingCriterion {
            by: name.into(),
            values: values.iter().map(|v| v.to_string()).collect(),
            ..GroupingCriterion::default()
        }
    }

    fn group(d: &str, g: &str) -> Owner {
        Owner::Group {
            dimension: d.into(),
            group: g.into(),
        }
    }

    #[test]
    fn the_default_is_the_parent_hierarchy() {
        let g = graph();
        let o = owners(&g, &[]);
        assert_eq!(o.owners["anna"], Owner::Parent(Some("sales".into())));
        assert_eq!(o.owners["sales"], Owner::Parent(None));
        assert!(o.unassigned.is_empty() && o.multiple.is_empty());
        assert!(is_default(&[]) && is_default(&[by("parent", &[])]));
        assert!(!is_default(&[by("project", &[]), by("parent", &[])]));
    }

    #[test]
    fn the_first_assigning_criterion_owns_and_the_value_order_decides() {
        let g = graph();
        let o = owners(&g, &[by("project", &["zeus"]), by("parent", &[])]);
        // Dora is in apollo and zeus, not one path: zeus is named first.
        assert_eq!(o.owners["dora"], group("project", "zeus"));
        assert!(o.multiple.contains("dora"));
        // Anna is in apollo and its sub-group core: one path, so the deepest owns, and that is not a conflict.
        assert_eq!(o.owners["anna"], group("project", "core"));
        assert!(!o.multiple.contains("anna"));
        // Gus is on no project: the next criterion owns him.
        assert_eq!(o.owners["gus"], Owner::Parent(Some("eng".into())));
        assert_eq!(o.rank["gus"], 1);
        // Without a value order the declared order decides: apollo before zeus.
        let declared = owners(&g, &[by("project", &[]), by("parent", &[])]);
        assert_eq!(declared.owners["dora"], group("project", "apollo"));
    }

    #[test]
    fn a_node_no_criterion_assigns_is_unassigned() {
        let g = graph();
        let o = owners(&g, &[by("project", &[]), by("site", &[])]);
        assert_eq!(o.owners["gus"], group("site", "munich"));
        let only = owners(&g, &[by("project", &[])]);
        assert_eq!(only.owners["gus"], Owner::Unassigned);
        assert!(only.unassigned.contains("gus") && only.unassigned.contains("sales"));
    }

    #[test]
    fn validation_refuses_unknown_dimensions_and_values() {
        let g = graph();
        assert!(validate(
            &[by("project", &["zeus"]), by("parent", &[])],
            &g.memberships
        )
        .is_ok());
        assert!(validate(&[by("team", &[])], &g.memberships).is_err());
        assert!(validate(&[by("project", &["hermes"])], &g.memberships).is_err());
    }
}
