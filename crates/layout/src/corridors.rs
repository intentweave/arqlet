//! Provisional (`LayoutOptions::organic_corridors`, #156, ADR 0020 amendment, step C1): the corridor plan of every organic
//! area, made on the boxes the layout placed. A measurement: nothing in the layout reads it.
//!
//! 1. The free space of an area (the rectangle around its children's boxes, with a margin) is cut twice: into the
//!    rectangles between two boxes that are next to each other along x (the corridors of axis `x`), and into those between
//!    two next to each other along y. Every free point lies in one of each. A box and the area's outer edge bound a
//!    corridor too.
//! 2. Where a corridor of one axis meets one of the other is a junction. A route goes from junction to junction; moving
//!    along y it runs along a corridor of axis `x`, moving along x along one of axis `y`.
//! 3. Every edge between two children gets for each end the side that faces the other end, and the shortest way between
//!    those sides, then the one with the fewest turns, where entering a corridor between two boxes costs `route_cost` for
//!    every route already in it (the space at the outer edge costs nothing). The edges are planned one after another in
//!    the order of their ids.
//!
//! All coordinates are integers (`QUANTUM` per layout unit), so the plan is the same on every platform.

use arqlet_model::{
    Corridor, CorridorAxis, CorridorPlan, CorridorRoute, CorridorSkipped, CorridorSystem, Graph,
    Mode, PortSide, Rect, Snapshot,
};
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};

/// The integer units of one layout unit.
const QUANTUM: f64 = 1024.;
/// How far the free space reaches beyond the outermost boxes.
const MARGIN: f64 = 48.;
/// The default of `OrganicCorridors::route_cost` (measured: `benchmarks/results/organic-corridor-plan-20261126.md`).
const DEFAULT_ROUTE_COST: f64 = 1.;
const MAX_ROUTE_COST: f64 = 10_000.;
const NONE: u32 = u32::MAX;

/// The distances a corridor's need is made of, in layout units.
#[derive(Clone, Copy)]
pub(crate) struct Distances {
    pub clearance: f64,
    pub track: f64,
    pub stub: f64,
    pub gap: f64,
}

/// A rectangle by its ranges: `[0]` along x, `[1]` along y.
type Area = [(i64, i64); 2];

/// The index of an axis: 0 is x, 1 is y.
type Axis = usize;

/// A free rectangle between the box `before` and the box `after` along one axis (`None`: the area's outer edge).
struct Cell {
    before: Option<usize>,
    after: Option<usize>,
    rect: Area,
}

struct Junction {
    rect: Area,
    /// The corridor (an index into all cells) that a route runs along when it moves along x (`[0]`) or y (`[1]`).
    along: [u32; 2],
    /// The neighbouring junctions along x and along y.
    next: [Vec<u32>; 2],
}

/// The free space of one area.
struct Space {
    boxes: Vec<Area>,
    /// The corridors of axis x, then those of axis y.
    cells: Vec<Cell>,
    x_cells: usize,
    junctions: Vec<Junction>,
    /// The junctions that start (`[axis][0]`) or end (`[axis][1]`) at a coordinate of an axis.
    bounds: [[BTreeMap<i64, Vec<u32>>; 2]; 2],
}

fn quantize(value: f64) -> i64 {
    (value * QUANTUM).round() as i64
}

fn units(value: i64) -> f64 {
    value as f64 / QUANTUM
}

/// The corridors of one axis: for every stretch across it, the free intervals between boxes that follow each other along it.
fn cells(boxes: &[Area], extent: Area, axis: Axis) -> Vec<Cell> {
    let across = 1 - axis;
    let mut cuts: Vec<i64> = boxes
        .iter()
        .flat_map(|b| [b[across].0, b[across].1])
        .chain([extent[across].0, extent[across].1])
        .collect();
    cuts.sort_unstable();
    cuts.dedup();
    let mut along: Vec<usize> = (0..boxes.len()).collect();
    along.sort_by_key(|&i| (boxes[i][axis].0, i));
    let mut out: Vec<Cell> = Vec::new();
    // The cells of the stretch before, by their two boxes and their range along the axis.
    type Key = (Option<usize>, Option<usize>, i64, i64);
    let mut open: BTreeMap<Key, usize> = BTreeMap::new();
    for stretch in cuts.windows(2) {
        let (from, to) = (stretch[0], stretch[1]);
        let mut found: Vec<Key> = Vec::new();
        let mut cursor = extent[axis].0;
        let mut before = None;
        for &i in &along {
            let b = boxes[i];
            if b[across].0 > from || b[across].1 < to {
                continue;
            }
            if b[axis].0 > cursor {
                found.push((before, Some(i), cursor, b[axis].0));
            }
            if b[axis].1 > cursor {
                cursor = b[axis].1;
                before = Some(i);
            }
        }
        if extent[axis].1 > cursor {
            found.push((before, None, cursor, extent[axis].1));
        }
        let mut now = BTreeMap::new();
        for key in found {
            let cell = match open.get(&key) {
                Some(&c) => {
                    out[c].rect[across].1 = to;
                    c
                }
                None => {
                    let mut rect = [(0, 0); 2];
                    rect[axis] = (key.2, key.3);
                    rect[across] = (from, to);
                    out.push(Cell {
                        before: key.0,
                        after: key.1,
                        rect,
                    });
                    out.len() - 1
                }
            };
            now.insert(key, cell);
        }
        open = now;
    }
    out
}

impl Space {
    fn new(boxes: Vec<Area>) -> Self {
        let margin = quantize(MARGIN);
        let mut extent = [(i64::MAX, i64::MIN); 2];
        for b in &boxes {
            for axis in 0..2 {
                extent[axis].0 = extent[axis].0.min(b[axis].0 - margin);
                extent[axis].1 = extent[axis].1.max(b[axis].1 + margin);
            }
        }
        let mut all = cells(&boxes, extent, 0);
        let x_cells = all.len();
        all.extend(cells(&boxes, extent, 1));
        // The y cells by their start along x, so that each x cell looks only at those that begin before it ends.
        let mut y_order: Vec<usize> = (x_cells..all.len()).collect();
        y_order.sort_by_key(|&c| (all[c].rect[0].0, c));
        let mut junctions: Vec<Junction> = Vec::new();
        for x in 0..x_cells {
            let v = all[x].rect;
            for &y in &y_order {
                let h = all[y].rect;
                if h[0].0 >= v[0].1 {
                    break;
                }
                let rect = [
                    (v[0].0.max(h[0].0), v[0].1.min(h[0].1)),
                    (v[1].0.max(h[1].0), v[1].1.min(h[1].1)),
                ];
                if rect[0].0 < rect[0].1 && rect[1].0 < rect[1].1 {
                    junctions.push(Junction {
                        rect,
                        // Along x a route runs along the y cell, along y along the x cell.
                        along: [y as u32, x as u32],
                        next: [Vec::new(), Vec::new()],
                    });
                }
            }
        }
        let mut bounds: [[BTreeMap<i64, Vec<u32>>; 2]; 2] = Default::default();
        for (j, junction) in junctions.iter().enumerate() {
            for (axis, bound) in bounds.iter_mut().enumerate() {
                bound[0]
                    .entry(junction.rect[axis].0)
                    .or_default()
                    .push(j as u32);
                bound[1]
                    .entry(junction.rect[axis].1)
                    .or_default()
                    .push(j as u32);
            }
        }
        // Neighbours along an axis share a stretch of the line where one ends and the other starts. The junctions do not
        // overlap, so those at one line follow each other across it.
        for axis in 0..2 {
            let across = 1 - axis;
            let mut pairs: Vec<(u32, u32)> = Vec::new();
            for (line, ending) in &bounds[axis][1] {
                let Some(starting) = bounds[axis][0].get(line) else {
                    continue;
                };
                let sorted = |list: &[u32]| {
                    let mut list = list.to_vec();
                    list.sort_by_key(|&j| junctions[j as usize].rect[across].0);
                    list
                };
                let (a, b) = (sorted(ending), sorted(starting));
                let (mut i, mut k) = (0, 0);
                while i < a.len() && k < b.len() {
                    let ra = junctions[a[i] as usize].rect[across];
                    let rb = junctions[b[k] as usize].rect[across];
                    if ra.0.max(rb.0) < ra.1.min(rb.1) {
                        pairs.push((a[i], b[k]));
                    }
                    if ra.1 <= rb.1 {
                        i += 1;
                    } else {
                        k += 1;
                    }
                }
            }
            for (a, b) in pairs {
                junctions[a as usize].next[axis].push(b);
                junctions[b as usize].next[axis].push(a);
            }
        }
        Space {
            boxes,
            cells: all,
            x_cells,
            junctions,
            bounds,
        }
    }

    /// The junctions at one side of a box, each with its distance along the side from the side's middle: a route may take
    /// its port anywhere on the side, and pays for the way from the middle.
    fn at_side(&self, b: usize, side: PortSide) -> Vec<(u32, i64)> {
        let (axis, high) = side_axis(side);
        let across = 1 - axis;
        let line = self.side_line(b, side);
        let range = self.boxes[b][across];
        let middle = (range.0 + range.1) / 2;
        // Beyond the high side the junctions start at the line, beyond the low side they end there.
        let Some(list) = self.bounds[axis][usize::from(!high)].get(&line) else {
            return Vec::new();
        };
        list.iter()
            .filter_map(|&j| {
                let r = self.junctions[j as usize].rect[across];
                (r.0.max(range.0) < r.1.min(range.1))
                    .then(|| (j, (r.0 - middle).max(middle - r.1).max(0)))
            })
            .collect()
    }

    /// The coordinate of a box's side along the side's axis.
    fn side_line(&self, b: usize, side: PortSide) -> i64 {
        let (axis, high) = side_axis(side);
        if high {
            self.boxes[b][axis].1
        } else {
            self.boxes[b][axis].0
        }
    }

    /// The side of box `a` that faces box `b` and the side of `b` that faces `a`: along the axis on which they are further
    /// apart (or overlap less). Then the facing sides of the other axis and the remaining sides, for a box without room
    /// there.
    fn facing_sides(&self, a: usize, b: usize) -> [PortSide; 4] {
        let (p, q) = (self.boxes[a], self.boxes[b]);
        let apart = |axis: Axis| {
            let distance = (q[axis].0 + q[axis].1 - p[axis].0 - p[axis].1).abs();
            distance - (p[axis].1 - p[axis].0) - (q[axis].1 - q[axis].0)
        };
        let after = |axis: Axis| q[axis].0 + q[axis].1 >= p[axis].0 + p[axis].1;
        let side = |axis: Axis, high: bool| match (axis, high) {
            (0, false) => PortSide::Left,
            (0, true) => PortSide::Right,
            (_, false) => PortSide::Top,
            (_, true) => PortSide::Bottom,
        };
        let first = if apart(0) >= apart(1) { 0 } else { 1 };
        let second = 1 - first;
        [
            side(first, after(first)),
            side(second, after(second)),
            side(second, !after(second)),
            side(first, !after(first)),
        ]
    }
}

/// The axis a side is crossed on, and whether it is the high side of that axis.
fn side_axis(side: PortSide) -> (Axis, bool) {
    match side {
        PortSide::Left => (0, false),
        PortSide::Right => (0, true),
        PortSide::Top => (1, false),
        PortSide::Bottom => (1, true),
    }
}

/// One end of a route: the side of a box and the junctions at it.
struct End {
    b: usize,
    side: PortSide,
    at: Vec<(u32, i64)>,
}

/// A planned way: its states from the source to the target, its length and its turns.
struct Way {
    /// (junction, axis of movement)
    steps: Vec<(u32, Axis)>,
    length: i64,
    turns: u32,
}

/// The state of a search: a junction, the axis the route moves along there, and whether it has only just started along it
/// (at the source, or after a turn).
fn state(junction: u32, axis: Axis, fresh: bool) -> usize {
    junction as usize * 4 + axis * 2 + usize::from(fresh)
}

#[derive(Clone, Copy)]
struct Label {
    search: u32,
    parent: u32,
    cost: i64,
    penalty: i64,
    turns: u32,
    /// Where across its direction the route may be: the stretch a straight run so far leaves open.
    band: (i64, i64),
    done: bool,
}

struct Search {
    labels: Vec<Label>,
    round: u32,
    queue: BinaryHeap<Reverse<(i64, u32, u32)>>,
}

impl Search {
    fn new(junctions: usize) -> Self {
        Search {
            labels: vec![
                Label {
                    search: 0,
                    parent: NONE,
                    cost: 0,
                    penalty: 0,
                    turns: 0,
                    band: (0, 0),
                    done: false,
                };
                junctions * 4
            ],
            round: 0,
            queue: BinaryHeap::new(),
        }
    }

    fn offer(
        &mut self,
        at: usize,
        parent: u32,
        cost: i64,
        penalty: i64,
        turns: u32,
        band: (i64, i64),
    ) {
        let label = &mut self.labels[at];
        if label.search == self.round && (label.done || (label.cost, label.turns) <= (cost, turns))
        {
            return;
        }
        *label = Label {
            search: self.round,
            parent,
            cost,
            penalty,
            turns,
            band,
            done: false,
        };
        self.queue.push(Reverse((cost, turns, at as u32)));
    }

    /// The cheapest way from `source` to `target`; `penalty` is what entering each corridor costs.
    fn way(&mut self, space: &Space, source: &End, target: &End, penalty: &[i64]) -> Option<Way> {
        self.round += 1;
        self.queue.clear();
        let (start_axis, _) = side_axis(source.side);
        let (end_axis, _) = side_axis(target.side);
        let start_line = space.side_line(source.b, source.side);
        let end_line = space.side_line(target.b, target.side);
        let centre = |j: u32, axis: Axis| {
            let r = space.junctions[j as usize].rect[axis];
            (r.0 + r.1) / 2
        };
        // The stretch of a junction across an axis that lies on a box's side.
        let on_side = |j: u32, b: usize, axis: Axis| {
            let r = space.junctions[j as usize].rect[1 - axis];
            let side = space.boxes[b][1 - axis];
            (r.0.max(side.0), r.1.min(side.1))
        };
        for &(j, offset) in &source.at {
            let band = on_side(j, source.b, start_axis);
            let cost = (centre(j, start_axis) - start_line).abs() + offset;
            self.offer(state(j, start_axis, true), NONE, cost, 0, 0, band);
        }
        let targets: BTreeMap<u32, i64> = target.at.iter().copied().collect();
        let states = self.labels.len() as u32;
        // The cheapest arrival found: (cost, turns, penalty, state).
        let mut best: Option<(i64, u32, i64, u32)> = None;
        while let Some(Reverse((cost, turns, at))) = self.queue.pop() {
            if at >= states {
                break;
            }
            let label = self.labels[at as usize];
            if label.done || (label.cost, label.turns) != (cost, turns) {
                continue;
            }
            self.labels[at as usize].done = true;
            let (j, axis, fresh) = (at / 4, (at as usize / 2) % 2, at % 2 == 1);
            let junction = &space.junctions[j as usize];
            let corridor = junction.along[axis] as usize;
            let started = label.parent == NONE;
            if let Some(&offset) = targets.get(&j).filter(|_| axis == end_axis) {
                let side = on_side(j, target.b, end_axis);
                let jog = (side.0 - label.band.1).max(label.band.0 - side.1).max(0);
                // A straight way within one junction runs along its corridor; a stub after a turn does not.
                let enter = if started { penalty[corridor] } else { 0 };
                let total = cost + (end_line - centre(j, end_axis)).abs() + offset + jog + enter;
                let bends = turns + if jog > 0 { 2 } else { 0 };
                if best.is_none_or(|b| (total, bends) < (b.0, b.1)) {
                    best = Some((total, bends, label.penalty + enter, at));
                    self.queue.push(Reverse((total, bends, states + at)));
                }
            }
            for &k in &junction.next[axis] {
                let next = &space.junctions[k as usize];
                let r = next.rect[1 - axis];
                let band = (label.band.0.max(r.0), label.band.1.min(r.1));
                let (band, jog) = if band.0 <= band.1 {
                    (band, 0)
                } else if r.0 > label.band.1 {
                    ((r.0, r.0), r.0 - label.band.1)
                } else {
                    ((r.1, r.1), label.band.0 - r.1)
                };
                let there = next.along[axis] as usize;
                let enter = if fresh { penalty[corridor] } else { 0 }
                    + if there != corridor { penalty[there] } else { 0 };
                let step = (centre(k, axis) - centre(j, axis)).abs() + jog;
                self.offer(
                    state(k, axis, false),
                    at,
                    cost + step + enter,
                    label.penalty + enter,
                    turns + if jog > 0 { 2 } else { 0 },
                    band,
                );
            }
            // A turn, not twice in one place.
            if !fresh || started {
                self.offer(
                    state(j, 1 - axis, true),
                    at,
                    cost,
                    label.penalty,
                    turns + 1,
                    junction.rect[axis],
                );
            }
        }
        let (total, turns, penalty, last) = best?;
        let mut steps = Vec::new();
        let mut at = last;
        while at != NONE {
            steps.push((at / 4, (at as usize / 2) % 2));
            at = self.labels[at as usize].parent;
        }
        steps.reverse();
        Some(Way {
            steps,
            length: total - penalty,
            turns,
        })
    }
}

/// An edge between two boxes of an area.
pub(crate) struct Link {
    pub edge: String,
    pub source: usize,
    pub target: usize,
}

/// The plan of one area: its boxes (`ids`, with their rectangles) and the edges between them, in the order they are
/// planned.
pub(crate) fn plan_area(
    area: &str,
    ids: &[&str],
    rects: &[Rect],
    links: &[Link],
    distances: Distances,
    route_cost: f64,
    mut skipped: CorridorSkipped,
) -> CorridorPlan {
    let boxes: Vec<Area> = rects
        .iter()
        .map(|r| {
            [
                (quantize(r.x), quantize(r.x + r.width)),
                (quantize(r.y), quantize(r.y + r.height)),
            ]
        })
        .collect();
    let space = Space::new(boxes);
    let mut search = Search::new(space.junctions.len());
    let cost = quantize(route_cost);
    let mut penalty = vec![0i64; space.cells.len()];
    let mut demand = vec![0usize; space.cells.len()];
    let mut ports = vec![0usize; space.cells.len()];
    let mut routes = Vec::new();
    // The interior segments of all routes, per axis of their position: the corridors each runs along and where it lies.
    let mut segments: [Vec<(i64, Vec<usize>)>; 2] = Default::default();
    for link in links {
        let sources = space.facing_sides(link.source, link.target);
        let targets = space.facing_sides(link.target, link.source);
        let end = |b: usize, sides: [PortSide; 4]| {
            sides.into_iter().find_map(|side| {
                let at = space.at_side(b, side);
                (!at.is_empty()).then_some(End { b, side, at })
            })
        };
        let planned = end(link.source, sources)
            .zip(end(link.target, targets))
            .and_then(|(source, target)| {
                search
                    .way(&space, &source, &target, &penalty)
                    .map(|way| (source.side, target.side, way))
            });
        let Some((source_side, target_side, way)) = planned else {
            skipped.unrouted += 1;
            continue;
        };
        // The corridor a port faces: the one across the side's axis at the first and the last junction.
        for (junction, side) in [
            (way.steps[0].0, source_side),
            (way.steps[way.steps.len() - 1].0, target_side),
        ] {
            let axis = side_axis(side).0;
            ports[space.junctions[junction as usize].along[1 - axis] as usize] += 1;
        }
        // The segments of the way: one per direction, between its turns, with the corridors each moves along.
        let mut parts: Vec<(Axis, u32, Vec<usize>)> =
            vec![(way.steps[0].1, way.steps[0].0, Vec::new())];
        for pair in way.steps.windows(2) {
            let ((j, a), (k, b)) = (pair[0], pair[1]);
            if a != b {
                parts.push((b, k, Vec::new()));
                continue;
            }
            let run = &mut parts.last_mut().expect("a segment").2;
            for junction in [j, k] {
                let corridor = space.junctions[junction as usize].along[a] as usize;
                if !run.contains(&corridor) {
                    run.push(corridor);
                }
            }
        }
        // A way within one junction without a turn runs straight from port to port along its corridor.
        if way.steps.len() == 1 {
            let (j, axis) = way.steps[0];
            parts[0]
                .2
                .push(space.junctions[j as usize].along[axis] as usize);
        }
        let mut used: Vec<usize> = Vec::new();
        for (_, _, run) in &parts {
            for &c in run {
                if !used.contains(&c) {
                    used.push(c);
                }
            }
        }
        for &c in &used {
            demand[c] += 1;
            // Only the space between two boxes has a width to share; at the outer edge the area grows.
            if space.cells[c].before.is_some() && space.cells[c].after.is_some() {
                penalty[c] = cost.saturating_mul(demand[c] as i64);
            }
        }
        for (moving, junction, run) in parts.iter().skip(1).take(parts.len().saturating_sub(2)) {
            // A segment that moves along one axis has its position on the other.
            let position = 1 - moving;
            let r = space.junctions[*junction as usize].rect[position];
            segments[position].push(((r.0 + r.1) / 2, run.clone()));
        }
        routes.push(CorridorRoute {
            edge: link.edge.clone(),
            source_side,
            target_side,
            corridors: used,
            length: units(way.length),
            turns: way.turns as usize,
        });
    }
    let system = |axis: Axis| {
        let of_axis = |c: usize| (c >= space.x_cells) == (axis == 1);
        let node_pairs: BTreeSet<(usize, usize)> = space
            .cells
            .iter()
            .enumerate()
            .filter(|(c, _)| of_axis(*c))
            .filter_map(|(_, cell)| cell.before.zip(cell.after))
            .collect();
        let mut segment_node: BTreeSet<(usize, usize)> = BTreeSet::new();
        let mut inside: BTreeMap<usize, Vec<(i64, usize)>> = BTreeMap::new();
        for (s, (position, run)) in segments[axis].iter().enumerate() {
            for &c in run {
                let cell = &space.cells[c];
                segment_node.extend(
                    [cell.before, cell.after]
                        .into_iter()
                        .flatten()
                        .map(|b| (s, b)),
                );
                inside.entry(c).or_default().push((*position, s));
            }
        }
        let mut segment_pairs: BTreeSet<(usize, usize)> = BTreeSet::new();
        for list in inside.values_mut() {
            list.sort_unstable();
            segment_pairs.extend(
                list.windows(2)
                    .map(|w| (w[0].1.min(w[1].1), w[0].1.max(w[1].1))),
            );
        }
        CorridorSystem {
            node_variables: space.boxes.len(),
            segment_variables: segments[axis].len(),
            node_pairs: node_pairs.len(),
            segment_node: segment_node.len(),
            segment_pairs: segment_pairs.len(),
        }
    };
    let (x_system, y_system) = (system(0), system(1));
    let corridors = space
        .cells
        .iter()
        .enumerate()
        .map(|(c, cell)| {
            let axis = usize::from(c >= space.x_cells);
            let need = if demand[c] == 0 {
                distances.gap
            } else {
                let stub = if ports[c] > 0 { distances.stub } else { 0. };
                (2. * distances.clearance + demand[c] as f64 * distances.track + stub)
                    .max(distances.gap)
            };
            let name = |b: Option<usize>| b.map(|b| ids[b].to_string());
            Corridor {
                axis: if axis == 0 {
                    CorridorAxis::X
                } else {
                    CorridorAxis::Y
                },
                before: name(cell.before),
                after: name(cell.after),
                rect: Rect {
                    x: units(cell.rect[0].0),
                    y: units(cell.rect[1].0),
                    width: units(cell.rect[0].1 - cell.rect[0].0),
                    height: units(cell.rect[1].1 - cell.rect[1].0),
                },
                demand: demand[c],
                ports: ports[c],
                need,
            }
        })
        .collect();
    CorridorPlan {
        area: area.to_string(),
        boxes: space.boxes.len(),
        clearance: distances.clearance,
        track: distances.track,
        stub: distances.stub,
        gap: distances.gap,
        route_cost,
        corridors,
        junctions: space.junctions.len(),
        routes,
        skipped,
        x_system,
        y_system,
    }
}

/// The cost per planned route the options ask for.
pub(crate) fn route_cost(options: &arqlet_model::LayoutOptions) -> Result<f64, String> {
    let cost = options
        .organic_corridors
        .as_ref()
        .and_then(|o| o.route_cost)
        .unwrap_or(DEFAULT_ROUTE_COST);
    if cost.is_finite() && (0. ..=MAX_ROUTE_COST).contains(&cost) {
        Ok(cost)
    } else {
        Err(format!(
            "organicCorridors.routeCost must be a finite value from 0 to {MAX_ROUTE_COST}"
        ))
    }
}

/// The plan of every organic area with at least two children, the root first, then the containers by id. Edges drawn
/// straight are left out: corridors are for orthogonal routes.
pub(crate) fn plan(
    snapshot: &Snapshot,
    distances: Distances,
    route_cost: f64,
) -> Vec<CorridorPlan> {
    let graph: &Graph = &snapshot.graph;
    let parents: BTreeMap<&str, Option<&str>> = graph
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.parent_id.as_deref()))
        .collect();
    let mut children: BTreeMap<Option<&str>, BTreeSet<&str>> = BTreeMap::new();
    for (&id, &parent) in &parents {
        if snapshot.boxes.contains_key(id) {
            children.entry(parent).or_default().insert(id);
        }
    }
    let straight: BTreeSet<&str> = snapshot.straight_edges.iter().map(String::as_str).collect();
    let mut edges: Vec<&arqlet_model::Edge> = graph.edges.iter().collect();
    edges.sort_by(|a, b| a.id.cmp(&b.id));
    let mut plans = Vec::new();
    for (area, members) in children {
        if members.len() < 2 || graph.area_mode(area) != Ok(Mode::Organic) {
            continue;
        }
        let ids: Vec<&str> = members.into_iter().collect();
        let index: BTreeMap<&str, usize> = ids.iter().enumerate().map(|(i, &id)| (id, i)).collect();
        // The child of the area that holds a node (the node itself when it is a child).
        let child_of = |id: &str| -> Option<usize> {
            std::iter::successors(Some(id), |x| parents.get(x).copied().flatten())
                .find(|x| parents.get(x).copied().flatten() == area)
                .and_then(|x| index.get(x).copied())
        };
        let mut links = Vec::new();
        let mut skipped = CorridorSkipped::default();
        for edge in &edges {
            let Some(target) = edge.target.as_deref() else {
                continue;
            };
            match (child_of(&edge.source), child_of(target)) {
                (Some(a), Some(b)) if a != b => {
                    if straight.contains(edge.id.as_str()) {
                        skipped.straight += 1;
                    } else {
                        links.push(Link {
                            edge: edge.id.clone(),
                            source: a,
                            target: b,
                        });
                    }
                }
                // Within one child: its own loop here, anything deeper belongs to an area inside it.
                (Some(a), Some(_)) => {
                    if edge.source == target && edge.source == ids[a] {
                        skipped.self_loops += 1;
                    }
                }
                (None, None) => {}
                _ => skipped.leaving += 1,
            }
        }
        let rects: Vec<Rect> = ids.iter().map(|&id| snapshot.boxes[id].clone()).collect();
        plans.push(plan_area(
            area.unwrap_or("<root>"),
            &ids,
            &rects,
            &links,
            distances,
            route_cost,
            skipped,
        ));
    }
    plans
}

#[cfg(test)]
mod tests {
    use super::*;

    const DISTANCES: Distances = Distances {
        clearance: 12.,
        track: 6.,
        stub: 12.,
        gap: 24.,
    };

    fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    fn links(pairs: &[(usize, usize)]) -> Vec<Link> {
        pairs
            .iter()
            .enumerate()
            .map(|(i, &(source, target))| Link {
                edge: format!("e{i}"),
                source,
                target,
            })
            .collect()
    }

    fn planned(rects: &[Rect], pairs: &[(usize, usize)], cost: f64) -> CorridorPlan {
        let ids = ["a", "b", "c", "d", "e", "f", "g", "h", "i"];
        plan_area(
            "<root>",
            &ids[..rects.len()],
            rects,
            &links(pairs),
            DISTANCES,
            cost,
            CorridorSkipped::default(),
        )
    }

    fn between<'p>(plan: &'p CorridorPlan, a: &str, b: &str) -> &'p Corridor {
        plan.corridors
            .iter()
            .find(|c| c.before.as_deref() == Some(a) && c.after.as_deref() == Some(b))
            .expect("a corridor")
    }

    #[test]
    fn the_free_space_is_covered_once_by_each_axis() {
        let rects = [
            rect(0., 0., 100., 40.),
            rect(130., 10., 60., 80.),
            rect(20., 70., 70., 50.),
        ];
        let plan = planned(&rects, &[], 0.);
        let lo = |f: fn(&Rect) -> f64| rects.iter().map(f).fold(f64::INFINITY, f64::min);
        let hi = |f: fn(&Rect) -> f64| rects.iter().map(f).fold(f64::NEG_INFINITY, f64::max);
        let width = hi(|r| r.x + r.width) - lo(|r| r.x) + 2. * MARGIN;
        let height = hi(|r| r.y + r.height) - lo(|r| r.y) + 2. * MARGIN;
        let free = width * height - rects.iter().map(|r| r.width * r.height).sum::<f64>();
        for axis in [CorridorAxis::X, CorridorAxis::Y] {
            let area: f64 = plan
                .corridors
                .iter()
                .filter(|c| c.axis == axis)
                .map(|c| c.rect.width * c.rect.height)
                .sum();
            assert!((area - free).abs() < 1e-6, "{axis:?}: {area} of {free}");
        }
        for c in &plan.corridors {
            for r in &rects {
                let overlaps = c.rect.x < r.x + r.width
                    && r.x < c.rect.x + c.rect.width
                    && c.rect.y < r.y + r.height
                    && r.y < c.rect.y + c.rect.height;
                assert!(!overlaps, "{c:?} covers {r:?}");
            }
        }
        assert_eq!(between(&plan, "a", "b").rect.width, 30.);
    }

    #[test]
    fn facing_neighbours_are_joined_straight_across_their_gap() {
        let rects = [rect(0., 0., 100., 50.), rect(124., 0., 100., 50.)];
        let plan = planned(&rects, &[(0, 1)], 0.);
        let route = &plan.routes[0];
        assert_eq!(
            (route.source_side, route.target_side),
            (PortSide::Right, PortSide::Left)
        );
        assert_eq!((route.length, route.turns), (24., 0));
        // The route runs across the gap, along a corridor of the other axis; the gap itself only has the two ports.
        let gap = between(&plan, "a", "b");
        assert_eq!((gap.demand, gap.ports, gap.need), (0, 2, 24.));
        assert!(route
            .corridors
            .iter()
            .all(|&c| plan.corridors[c].axis == CorridorAxis::Y));
    }

    #[test]
    fn routes_past_a_box_load_the_corridors_they_run_along_and_set_their_need() {
        // Three boxes in a row: the edges between the outer ones go around the middle one.
        let rects = [
            rect(0., 0., 100., 50.),
            rect(124., 0., 100., 50.),
            rect(248., 0., 100., 50.),
        ];
        let plan = planned(&rects, &[(0, 2), (2, 0)], 0.);
        assert_eq!(plan.routes.len(), 2);
        for route in &plan.routes {
            assert!(route.turns >= 2, "{route:?}");
        }
        let used: Vec<&Corridor> = plan.corridors.iter().filter(|c| c.demand > 0).collect();
        assert!(!used.is_empty());
        for c in used {
            let stub = if c.ports > 0 { 12. } else { 0. };
            assert_eq!(c.need, 24. + 6. * c.demand as f64 + stub, "{c:?}");
        }
        // Interior segments are variables of the joint system.
        assert!(plan.x_system.segment_variables + plan.y_system.segment_variables > 0);
        assert_eq!(plan.x_system.node_variables, 3);
        assert_eq!(plan.x_system.node_pairs, 2);
    }

    #[test]
    fn a_cost_per_route_spreads_routes_over_parallel_corridors() {
        // A box between two ends, with a box above and one below it: between it and each of them runs an equally long way.
        let rects = [
            rect(0., 0., 60., 60.),
            rect(100., 0., 60., 60.),
            rect(200., 0., 60., 60.),
            rect(100., -100., 60., 60.),
            rect(100., 100., 60., 60.),
        ];
        let pairs = vec![(0, 2); 6];
        let split = |cost: f64| {
            let plan = planned(&rects, &pairs, cost);
            assert_eq!(plan.routes.len(), 6);
            (
                between(&plan, "d", "b").demand,
                between(&plan, "b", "e").demand,
            )
        };
        let (above, below) = split(0.);
        assert_eq!(above.max(below), 6);
        let (above, below) = split(12.);
        assert_eq!((above, below), (3, 3));
    }

    #[test]
    fn the_plan_does_not_depend_on_anything_but_its_input() {
        let rects: Vec<Rect> = (0..9)
            .map(|i| {
                rect(
                    f64::from(i % 3) * 150.1,
                    f64::from(i / 3) * 90.3 + f64::from(i) * 3.7,
                    100.,
                    54.,
                )
            })
            .collect();
        let pairs: Vec<(usize, usize)> = (0..9).map(|i| (i, (i * 4 + 1) % 9)).collect();
        let pairs: Vec<_> = pairs.into_iter().filter(|(a, b)| a != b).collect();
        let first = planned(&rects, &pairs, 24.);
        assert_eq!(first, planned(&rects, &pairs, 24.));
        assert_eq!(first.skipped.unrouted, 0);
        assert_eq!(first.routes.len(), pairs.len());
    }

    #[test]
    fn a_box_without_room_on_any_side_is_reported_as_unrouted() {
        // The second box lies inside the first: no side of it touches free space.
        let rects = [rect(0., 0., 200., 200.), rect(50., 50., 20., 20.)];
        let plan = planned(&rects, &[(0, 1)], 0.);
        assert_eq!((plan.routes.len(), plan.skipped.unrouted), (0, 1));
    }
}
