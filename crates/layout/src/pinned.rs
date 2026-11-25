//! Experimental (`pinsInSinglePass`, ADR 0016, amendment *Pins in single pass*, step P1): under single pass and in the
//! hierarchical pipeline it falls back to, a pinned leaf of a layered area is a constraint of the arrangement instead of a
//! box written over it. The pin gives the node its rank
//! (the rank whose band holds the pin's middle along the flow, or the nearest, in a drawing to start from), its order in
//! that rank and its place across the flow, which the placement holds. Along the flow the node stands in its rank's band;
//! the difference to the pin is reported. The layout itself runs without the pin, so every route is constructed and buses
//! are drawn as buses. Containers (step P2), regions of other strategies and reversed flows keep what they did.
use arqlet_model::{Graph, LayoutOptions, Mode, Point, Rect, Snapshot};
use std::collections::{BTreeMap, BTreeSet};

const ROOT: &str = "<root>";
const EPS: f64 = 1e-9;

/// What a pin asks of its node's area.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Place {
    /// The pin: the node's top-left corner in its parent's frame.
    pub at: Point,
    /// The siblings in the band the pin names, those without an edge to the node first: the node takes the first one's rank.
    pub anchors: Vec<String>,
    /// The siblings in the bands before and after it, which say where the node stands when its band holds no other.
    pub earlier: Vec<String>,
    pub later: Vec<String>,
}

pub(crate) type Places = BTreeMap<String, Place>;

/// Whether the option applies to these pins: the layered areas are ordered hierarchically (single pass and the pipeline it
/// falls back to), and every pin is a leaf of an area that flows left to right or top to bottom (or chooses its
/// direction). Otherwise the pins are handled as without the option.
pub(crate) fn applies(graph: &Graph, options: &LayoutOptions) -> bool {
    if !options.pins_in_single_pass
        || !options.layered_construction
        || !options.hierarchical_ordering
        || options.global_layering
        || options.visible_node_budget != 0
        || options.pins.is_empty()
    {
        return false;
    }
    let containers: BTreeSet<&str> = graph
        .nodes
        .iter()
        .filter_map(|n| n.parent_id.as_deref())
        .collect();
    let parents: BTreeMap<&str, Option<&str>> = graph
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.parent_id.as_deref()))
        .collect();
    options.pins.keys().all(|id| {
        !containers.contains(id.as_str())
            && parents
                .get(id.as_str())
                .is_some_and(|parent| match graph.area_mode(*parent) {
                    Ok(Mode::Orientation(o)) => !o.reversed(),
                    Ok(Mode::Chooser(_)) => true,
                    _ => false,
                })
    })
}

fn area_of<'a>(graph: &'a Graph, node: &str) -> Option<&'a str> {
    let node = graph.nodes.iter().find(|n| n.id == node)?;
    Some(node.parent_id.as_deref().unwrap_or(ROOT))
}

/// Whether a drawing can say where the pins' bands are: it has a box and a rank for every pinned node.
pub(crate) fn covers(drawing: &Snapshot, graph: &Graph, pins: &BTreeMap<String, Point>) -> bool {
    pins.keys().all(|id| {
        drawing.boxes.contains_key(id)
            && area_of(graph, id)
                .and_then(|area| drawing.layered_plans.get(area))
                .is_some_and(|plan| plan.ranks.iter().any(|rank| rank.contains(id)))
    })
}

/// The child of `area` that holds `node` (the node itself or an ancestor).
fn sibling<'a>(
    parents: &BTreeMap<&'a str, Option<&'a str>>,
    area: &str,
    node: &str,
) -> Option<&'a str> {
    let (&id, &parent) = parents.get_key_value(node)?;
    if parent.unwrap_or(ROOT) == area {
        Some(id)
    } else {
        sibling(parents, area, parent?)
    }
}

/// The places the pins ask for, from the bands of the ranks of `reference` (see `covers`).
pub(crate) fn places(
    graph: &Graph,
    reference: &Snapshot,
    pins: &BTreeMap<String, Point>,
) -> Places {
    let parents: BTreeMap<&str, Option<&str>> = graph
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.parent_id.as_deref()))
        .collect();
    let mut out = Places::new();
    for (id, pin) in pins {
        let Some(area) = area_of(graph, id) else {
            continue;
        };
        let (Some(plan), Some(own)) = (
            reference.layered_plans.get(area),
            reference.boxes.get(id.as_str()),
        ) else {
            continue;
        };
        let along = |r: &Rect| {
            if plan.vertical {
                (r.y, r.y + r.height)
            } else {
                (r.x, r.x + r.width)
            }
        };
        // The ranks that hold siblings (not only virtual elements), each with its band along the flow.
        let bands: Vec<(Vec<&str>, f64, f64)> = plan
            .ranks
            .iter()
            .filter_map(|rank| {
                let members: Vec<&str> = rank
                    .iter()
                    .map(String::as_str)
                    .filter(|m| {
                        parents.get(m).is_some_and(|p| p.unwrap_or(ROOT) == area)
                            && reference.boxes.contains_key(*m)
                    })
                    .collect();
                let (low, high) = members
                    .iter()
                    .map(|m| along(&reference.boxes[*m]))
                    .fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), (a, b)| {
                        (l.min(a), h.max(b))
                    });
                (!members.is_empty()).then_some((members, low, high))
            })
            .collect();
        let middle = if plan.vertical {
            pin.y + own.height / 2.
        } else {
            pin.x + own.width / 2.
        };
        let distance = |low: f64, high: f64| (low - middle).max(middle - high).max(0.);
        let Some(k) = (0..bands.len()).min_by(|&a, &b| {
            distance(bands[a].1, bands[a].2)
                .total_cmp(&distance(bands[b].1, bands[b].2))
                .then(a.cmp(&b))
        }) else {
            continue;
        };
        // The siblings that hold an end of an edge of the node.
        let linked: BTreeSet<&str> = graph
            .edges
            .iter()
            .filter_map(|e| {
                let target = e.target.as_deref()?;
                if e.source == *id {
                    sibling(&parents, area, target)
                } else if target == id {
                    sibling(&parents, area, &e.source)
                } else {
                    None
                }
            })
            .collect();
        let others = |range: &[(Vec<&str>, f64, f64)]| -> Vec<String> {
            range
                .iter()
                .flat_map(|(members, _, _)| members)
                .filter(|m| **m != id.as_str())
                .map(|m| m.to_string())
                .collect()
        };
        let mut anchors = others(&bands[k..=k]);
        anchors.sort_by_key(|a| (linked.contains(a.as_str()), a.clone()));
        out.insert(
            id.clone(),
            Place {
                at: pin.clone(),
                anchors,
                earlier: others(&bands[..k]),
                later: others(&bands[k + 1..]),
            },
        );
    }
    out
}

/// The ranks of an area with its pinned children in the ranks their pins name. Every other child keeps its rank unless a
/// link needs it further on: a link spans at least one rank in either direction, and the caller reverses the links that
/// now run against the flow, as those of a cycle.
pub(crate) fn rank(
    ranks: &mut BTreeMap<String, usize>,
    links: &BTreeSet<(String, String)>,
    places: &Places,
) {
    if !ranks.keys().any(|id| places.contains_key(id)) {
        return;
    }
    let targets: BTreeMap<&String, usize> = ranks
        .iter()
        .filter_map(|(id, &base)| {
            let place = places.get(id)?;
            let of = |ids: &[String]| -> Vec<usize> {
                ids.iter().filter_map(|x| ranks.get(x).copied()).collect()
            };
            let target = match place.anchors.iter().find_map(|a| ranks.get(a)) {
                Some(&anchor) => anchor,
                // Alone in its band: where it stood, if that is still between the bands around it, else just after those
                // before it.
                None => {
                    let (earlier, later) = (of(&place.earlier), of(&place.later));
                    let low = earlier.iter().max().map_or(0, |r| r + 1);
                    let fits = base >= low && later.iter().min().is_none_or(|&high| base < high);
                    if fits {
                        base
                    } else {
                        low
                    }
                }
            };
            Some((id, target))
        })
        .collect();
    // The children by rank, a pinned one first in its rank: a link then runs from the earlier to the later one.
    let mut order: Vec<(&String, usize, bool)> = ranks
        .iter()
        .map(|(id, &base)| match targets.get(id) {
            Some(&target) => (id, target, false),
            None => (id, base, true),
        })
        .collect();
    order.sort_by(|a, b| (a.1, a.2, a.0).cmp(&(b.1, b.2, b.0)));
    let mut neighbours: BTreeMap<&String, Vec<&String>> = BTreeMap::new();
    for (a, b) in links {
        if a != b {
            neighbours.entry(a).or_default().push(b);
            neighbours.entry(b).or_default().push(a);
        }
    }
    let mut done: BTreeMap<&String, usize> = BTreeMap::new();
    for (id, wanted, _) in order {
        let needed = neighbours
            .get(id)
            .into_iter()
            .flatten()
            .filter_map(|n| done.get(n))
            .map(|&r| r + 1)
            .max()
            .unwrap_or(0);
        done.insert(id, wanted.max(needed));
    }
    let done: BTreeMap<String, usize> = done.into_iter().map(|(k, v)| (k.clone(), v)).collect();
    ranks.extend(done);
}

/// Puts the held elements of every rank at their places across the flow (`held`), keeping the gaps (`space`) and the near
/// side of the area (`least`). A held element stands after the others whose middle lies before its middle, as far as they
/// fit before it; the others keep their order and give way. A place that cannot be held (before the near side, or two held
/// elements too close) moves its element on.
pub(crate) fn hold(
    ranks: &mut [Vec<usize>],
    start: &mut [Option<f64>],
    size: &[f64],
    space: &dyn Fn(usize, usize) -> f64,
    held: &[Option<f64>],
    least: f64,
) {
    for ids in ranks.iter_mut() {
        if !ids.iter().any(|&v| held[v].is_some()) {
            continue;
        }
        let middle =
            |start: &[Option<f64>], v: usize| held[v].or(start[v]).unwrap_or(least) + size[v] / 2.;
        let mut order: Vec<usize> = ids.iter().copied().filter(|&v| held[v].is_none()).collect();
        let mut pinned: Vec<usize> = ids.iter().copied().filter(|&v| held[v].is_some()).collect();
        pinned.sort_by(|&a, &b| {
            middle(start, a)
                .total_cmp(&middle(start, b))
                .then(a.cmp(&b))
        });
        for &p in &pinned {
            let at = order
                .iter()
                .rposition(|&u| middle(start, u) < middle(start, p))
                .map_or(0, |i| i + 1);
            order.insert(at, p);
        }
        // The elements before a held one, packed, must end before its place; the nearest of them goes behind it until they
        // do.
        for &p in &pinned {
            let target = held[p].expect("held");
            loop {
                let i = order.iter().position(|&v| v == p).expect("in the rank");
                if i == 0 || held[order[i - 1]].is_some() {
                    break;
                }
                let mut end = least;
                for k in 0..i {
                    let v = order[k];
                    let mut at = if k == 0 {
                        least
                    } else {
                        end + space(order[k - 1], v)
                    };
                    if let Some(t) = held[v] {
                        at = at.max(t);
                    }
                    end = at + size[v];
                }
                if end + space(order[i - 1], p) <= target + EPS {
                    break;
                }
                order.swap(i - 1, i);
            }
        }
        // The latest start of every element that leaves the held ones after it their places.
        let mut latest = vec![f64::INFINITY; order.len()];
        for i in (0..order.len()).rev() {
            let v = order[i];
            latest[i] = match held[v] {
                Some(t) => t,
                None => match order.get(i + 1) {
                    Some(&next) => latest[i + 1] - space(v, next) - size[v],
                    None => f64::INFINITY,
                },
            };
        }
        let mut end = least;
        for (i, &v) in order.iter().enumerate() {
            let low = if i == 0 {
                least
            } else {
                end + space(order[i - 1], v)
            };
            let at = match held[v] {
                Some(t) => t.max(low),
                None => start[v].unwrap_or(least).min(latest[i]).max(low),
            };
            start[v] = Some(at);
            end = at + size[v];
        }
        *ids = order;
    }
}

/// How far every pinned node of the drawing stands from its pin.
pub(crate) fn diagnostics(drawing: &Snapshot, pins: &BTreeMap<String, Point>) -> Vec<String> {
    let round = |v: f64| (v * 10.).round() / 10. + 0.;
    pins.iter()
        .filter_map(|(id, pin)| {
            let r = drawing.boxes.get(id)?;
            let plan = drawing.layered_plans.get(area_of(&drawing.graph, id)?)?;
            let (across, along) = if plan.vertical {
                (r.x - pin.x, r.y - pin.y)
            } else {
                (r.y - pin.y, r.x - pin.x)
            };
            let (across, along) = (round(across), round(along));
            Some(if across == 0. {
                format!("Pin {id}: held across the flow, {along} along it")
            } else {
                format!("Pin {id}: {across} across the flow and {along} along it from the pin")
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ranks(list: &[(&str, usize)]) -> BTreeMap<String, usize> {
        list.iter().map(|(id, r)| (id.to_string(), *r)).collect()
    }

    fn links(list: &[(&str, &str)]) -> BTreeSet<(String, String)> {
        list.iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect()
    }

    fn place(anchors: &[&str], earlier: &[&str], later: &[&str]) -> Place {
        let own = |ids: &[&str]| ids.iter().map(|i| i.to_string()).collect();
        Place {
            at: Point { x: 0., y: 0. },
            anchors: own(anchors),
            earlier: own(earlier),
            later: own(later),
        }
    }

    #[test]
    fn a_pinned_node_takes_its_anchors_rank_and_only_the_nodes_a_link_needs_move_on() {
        // a -> b -> c, d -> c; b is pinned into the band of d (rank 1 -> 0).
        let mut r = ranks(&[("a", 0), ("b", 1), ("c", 2), ("d", 0)]);
        let l = links(&[("a", "b"), ("b", "c"), ("d", "c")]);
        let places = Places::from([("b".into(), place(&["d"], &[], &["c"]))]);
        rank(&mut r, &l, &places);
        // b holds rank 0; a, which had it too and is linked to b, moves on (its edge now runs against the flow).
        assert_eq!(r, ranks(&[("a", 1), ("b", 0), ("c", 2), ("d", 0)]));
        for (x, y) in &l {
            assert_ne!(r[x], r[y], "{x}-{y}");
        }
    }

    #[test]
    fn a_pinned_node_moved_to_a_later_band_reverses_the_links_it_passes() {
        let mut r = ranks(&[("a", 0), ("b", 1), ("c", 2)]);
        let l = links(&[("a", "b"), ("b", "c")]);
        let places = Places::from([("a".into(), place(&["c"], &["b"], &[]))]);
        rank(&mut r, &l, &places);
        // a holds rank 2; c, in that rank and not linked to a, is processed after it and stays.
        assert_eq!(r, ranks(&[("a", 2), ("b", 1), ("c", 2)]));
    }

    #[test]
    fn a_node_alone_in_its_band_stays_between_the_bands_around_it() {
        let mut r = ranks(&[("a", 0), ("b", 1), ("c", 2)]);
        let places = Places::from([("b".into(), place(&[], &["a"], &["c"]))]);
        rank(&mut r, &links(&[("a", "b"), ("b", "c")]), &places);
        assert_eq!(r, ranks(&[("a", 0), ("b", 1), ("c", 2)]));
        // The bands say it stands first: it takes rank 0, and its predecessor moves on.
        let places = Places::from([("b".into(), place(&[], &[], &["a", "c"]))]);
        rank(&mut r, &links(&[("a", "b"), ("b", "c")]), &places);
        assert_eq!(r, ranks(&[("a", 1), ("b", 0), ("c", 2)]));
    }

    /// Runs `hold` on one rank of elements of size 10 with a gap of 4 and a near side at 24.
    fn held(start: &[f64], pins: &[(usize, f64)]) -> (Vec<usize>, Vec<f64>) {
        let n = start.len();
        let mut ids = vec![(0..n).collect::<Vec<_>>()];
        let mut at: Vec<Option<f64>> = start.iter().map(|&v| Some(v)).collect();
        let mut fixed = vec![None; n];
        for &(v, t) in pins {
            fixed[v] = Some(t);
        }
        hold(&mut ids, &mut at, &vec![10.; n], &|_, _| 4., &fixed, 24.);
        (ids.remove(0), at.into_iter().map(|v| v.unwrap()).collect())
    }

    #[test]
    fn a_held_element_stands_at_its_place_and_the_others_give_way() {
        // Dragged behind the last one.
        assert_eq!(
            held(&[24., 38., 52.], &[(0, 200.)]),
            (vec![1, 2, 0], vec![200., 38., 52.])
        );
        // Between two: the one before it gives way towards the near side, the one after it moves on.
        assert_eq!(
            held(&[40., 54., 68.], &[(2, 50.)]),
            (vec![0, 2, 1], vec![36., 64., 50.])
        );
    }

    #[test]
    fn an_element_that_does_not_fit_before_a_held_one_goes_behind_it() {
        // Held at the near side, though another element's middle lies before its own.
        let (order, at) = held(&[24., 38.], &[(1, 30.)]);
        assert_eq!(order, vec![1, 0]);
        assert_eq!(at, vec![44., 30.]);
    }

    #[test]
    fn a_place_that_cannot_be_held_moves_its_element_on() {
        // Before the near side.
        assert_eq!(held(&[24.], &[(0, 3.)]).1, vec![24.]);
        // Two held elements closer than their size and gap.
        assert_eq!(
            held(&[24., 38.], &[(0, 100.), (1, 105.)]).1,
            vec![100., 114.]
        );
    }
}
