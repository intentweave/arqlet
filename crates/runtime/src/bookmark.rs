//! Provisional (#151): the compact form of a view controller bookmark, short enough for a URL, and the reading of a
//! bookmark against a source that may have changed. Both operations are read only; the decisions they return are applied
//! with `restore`.
//!
//! The text is `b1.` and the base64url of a small JSON document (`Compact`):
//!
//! - **Only what differs from the defaults.** The collapsed containers are compared with those the engine would collapse
//!   on an import of this source with the snapshot's options (`default_collapsed`); the shorter of that difference and
//!   the whole list is kept. Every other decision is compared with `defaults`, the intent of the view as it was imported,
//!   when the caller gives it, and with an empty intent otherwise.
//! - **An id table.** Every id stands once, in the order of first use, as the length it shares with the one before and
//!   its rest; the decisions name ids by their place in the table, lists as differences of places. The ids themselves
//!   travel, not places in the source graph, so that a reader with another revision of the source can say which of them
//!   it no longer has.
//! - **Fingerprints** of the source without the fields a decision writes into it (`source_fingerprint`) and of the
//!   defaults compared with, so that a reader can tell that either differs.
//!
//! Reading never refuses a bookmark for what the source lost: the decisions that name a missing id, or no longer fit the
//! graph, are left out and listed (`Reading::missing`, `Reading::ignored`).
use arqlet_model::{
    ContainerSizing, DirectionConstraint, EdgePorts, Graph, GraphIndex, Hidden, Intent, NodeShape,
    NodeSize, OrderHint, Point, Port, PortSide, RoutingConstraint, Snapshot, StrategyConstraint,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

/// The start of a compact bookmark of this version. No base64url text holds a `.`, so the JSON form cannot be mistaken
/// for it.
pub const PREFIX: &str = "b1.";

/// The longest text read or written.
const MAX_TEXT: usize = 4 << 20;
/// The most bytes of ids a reading expands to (the table, and every use of an id): a short text cannot ask for more.
const MAX_ID_BYTES: usize = 16 << 20;

const REST: &str = arqlet_layout::communities::REST_PREFIX;

/// The path highlighted, or the neighbourhood: what the view controller keeps beside the decisions.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Highlight {
    Trace {
        from: String,
        to: String,
        /// The query's further fields (`mode`, `edgeKinds`), kept as they are.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        init: Option<Value>,
    },
    Neighbourhood {
        #[serde(rename = "nodeId")]
        node_id: NodeIds,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        init: Option<Value>,
    },
}

/// One id or a list of them.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum NodeIds {
    One(String),
    Many(Vec<String>),
}

/// The selected node or edge.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum Selection {
    #[serde(rename = "nodeId")]
    Node(String),
    #[serde(rename = "edgeId")]
    Edge(String),
}

/// What a bookmark holds beside the decisions.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Marks {
    pub highlight: Option<Highlight>,
    pub selection: Option<Selection>,
}

/// A decision of a bookmark that this graph cannot take.
#[derive(Clone, Debug, Serialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct Ignored {
    /// The kind of decision: `collapsed`, `pins`, `partial`, `hidden`, `highlight`, `selection`, …
    pub field: &'static str,
    pub id: String,
}

/// A bookmark read against a snapshot.
#[derive(Clone, Debug, PartialEq)]
pub struct Reading {
    /// The decisions that fit this graph, to `restore`.
    pub intent: Intent,
    pub marks: Marks,
    /// Whether the bookmark was made on this source (without the fields decisions write into it).
    pub same_source: bool,
    /// Whether the defaults it was compared with are the ones used to read it.
    pub same_defaults: bool,
    /// The ids the bookmark names that this graph does not have.
    pub missing: Vec<String>,
    /// The decisions left out: they name a missing id or no longer fit the graph.
    pub ignored: Vec<Ignored>,
}

fn is_false(value: &bool) -> bool {
    !*value
}
fn is_zero(value: &u8) -> bool {
    *value == 0
}
fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}

/// The document inside the text. Short names: every byte is part of a URL.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Compact {
    /// The source's fingerprint.
    s: String,
    /// The fingerprint of the defaults compared with.
    d: String,
    /// 0: compared with no defaults; 1: with defaults that hold nothing but collapsed containers; 2: with defaults that
    /// hold more, which a reader therefore needs.
    #[serde(default, skip_serializing_if = "is_zero")]
    r: u8,
    /// The id table: for each id the length in bytes it shares with the one before, then the rest of it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    t: Vec<Value>,
    #[serde(default, skip_serializing_if = "is_default")]
    c: Collapse,
    #[serde(default, skip_serializing_if = "is_default")]
    i: Changes,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    h: Option<CompactHighlight>,
    /// The selection: `[0, node]` or `[1, edge]`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    x: Option<(u8, usize)>,
}

/// A list of places in the id table, each as the difference from the one before.
type List = Vec<i64>;

/// The collapsed containers: the difference from the default ones (`o` opened, `a` also closed), or the whole list `l`.
#[derive(Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Collapse {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    o: List,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    a: List,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    l: Option<List>,
}

/// The difference of a map from its default: entries set, keys removed.
#[derive(PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MapChange<V> {
    #[serde(default = "Vec::new", skip_serializing_if = "Vec::is_empty")]
    s: Vec<(usize, V)>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    u: List,
}

impl<V> Default for MapChange<V> {
    fn default() -> Self {
        Self {
            s: Vec::new(),
            u: Vec::new(),
        }
    }
}

impl<V> MapChange<V> {
    fn is_empty(&self) -> bool {
        self.s.is_empty() && self.u.is_empty()
    }
}

/// The difference of a keyed list from its default: keys removed, entries replaced in place, entries added at the end.
#[derive(PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ListChange<V> {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    u: List,
    #[serde(default = "Vec::new", skip_serializing_if = "Vec::is_empty")]
    s: Vec<V>,
    #[serde(default = "Vec::new", skip_serializing_if = "Vec::is_empty")]
    a: Vec<V>,
}

impl<V> Default for ListChange<V> {
    fn default() -> Self {
        Self {
            u: Vec::new(),
            s: Vec::new(),
            a: Vec::new(),
        }
    }
}

impl<V> ListChange<V> {
    fn is_empty(&self) -> bool {
        self.u.is_empty() && self.s.is_empty() && self.a.is_empty()
    }
}

/// A port: id, node, side, offset, row.
type CompactPort = (usize, usize, PortSide, f64, Option<String>);
/// The ports of an edge's ends: edge, source port, target port.
type CompactEdgePorts = (usize, Option<usize>, Option<usize>);

#[derive(PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CompactHidden {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    k: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    n: List,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    e: Vec<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    c: bool,
}

/// Every field of [`Intent`] but the collapsed containers, as its difference from the defaults.
#[derive(Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Changes {
    /// pins
    #[serde(default, skip_serializing_if = "MapChange::is_empty")]
    p: MapChange<(f64, f64)>,
    /// containerSizing
    #[serde(default, skip_serializing_if = "MapChange::is_empty")]
    cs: MapChange<ContainerSizing>,
    /// nodeSizes
    #[serde(default, skip_serializing_if = "MapChange::is_empty")]
    ns: MapChange<(f64, f64)>,
    /// shapes
    #[serde(default, skip_serializing_if = "MapChange::is_empty")]
    sh: MapChange<NodeShape>,
    /// defaultDirection
    #[serde(default, skip_serializing_if = "Option::is_none")]
    dd: Option<DirectionConstraint>,
    /// containerDirections
    #[serde(default, skip_serializing_if = "MapChange::is_empty")]
    cd: MapChange<DirectionConstraint>,
    /// defaultStrategy
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ds: Option<StrategyConstraint>,
    /// containerStrategies
    #[serde(default, skip_serializing_if = "MapChange::is_empty")]
    ct: MapChange<StrategyConstraint>,
    /// defaultRouting: an empty list clears it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    dr: Option<Vec<RoutingConstraint>>,
    /// containerRouting
    #[serde(default, skip_serializing_if = "MapChange::is_empty")]
    cr: MapChange<RoutingConstraint>,
    /// ports
    #[serde(default, skip_serializing_if = "ListChange::is_empty")]
    po: ListChange<CompactPort>,
    /// edgePorts
    #[serde(default, skip_serializing_if = "ListChange::is_empty")]
    ep: ListChange<CompactEdgePorts>,
    /// labelPositions
    #[serde(default, skip_serializing_if = "MapChange::is_empty")]
    lp: MapChange<(f64, f64)>,
    /// orderHints: the sibling it follows.
    #[serde(default, skip_serializing_if = "MapChange::is_empty")]
    oh: MapChange<Option<usize>>,
    /// partial
    #[serde(default, skip_serializing_if = "MapChange::is_empty")]
    pa: MapChange<List>,
    /// hidden, whole when it differs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    hi: Option<CompactHidden>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CompactHighlight {
    /// A path: its two ends.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    t: Option<(usize, usize)>,
    /// A neighbourhood of one node, or of several.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    n: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    m: Option<List>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    q: Option<Value>,
}

// Writing.

/// The id table as it is built: each id once, in the order of first use.
#[derive(Default)]
struct Table<'a> {
    place: BTreeMap<&'a str, usize>,
    ids: Vec<&'a str>,
}

impl<'a> Table<'a> {
    fn id(&mut self, id: &'a str) -> usize {
        *self.place.entry(id).or_insert_with(|| {
            self.ids.push(id);
            self.ids.len() - 1
        })
    }

    fn list(&mut self, ids: impl IntoIterator<Item = &'a String>) -> List {
        let mut last = 0i64;
        ids.into_iter()
            .map(|id| {
                let at = self.id(id) as i64;
                let step = at - last;
                last = at;
                step
            })
            .collect()
    }

    fn encode(&self) -> Vec<Value> {
        let mut before = "";
        let mut out = Vec::with_capacity(self.ids.len() * 2);
        for id in &self.ids {
            let mut shared = before
                .bytes()
                .zip(id.bytes())
                .take_while(|(a, b)| a == b)
                .count();
            while !id.is_char_boundary(shared) {
                shared -= 1;
            }
            out.push(json!(shared));
            out.push(json!(&id[shared..]));
            before = id;
        }
        out
    }

    fn map<V: PartialEq, W>(
        &mut self,
        now: &'a BTreeMap<String, V>,
        base: &'a BTreeMap<String, V>,
        mut value: impl FnMut(&mut Self, &'a V) -> W,
    ) -> MapChange<W> {
        let mut s = Vec::new();
        for (id, v) in now {
            if base.get(id) != Some(v) {
                let at = self.id(id);
                s.push((at, value(self, v)));
            }
        }
        let u = self.list(base.keys().filter(|id| !now.contains_key(*id)));
        MapChange { s, u }
    }

    /// `now` as changes of `base` that give it back in its order: the longest start of `now` that follows the order of
    /// `base` stays in place, the rest is added at the end.
    fn keyed<V: PartialEq, W>(
        &mut self,
        now: &'a [V],
        base: &'a [V],
        key: impl Fn(&V) -> &String,
        mut value: impl FnMut(&mut Self, &'a V) -> W,
    ) -> ListChange<W> {
        let place: BTreeMap<&String, usize> =
            base.iter().enumerate().map(|(i, v)| (key(v), i)).collect();
        let (mut next, mut taken) = (0, 0);
        let mut removed = Vec::new();
        let mut replaced = Vec::new();
        for v in now {
            let Some(&at) = place.get(key(v)).filter(|&&at| at >= next) else {
                break;
            };
            removed.extend(base[next..at].iter().map(&key));
            if base[at] != *v {
                replaced.push(v);
            }
            next = at + 1;
            taken += 1;
        }
        removed.extend(base[next..].iter().map(&key));
        let u = self.list(removed);
        let s = replaced.into_iter().map(|v| value(self, v)).collect();
        let a = now[taken..].iter().map(|v| value(self, v)).collect();
        ListChange { u, s, a }
    }
}

fn changes<'a>(table: &mut Table<'a>, now: &'a Intent, base: &'a Intent) -> Changes {
    // Every field is named: a new field of `Intent` does not compile until it has a place here.
    let Intent {
        pins,
        container_sizing,
        node_sizes,
        collapsed_ids: _,
        shapes,
        default_direction,
        container_directions,
        default_strategy,
        container_strategies,
        default_routing,
        container_routing,
        ports,
        edge_ports,
        label_positions,
        order_hints,
        partial,
        hidden,
    } = now;
    let port = |t: &mut Table<'a>, p: &'a Port| {
        (
            t.id(&p.id),
            t.id(&p.node_id),
            p.side,
            p.offset,
            p.row.clone(),
        )
    };
    let ends = |t: &mut Table<'a>, e: &'a EdgePorts| {
        (
            t.id(&e.edge_id),
            e.source_port.as_deref().map(|id| t.id(id)),
            e.target_port.as_deref().map(|id| t.id(id)),
        )
    };
    Changes {
        pa: table.map(partial, &base.partial, |t, shown| t.list(shown)),
        p: table.map(pins, &base.pins, |_, p| (p.x, p.y)),
        cs: table.map(container_sizing, &base.container_sizing, |_, v| v.clone()),
        ns: table.map(node_sizes, &base.node_sizes, |_, z| (z.width, z.height)),
        sh: table.map(shapes, &base.shapes, |_, v| *v),
        dd: (*default_direction != base.default_direction).then(|| default_direction.clone()),
        cd: table.map(container_directions, &base.container_directions, |_, v| {
            v.clone()
        }),
        ds: (*default_strategy != base.default_strategy).then(|| default_strategy.clone()),
        ct: table.map(container_strategies, &base.container_strategies, |_, v| {
            v.clone()
        }),
        dr: (*default_routing != base.default_routing)
            .then(|| default_routing.iter().cloned().collect()),
        cr: table.map(container_routing, &base.container_routing, |_, v| v.clone()),
        po: table.keyed(ports, &base.ports, |p| &p.id, port),
        ep: table.keyed(edge_ports, &base.edge_ports, |e| &e.edge_id, ends),
        lp: table.map(label_positions, &base.label_positions, |_, p| (p.x, p.y)),
        oh: table.map(order_hints, &base.order_hints, |t, h| {
            h.after.as_deref().map(|id| t.id(id))
        }),
        hi: (*hidden != base.hidden).then(|| CompactHidden {
            k: hidden.node_kinds.clone(),
            n: table.list(&hidden.nodes),
            e: hidden.edge_kinds.clone(),
            c: hidden.compact,
        }),
    }
}

/// The fingerprint of a source graph without the fields a decision writes into it (shapes, directions, strategies,
/// routing, ports), so that a view that changed them still belongs to the same source.
pub fn source_fingerprint(source: &Graph) -> String {
    let mut graph = source.clone();
    for node in &mut graph.nodes {
        node.shape = NodeShape::default();
    }
    graph.default_direction = Default::default();
    graph.container_directions = Default::default();
    graph.default_strategy = Default::default();
    graph.container_strategies = Default::default();
    graph.default_routing = Default::default();
    graph.container_routing = Default::default();
    graph.ports = Default::default();
    graph.edge_ports = Default::default();
    arqlet_model::source_digest(&graph)
}

/// FNV-1a (64 bits) of the defaults compared with: the collapsed containers when the difference from them is kept, and
/// the caller's other defaults.
fn defaults_fingerprint(collapsed: Option<&BTreeSet<String>>, base: Option<&Intent>) -> String {
    // No defaults and empty ones are the same.
    let base = base.map_or_else(Intent::default, |b| Intent {
        collapsed_ids: Vec::new(),
        ..b.clone()
    });
    let text = serde_json::to_string(&(collapsed, base)).expect("defaults serialize");
    let hash = text.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
    });
    format!("{hash:016x}")
}

/// The compact text of the snapshot's decisions with `marks`, compared with `defaults` (the intent of the view as it
/// was imported) when given.
pub fn compact(
    snapshot: &Snapshot,
    defaults: Option<&Intent>,
    marks: &Marks,
) -> Result<String, String> {
    let intent = crate::history::intent_of(snapshot);
    let source = snapshot.source_graph.as_deref().unwrap_or(&snapshot.graph);
    let (_, default_collapsed) = crate::default_view(snapshot)?;
    let now: BTreeSet<&String> = intent.collapsed_ids.iter().collect();
    let opened: Vec<&String> = default_collapsed
        .iter()
        .filter(|id| !now.contains(id))
        .collect();
    let closed: Vec<&String> = now
        .iter()
        .copied()
        .filter(|id| !default_collapsed.contains(*id))
        .collect();
    let whole = now.len() < opened.len() + closed.len();

    let empty = Intent::default();
    let base = defaults.unwrap_or(&empty);
    let mut table = Table::default();
    let c = if whole {
        Collapse {
            l: Some(table.list(now.iter().copied())),
            ..Collapse::default()
        }
    } else {
        Collapse {
            o: table.list(opened),
            a: table.list(closed),
            l: None,
        }
    };
    let i = changes(&mut table, &intent, base);
    let h = marks.highlight.as_ref().map(|h| match h {
        Highlight::Trace { from, to, init } => CompactHighlight {
            t: Some((table.id(from), table.id(to))),
            n: None,
            m: None,
            q: init.clone(),
        },
        Highlight::Neighbourhood { node_id, init } => CompactHighlight {
            t: None,
            n: match node_id {
                NodeIds::One(id) => Some(table.id(id)),
                NodeIds::Many(_) => None,
            },
            m: match node_id {
                NodeIds::One(_) => None,
                NodeIds::Many(ids) => Some(table.list(ids)),
            },
            q: init.clone(),
        },
    });
    let x = marks.selection.as_ref().map(|s| match s {
        Selection::Node(id) => (0, table.id(id)),
        Selection::Edge(id) => (1, table.id(id)),
    });
    let relative = match defaults {
        None => 0,
        Some(d) => {
            let bare = Intent {
                collapsed_ids: Vec::new(),
                ..d.clone()
            };
            if bare == empty {
                1
            } else {
                2
            }
        }
    };
    let document = Compact {
        s: source_fingerprint(source),
        d: defaults_fingerprint((!whole).then_some(&default_collapsed), defaults),
        r: relative,
        t: table.encode(),
        c,
        i,
        h,
        x,
    };
    let bytes = serde_json::to_vec(&document).map_err(|e| e.to_string())?;
    let text = format!("{PREFIX}{}", base64url(&bytes));
    if text.len() > MAX_TEXT {
        return Err(format!(
            "The view's decisions make a bookmark of {} bytes, more than {MAX_TEXT}",
            text.len()
        ));
    }
    Ok(text)
}

// Reading.

fn damaged(what: &str) -> String {
    format!("The compact bookmark is damaged: {what}")
}

/// The id table as it is read: every use of an id counts against `MAX_ID_BYTES`.
struct IdTable {
    ids: Vec<String>,
    left: usize,
}

impl IdTable {
    fn decode(table: &[Value]) -> Result<Self, String> {
        let pairs = table.chunks_exact(2);
        if !pairs.remainder().is_empty() {
            return Err(damaged("the id table"));
        }
        let mut left = MAX_ID_BYTES;
        let mut ids: Vec<String> = Vec::with_capacity(table.len() / 2);
        for pair in pairs {
            let before = ids.last().map_or("", String::as_str);
            let shared = pair[0]
                .as_u64()
                .and_then(|n| usize::try_from(n).ok())
                .and_then(|n| before.get(..n));
            let (Some(shared), Some(rest)) = (shared, pair[1].as_str()) else {
                return Err(damaged("the id table"));
            };
            left = left
                .checked_sub(shared.len() + rest.len())
                .ok_or_else(|| damaged("too many ids"))?;
            let id = format!("{shared}{rest}");
            ids.push(id);
        }
        Ok(Self { ids, left })
    }

    fn id(&mut self, at: usize) -> Result<String, String> {
        let id = self
            .ids
            .get(at)
            .ok_or_else(|| damaged("an unknown place"))?;
        self.left = self
            .left
            .checked_sub(id.len())
            .ok_or_else(|| damaged("too many ids"))?;
        Ok(id.clone())
    }

    fn list(&mut self, steps: &[i64]) -> Result<Vec<String>, String> {
        let mut at = 0i64;
        steps
            .iter()
            .map(|step| {
                at = at
                    .checked_add(*step)
                    .ok_or_else(|| damaged("an unknown place"))?;
                self.id(usize::try_from(at).map_err(|_| damaged("an unknown place"))?)
            })
            .collect()
    }

    /// Applies a map's change to `map`; the keys it removes are noted in `removed` under `field`.
    fn map<V, W>(
        &mut self,
        map: &mut BTreeMap<String, V>,
        change: MapChange<W>,
        field: &'static str,
        removed: &mut Removed,
        mut value: impl FnMut(&mut Self, W) -> Result<V, String>,
    ) -> Result<(), String> {
        for id in self.list(&change.u)? {
            map.remove(&id);
            removed.push((field, id));
        }
        for (at, v) in change.s {
            let id = self.id(at)?;
            let v = value(self, v)?;
            map.insert(id, v);
        }
        Ok(())
    }

    fn keyed<V, W>(
        &mut self,
        list: &mut Vec<V>,
        change: ListChange<W>,
        key: impl Fn(&V) -> &String,
        mut value: impl FnMut(&mut Self, W) -> Result<V, String>,
    ) -> Result<(), String> {
        let gone: BTreeSet<String> = self.list(&change.u)?.into_iter().collect();
        list.retain(|v| !gone.contains(key(v)));
        let place: BTreeMap<String, usize> = list
            .iter()
            .enumerate()
            .map(|(i, v)| (key(v).clone(), i))
            .collect();
        for v in change.s {
            let v = value(self, v)?;
            // An entry the defaults no longer have is added.
            match place.get(key(&v)) {
                Some(&at) => list[at] = v,
                None => list.push(v),
            }
        }
        for v in change.a {
            let v = value(self, v)?;
            list.retain(|old| key(old) != key(&v));
            list.push(v);
        }
        Ok(())
    }
}

/// The ids a bookmark names to take away from the defaults, each with its field: nothing to apply when one is gone, but
/// the bookmark still names it.
type Removed = Vec<(&'static str, String)>;

fn apply(
    ids: &mut IdTable,
    intent: &mut Intent,
    changes: Changes,
    removed: &mut Removed,
) -> Result<(), String> {
    let Changes {
        p,
        cs,
        ns,
        sh,
        dd,
        cd,
        ds,
        ct,
        dr,
        cr,
        po,
        ep,
        lp,
        oh,
        pa,
        hi,
    } = changes;
    let point = |_: &mut IdTable, (x, y): (f64, f64)| Ok(Point { x, y });
    ids.map(&mut intent.pins, p, "pins", removed, point)?;
    ids.map(
        &mut intent.container_sizing,
        cs,
        "containerSizing",
        removed,
        |_, v| Ok(v),
    )?;
    ids.map(
        &mut intent.node_sizes,
        ns,
        "nodeSizes",
        removed,
        |_, (width, height)| Ok(NodeSize { width, height }),
    )?;
    ids.map(&mut intent.shapes, sh, "shapes", removed, |_, v| Ok(v))?;
    if let Some(v) = dd {
        intent.default_direction = v;
    }
    ids.map(
        &mut intent.container_directions,
        cd,
        "containerDirections",
        removed,
        |_, v| Ok(v),
    )?;
    if let Some(v) = ds {
        intent.default_strategy = v;
    }
    ids.map(
        &mut intent.container_strategies,
        ct,
        "containerStrategies",
        removed,
        |_, v| Ok(v),
    )?;
    if let Some(v) = dr {
        intent.default_routing = v.into_iter().next();
    }
    ids.map(
        &mut intent.container_routing,
        cr,
        "containerRouting",
        removed,
        |_, v| Ok(v),
    )?;
    // A removed port or binding is not reported: the defaults may simply not declare it any more.
    ids.keyed(
        &mut intent.ports,
        po,
        |p| &p.id,
        |ids, (id, node, side, offset, row)| {
            Ok(Port {
                id: ids.id(id)?,
                node_id: ids.id(node)?,
                side,
                offset,
                row,
            })
        },
    )?;
    ids.keyed(
        &mut intent.edge_ports,
        ep,
        |e| &e.edge_id,
        |ids, (edge, source, target)| {
            Ok(EdgePorts {
                edge_id: ids.id(edge)?,
                source_port: source.map(|at| ids.id(at)).transpose()?,
                target_port: target.map(|at| ids.id(at)).transpose()?,
            })
        },
    )?;
    // The labels' removed positions are not reported either: their ids are made by the layout.
    ids.map(
        &mut intent.label_positions,
        lp,
        "labelPositions",
        &mut Vec::new(),
        point,
    )?;
    ids.map(
        &mut intent.order_hints,
        oh,
        "orderHints",
        removed,
        |ids, after| {
            Ok(OrderHint {
                after: after.map(|at| ids.id(at)).transpose()?,
            })
        },
    )?;
    ids.map(&mut intent.partial, pa, "partial", removed, |ids, shown| {
        ids.list(&shown)
    })?;
    if let Some(h) = hi {
        intent.hidden = Hidden {
            node_kinds: h.k,
            nodes: ids.list(&h.n)?,
            edge_kinds: h.e,
            compact: h.c,
        };
    }
    Ok(())
}

/// Reads a compact bookmark against the snapshot. `defaults` is the intent of this view as it was imported: needed when
/// the bookmark was compared with defaults that hold more than collapsed containers.
pub fn read(snapshot: &Snapshot, defaults: Option<&Intent>, text: &str) -> Result<Reading, String> {
    if text.len() > MAX_TEXT {
        return Err(damaged("it is too long"));
    }
    let Some(body) = text.strip_prefix(PREFIX) else {
        let version = text
            .strip_prefix('b')
            .and_then(|rest| rest.split_once('.'))
            .map(|(v, _)| v)
            .filter(|v| !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit()));
        return Err(match version {
            Some(v) => format!("Compact bookmark version {v} is not supported"),
            None => "Not a compact bookmark".into(),
        });
    };
    let bytes = from_base64url(body).ok_or_else(|| damaged("not base64url"))?;
    let document: Compact = serde_json::from_slice(&bytes).map_err(|e| damaged(&e.to_string()))?;
    if document.r == 2 && defaults.is_none() {
        return Err(
            "The bookmark is relative to the view's defaults: send the intent of the view as imported as defaults"
                .into(),
        );
    }
    let source = snapshot.source_graph.as_deref().unwrap_or(&snapshot.graph);
    let (view, default_collapsed) = crate::default_view(snapshot)?;
    let mut ids = IdTable::decode(&document.t)?;
    // A bookmark compared with nothing holds its decisions whole: the defaults would add to them.
    let base = defaults.filter(|_| document.r != 0);
    let whole = document.c.l.is_some();
    let same_defaults =
        defaults_fingerprint((!whole).then_some(&default_collapsed), base) == document.d;
    let mut intent = base.cloned().unwrap_or_default();
    let mut removed = Removed::new();
    let collapsed: BTreeSet<String> = match &document.c.l {
        Some(list) => ids.list(list)?.into_iter().collect(),
        None => {
            let opened = ids.list(&document.c.o)?;
            let mut collapsed = default_collapsed;
            for id in &opened {
                collapsed.remove(id);
            }
            removed.extend(opened.into_iter().map(|id| ("collapsed", id)));
            collapsed.extend(ids.list(&document.c.a)?);
            collapsed
        }
    };
    intent.collapsed_ids = collapsed.into_iter().collect();
    apply(&mut ids, &mut intent, document.i, &mut removed)?;
    let highlight = match document.h {
        None => None,
        Some(h) => Some(match (h.t, h.n, h.m) {
            (Some((from, to)), None, None) => Highlight::Trace {
                from: ids.id(from)?,
                to: ids.id(to)?,
                init: h.q,
            },
            (None, Some(at), None) => Highlight::Neighbourhood {
                node_id: NodeIds::One(ids.id(at)?),
                init: h.q,
            },
            (None, None, Some(list)) => Highlight::Neighbourhood {
                node_id: NodeIds::Many(ids.list(&list)?),
                init: h.q,
            },
            _ => return Err(damaged("the highlight")),
        }),
    };
    let selection = match document.x {
        None => None,
        Some((0, at)) => Some(Selection::Node(ids.id(at)?)),
        Some((1, at)) => Some(Selection::Edge(ids.id(at)?)),
        Some(_) => return Err(damaged("the selection")),
    };
    let marks = Marks {
        highlight,
        selection,
    };
    Ok(Reading {
        same_source: source_fingerprint(source) == document.s,
        same_defaults,
        ..fit(source, &view, intent, marks, removed)
    })
}

/// Collects what does not fit.
#[derive(Default)]
struct Report {
    missing: BTreeSet<String>,
    ignored: BTreeSet<Ignored>,
}

impl Report {
    /// Whether `fits`; when not, the decision of `field` for `id` is ignored.
    fn check(&mut self, field: &'static str, id: &str, fits: bool) -> bool {
        if !fits {
            self.ignored.insert(Ignored {
                field,
                id: id.into(),
            });
        }
        fits
    }

    /// Whether the graph has `id` (`known`); when not it is missing and the decision ignored.
    fn known(&mut self, field: &'static str, id: &str, known: bool) -> bool {
        if !known {
            self.missing.insert(id.into());
        }
        self.check(field, id, known)
    }
}

/// The decisions of `intent` and `marks` that the snapshot's graph can take, and what was left out.
fn fit(
    source: &Graph,
    view: &Graph,
    mut intent: Intent,
    marks: Marks,
    removed: Removed,
) -> Reading {
    let index = GraphIndex::of(source);
    let view_index = GraphIndex::of(view);
    // A node of the source graph, or one the view adds (a community, a group).
    let real = |id: &str| index.node.contains_key(id);
    let shown = |id: &str| view_index.node.contains_key(id) || real(id);
    let container = |id: &str| index.node.get(id).is_some_and(|&i| index.container[i]);
    let view_container = |id: &str| {
        view_index
            .node
            .get(id)
            .is_some_and(|&i| view_index.container[i])
    };
    let parent_in_view = |id: &str| {
        view_index
            .node
            .get(id)
            .and_then(|&i| view_index.parent[i])
            .map(|p| view.nodes[p].id.as_str())
    };
    let parent = |id: &str| {
        index
            .node
            .get(id)
            .and_then(|&i| source.nodes[i].parent_id.as_deref())
    };
    let edges: BTreeMap<&str, usize> = source
        .edges
        .iter()
        .enumerate()
        .map(|(i, e)| (e.id.as_str(), i))
        .collect();
    let bus = |id: &str| source.buses.iter().any(|b| b.id == id);
    let mut report = Report::default();

    for (field, id) in &removed {
        // A rest stands for a container shown in part.
        let id = id.strip_prefix(REST).filter(|_| !shown(id)).unwrap_or(id);
        report.known(field, id, shown(id));
    }

    intent.partial.retain(|id, children| {
        if !report.known("partial", id, shown(id))
            || !report.check("partial", id, view_container(id))
        {
            return false;
        }
        let had = !children.is_empty();
        children.retain(|child| {
            report.known("partial", child, shown(child))
                && report.check("partial", child, parent_in_view(child) == Some(id.as_str()))
        });
        !had || !children.is_empty()
    });
    let partial = &intent.partial;
    intent.collapsed_ids.retain(|id| {
        if view_container(id) {
            return true;
        }
        match id.strip_prefix(REST).filter(|_| !shown(id)) {
            Some(of) => {
                report.known("collapsed", of, shown(of))
                    && report.check("collapsed", id, partial.contains_key(of))
            }
            None => {
                report.known("collapsed", id, shown(id));
                report.check("collapsed", id, false)
            }
        }
    });
    intent
        .pins
        .retain(|id, _| report.known("pins", id, real(id)));
    intent
        .node_sizes
        .retain(|id, _| report.known("nodeSizes", id, real(id)));
    intent
        .shapes
        .retain(|id, _| report.known("shapes", id, real(id)));
    intent.container_sizing.retain(|id, _| {
        report.known("containerSizing", id, real(id))
            && report.check("containerSizing", id, container(id))
    });
    intent.container_directions.retain(|id, _| {
        report.known("containerDirections", id, real(id))
            && report.check("containerDirections", id, container(id))
    });
    intent.container_strategies.retain(|id, _| {
        report.known("containerStrategies", id, real(id))
            && report.check("containerStrategies", id, container(id))
    });
    intent.container_routing.retain(|id, _| {
        report.known("containerRouting", id, real(id))
            && report.check("containerRouting", id, container(id))
    });
    intent.order_hints.retain(|id, hint| {
        if !report.known("orderHints", id, real(id)) {
            return false;
        }
        match hint.after.as_deref() {
            None => true,
            Some(after) => {
                report.known("orderHints", after, real(after))
                    && report.check("orderHints", id, after != id && parent(after) == parent(id))
            }
        }
    });
    intent
        .ports
        .retain(|p| report.known("ports", &p.node_id, real(&p.node_id)));
    let ports: BTreeMap<String, String> = intent
        .ports
        .iter()
        .map(|p| (p.id.clone(), p.node_id.clone()))
        .collect();
    intent.edge_ports.retain_mut(|binding| {
        let edge = edges.get(binding.edge_id.as_str());
        if !report.known("edgePorts", &binding.edge_id, edge.is_some()) {
            return false;
        }
        let edge = &source.edges[*edge.expect("known")];
        let had = binding.source_port.is_some() || binding.target_port.is_some();
        for (end, node) in [
            (&mut binding.source_port, Some(&edge.source)),
            (&mut binding.target_port, edge.target.as_ref()),
        ] {
            if let Some(port) = end.clone() {
                if !report.check("edgePorts", &port, ports.get(&port) == node) {
                    *end = None;
                }
            }
        }
        !had || binding.source_port.is_some() || binding.target_port.is_some()
    });
    // A label belongs to an edge, a bus or a port, declared or made by the layout for an end of an edge or a fan.
    intent.label_positions.retain(|id, _| {
        let id = id.as_str();
        let end = id
            .strip_suffix(":source")
            .or_else(|| id.strip_suffix(":target"));
        let fan = id
            .strip_prefix("fan:source:")
            .or_else(|| id.strip_prefix("fan:target:"));
        let known = edges.contains_key(id)
            || bus(id)
            || ports.contains_key(id)
            || end.is_some_and(|e| edges.contains_key(e.strip_prefix("auto:").unwrap_or(e)))
            || fan.is_some_and(shown);
        report.known("labelPositions", id, known)
    });
    intent
        .hidden
        .nodes
        .retain(|id| report.known("hidden", id, shown(id) || bus(id)));

    let highlight = marks.highlight.and_then(|h| match h {
        Highlight::Trace { from, to, init } => {
            let ends = [&from, &to].map(|id| report.known("highlight", id, shown(id)));
            (ends == [true, true]).then_some(Highlight::Trace { from, to, init })
        }
        Highlight::Neighbourhood {
            node_id: NodeIds::One(id),
            init,
        } => report
            .known("highlight", &id, shown(&id))
            .then_some(Highlight::Neighbourhood {
                node_id: NodeIds::One(id),
                init,
            }),
        Highlight::Neighbourhood {
            node_id: NodeIds::Many(mut list),
            init,
        } => {
            list.retain(|id| report.known("highlight", id, shown(id)));
            (!list.is_empty()).then_some(Highlight::Neighbourhood {
                node_id: NodeIds::Many(list),
                init,
            })
        }
    });
    let selection = marks.selection.filter(|s| match s {
        Selection::Node(id) => report.known("selection", id, shown(id) || bus(id)),
        Selection::Edge(id) => {
            report.known("selection", id, edges.contains_key(id.as_str()) || bus(id))
        }
    });
    Reading {
        intent,
        marks: Marks {
            highlight,
            selection,
        },
        same_source: true,
        same_defaults: true,
        missing: report.missing.into_iter().collect(),
        ignored: report.ignored.into_iter().collect(),
    }
}

// The operations.

/// The JSON form of a bookmark (`ViewBookmark` of the engine client).
#[derive(Deserialize)]
struct JsonForm {
    version: u64,
    #[serde(default)]
    digest: Option<String>,
    intent: Intent,
    #[serde(default)]
    highlight: Option<Highlight>,
    #[serde(default)]
    selection: Option<Selection>,
}

/// An optional field of a request.
fn field<T: serde::de::DeserializeOwned>(v: &Value, name: &str) -> Result<Option<T>, String> {
    match v.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => T::deserialize(value)
            .map(Some)
            .map_err(|e| format!("{name}: {e}")),
    }
}

/// `compactBookmark` (read only): `{highlight?, selection?, defaults?}` answered with `{text}`.
pub fn compact_on(snapshot: &Snapshot, v: &Value) -> Result<Value, String> {
    let marks = Marks {
        highlight: field(v, "highlight")?,
        selection: field(v, "selection")?,
    };
    let defaults: Option<Intent> = field(v, "defaults")?;
    let text = compact(snapshot, defaults.as_ref(), &marks)?;
    Ok(json!({"ok": true, "text": text}))
}

/// `readBookmark` (read only): a compact bookmark (`text`) or one in the JSON form (`bookmark`) read against the
/// snapshot: `{bookmark, sameSource, sameDefaults, missing, ignored}`, the bookmark in the JSON form, holding what this
/// graph can take.
pub fn read_on(snapshot: &Snapshot, v: &Value) -> Result<Value, String> {
    let reading = if let Some(text) = field::<String>(v, "text")? {
        let defaults: Option<Intent> = field(v, "defaults")?;
        read(snapshot, defaults.as_ref(), &text)?
    } else {
        let form: JsonForm = field(v, "bookmark")?.ok_or("Missing text or bookmark")?;
        if form.version != 1 {
            return Err(format!(
                "Bookmark version {} is not supported",
                form.version
            ));
        }
        let marks = Marks {
            highlight: form.highlight,
            selection: form.selection,
        };
        let same_source = form
            .digest
            .is_none_or(|d| crate::find::digest_on(snapshot)["digest"] == d.as_str());
        let source = snapshot.source_graph.as_deref().unwrap_or(&snapshot.graph);
        let (view, _) = crate::default_view(snapshot)?;
        Reading {
            same_source,
            ..fit(source, &view, form.intent, marks, Removed::new())
        }
    };
    // Without a digest: it fits this graph now.
    let mut bookmark = json!({"version": 1, "digest": null, "intent": reading.intent});
    if let Some(highlight) = &reading.marks.highlight {
        bookmark["highlight"] = json!(highlight);
    }
    if let Some(selection) = &reading.marks.selection {
        bookmark["selection"] = json!(selection);
    }
    Ok(
        json!({"ok": true, "bookmark": bookmark, "sameSource": reading.same_source,
        "sameDefaults": reading.same_defaults, "missing": reading.missing, "ignored": reading.ignored}),
    )
}

/// `compactBookmark` and `readBookmark` of a request that carries its snapshot.
pub fn bookmark_response(v: &Value) -> Value {
    let answer = Snapshot::deserialize(&v["snapshot"])
        .map_err(|e| e.to_string())
        .and_then(|snapshot| match v["op"].as_str() {
            Some("compactBookmark") => compact_on(&snapshot, v),
            _ => read_on(&snapshot, v),
        });
    answer.unwrap_or_else(|error| json!({"ok": false, "error": error}))
}

// base64url (RFC 4648, section 5) without padding.

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

fn base64url(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() / 3 * 4 + 4);
    for chunk in bytes.chunks(3) {
        let mut group = 0u32;
        for i in 0..3 {
            group = (group << 8) | u32::from(*chunk.get(i).unwrap_or(&0));
        }
        for i in 0..=chunk.len() {
            out.push(char::from(ALPHABET[(group >> (18 - 6 * i)) as usize & 63]));
        }
    }
    out
}

fn from_base64url(text: &str) -> Option<Vec<u8>> {
    if text.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(text.len() / 4 * 3 + 3);
    for chunk in text.as_bytes().chunks(4) {
        let mut group = 0u32;
        for i in 0..4 {
            let six = match chunk.get(i) {
                Some(c) => ALPHABET.iter().position(|a| a == c)? as u32,
                None => 0,
            };
            group = (group << 6) | six;
        }
        for i in 0..chunk.len() - 1 {
            out.push((group >> (16 - 8 * i)) as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64url_round_trips_every_length() {
        assert_eq!(base64url(b"fo"), "Zm8");
        assert_eq!(base64url(&[0xfb, 0xff]), "-_8");
        let bytes: Vec<u8> = (0..=255).collect();
        for n in 0..bytes.len() {
            assert_eq!(
                from_base64url(&base64url(&bytes[..n])).unwrap(),
                &bytes[..n]
            );
        }
        assert!(from_base64url("a").is_none());
        assert!(from_base64url("ab=c").is_none());
    }

    #[test]
    fn the_id_table_shares_beginnings_at_character_boundaries() {
        let ids = ["área/one", "área/two", "è", "é", "é/x", ""].map(String::from);
        let mut table = Table::default();
        let steps = table.list(&ids);
        assert_eq!(steps, [0, 1, 1, 1, 1, 1]);
        let encoded = table.encode();
        assert_eq!(encoded[2..4], [json!("área/".len()), json!("two")]);
        // `è` and `é` share their first byte, not a character.
        assert_eq!(encoded[6..8], [json!(0), json!("é")]);
        let mut read = IdTable::decode(&encoded).unwrap();
        assert_eq!(read.list(&steps).unwrap(), ids);
        assert!(read.list(&[6]).is_err());
        assert!(read.list(&[-1]).is_err());
        assert!(IdTable::decode(&[json!(1), json!("x")]).is_err());
    }

    #[test]
    fn a_short_table_cannot_expand_without_bound() {
        let long = "x".repeat(1 << 16);
        let mut table = vec![json!(0), json!(long)];
        for _ in 0..1000 {
            table.extend([json!(1 << 16), json!("")]);
        }
        assert!(IdTable::decode(&table)
            .err()
            .unwrap()
            .contains("too many ids"));
    }

    #[test]
    fn a_keyed_list_keeps_its_order() {
        let list = |s: &str| -> Vec<(String, u8)> {
            s.split_whitespace()
                .map(|w| (w[..1].to_string(), w.len() as u8))
                .collect()
        };
        let base = list("a b c d");
        for now in [
            "a b c d",
            "a bb c d",
            "a c d bb",
            "b c d a",
            "d c b a",
            "e a b c d",
            "a b c d e",
            "",
        ] {
            let now = list(now);
            let mut table = Table::default();
            let change = table.keyed(&now, &base, |v| &v.0, |t, v| (t.id(&v.0), v.1));
            let mut ids = IdTable::decode(&table.encode()).unwrap();
            let mut got = base.clone();
            ids.keyed(
                &mut got,
                change,
                |v| &v.0,
                |ids, (at, n)| Ok((ids.id(at)?, n)),
            )
            .unwrap();
            assert_eq!(got, now);
        }
    }
}
