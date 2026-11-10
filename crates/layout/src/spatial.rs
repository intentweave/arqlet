use std::collections::BTreeMap;

const COORD_EPSILON: f64 = 1e-9;

/// A deterministic uniform grid over node centres.
pub(crate) struct SpatialHash {
    cell_size: f64,
    positions: Vec<(f64, f64)>,
    cell_of: Vec<(i64, i64)>,
    buckets: BTreeMap<(i64, i64), Vec<usize>>,
}

impl SpatialHash {
    pub(crate) fn new(positions: &[(f64, f64)], cell_size: f64) -> Self {
        assert!(cell_size.is_finite() && cell_size > 0.);
        let mut hash = Self {
            cell_size,
            positions: Vec::new(),
            cell_of: Vec::new(),
            buckets: BTreeMap::new(),
        };
        hash.rebuild(positions);
        hash
    }

    pub(crate) fn rebuild(&mut self, positions: &[(f64, f64)]) {
        self.buckets.clear();
        self.positions.clear();
        self.cell_of.clear();
        self.positions.extend_from_slice(positions);
        self.cell_of.reserve(positions.len());
        for (index, &position) in positions.iter().enumerate() {
            let cell = self.cell(position);
            self.cell_of.push(cell);
            self.buckets.entry(cell).or_default().push(index);
        }
    }

    pub(crate) fn update(&mut self, index: usize, position: (f64, f64)) {
        let old_cell = self.cell_of[index];
        let new_cell = self.cell(position);
        self.positions[index] = position;
        if old_cell == new_cell {
            return;
        }
        let empty = {
            let bucket = self
                .buckets
                .get_mut(&old_cell)
                .expect("indexed cell exists");
            let slot = bucket
                .binary_search(&index)
                .expect("indexed node exists in its cell");
            bucket.remove(slot);
            bucket.is_empty()
        };
        if empty {
            self.buckets.remove(&old_cell);
        }
        let bucket = self.buckets.entry(new_cell).or_default();
        let slot = bucket.binary_search(&index).unwrap_or_else(|slot| slot);
        bucket.insert(slot, index);
        self.cell_of[index] = new_cell;
    }

    /// Returns the lowest-index candidate at or after `start` whose centre is within one cell on each axis.
    pub(crate) fn next_neighbor(
        &self,
        centre: (f64, f64),
        start: usize,
        exclude: usize,
    ) -> Option<usize> {
        let (cx, cy) = self.cell(centre);
        let reach = self.cell_size + COORD_EPSILON;
        let mut next = None;
        for dx in -1..=1 {
            for dy in -1..=1 {
                let cell = (cx.saturating_add(dx), cy.saturating_add(dy));
                let Some(nodes) = self.buckets.get(&cell) else {
                    continue;
                };
                let start_at = nodes.partition_point(|&index| index < start);
                for &index in &nodes[start_at..] {
                    if next.is_some_and(|current| index >= current) {
                        break;
                    }
                    if index == exclude {
                        continue;
                    }
                    let position = self.positions[index];
                    if (position.0 - centre.0).abs() <= reach
                        && (position.1 - centre.1).abs() <= reach
                    {
                        next = Some(index);
                        break;
                    }
                }
            }
        }
        next
    }

    /// Every candidate with an index above `after` whose centre is within one cell on each axis, in ascending index: the
    /// sequence that repeated `next_neighbor` calls from `after + 1` give while the hash does not change, in one pass.
    pub(crate) fn neighbors_after(
        &self,
        centre: (f64, f64),
        after: usize,
        result: &mut Vec<usize>,
    ) {
        result.clear();
        let (cx, cy) = self.cell(centre);
        let reach = self.cell_size + COORD_EPSILON;
        for dx in -1..=1 {
            for dy in -1..=1 {
                let cell = (cx.saturating_add(dx), cy.saturating_add(dy));
                let Some(nodes) = self.buckets.get(&cell) else {
                    continue;
                };
                let start_at = nodes.partition_point(|&index| index <= after);
                for &index in &nodes[start_at..] {
                    let position = self.positions[index];
                    if (position.0 - centre.0).abs() <= reach
                        && (position.1 - centre.1).abs() <= reach
                    {
                        result.push(index);
                    }
                }
            }
        }
        result.sort_unstable();
    }

    /// All nodes whose centres fall in the bounds, sorted by node index. Large windows fall back to all nodes.
    pub(crate) fn query_bounds(&self, min: (f64, f64), max: (f64, f64), result: &mut Vec<usize>) {
        result.clear();
        if self.positions.is_empty() {
            return;
        }
        let low = self.cell(min);
        let high = self.cell(max);
        let width = (i128::from(high.0) - i128::from(low.0) + 1) as u128;
        let height = (i128::from(high.1) - i128::from(low.1) + 1) as u128;
        let limit = self.buckets.len().saturating_mul(4).max(64) as u128;
        if width.saturating_mul(height) > limit {
            result.extend(0..self.positions.len());
            return;
        }
        let mut x = low.0;
        loop {
            let mut y = low.1;
            loop {
                if let Some(nodes) = self.buckets.get(&(x, y)) {
                    result.extend(nodes.iter().copied());
                }
                if y == high.1 {
                    break;
                }
                y = y.saturating_add(1);
            }
            if x == high.0 {
                break;
            }
            x = x.saturating_add(1);
        }
        result.sort_unstable();
        result.dedup();
    }

    pub(crate) fn point_in_bounds(&self, index: usize, min: (f64, f64), max: (f64, f64)) -> bool {
        let position = self.positions[index];
        position.0 >= min.0 && position.0 <= max.0 && position.1 >= min.1 && position.1 <= max.1
    }

    fn cell(&self, position: (f64, f64)) -> (i64, i64) {
        (
            (position.0 / self.cell_size).floor() as i64,
            (position.1 / self.cell_size).floor() as i64,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arqlet_model::outline::{within, Outline};
    use arqlet_model::{NodeShape, Rect};

    fn outline(shape: NodeShape, centre: (f64, f64), size: (f64, f64)) -> Outline {
        Outline::of(
            shape,
            &Rect {
                x: centre.0 - size.0 / 2.,
                y: centre.1 - size.1 / 2.,
                width: size.0,
                height: size.1,
            },
        )
    }

    fn centre(outline: &Outline) -> (f64, f64) {
        let bounds = outline.bounds();
        (bounds.x + bounds.width / 2., bounds.y + bounds.height / 2.)
    }

    #[test]
    fn neighbouring_cells_cover_all_clearance_overlaps() {
        let gap = 24.;
        let outlines = [
            outline(NodeShape::Rect, (-180., -80.), (320., 60.)),
            outline(NodeShape::Circle, (0., -80.), (90., 90.)),
            outline(NodeShape::Rect, (182., -80.), (52., 200.)),
            outline(NodeShape::Circle, (165., 30.), (160., 160.)),
            outline(NodeShape::Rect, (500., 320.), (80., 50.)),
            outline(NodeShape::Circle, (-490., 300.), (40., 40.)),
            outline(NodeShape::Rect, (-420., 300.), (120., 40.)),
        ];
        let centers: Vec<_> = outlines.iter().map(centre).collect();
        let extent = outlines
            .iter()
            .map(|outline| {
                let bounds = outline.bounds();
                bounds.width.max(bounds.height)
            })
            .fold(0., f64::max);
        let hash = SpatialHash::new(&centers, extent + gap);
        for a in 0..outlines.len() {
            let mut candidates = Vec::new();
            let mut start = a + 1;
            while let Some(b) = hash.next_neighbor(centers[a], start, a) {
                candidates.push(b);
                start = b + 1;
            }
            for b in a + 1..outlines.len() {
                if within(&outlines[a], &outlines[b], gap) {
                    assert!(
                        candidates.contains(&b),
                        "missed conflicting pair ({a}, {b})"
                    );
                }
            }
        }
    }

    #[test]
    fn segment_windows_include_every_hit_and_the_large_window_fallback_is_complete() {
        let outlines = [
            outline(NodeShape::Rect, (-250., -50.), (80., 240.)),
            outline(NodeShape::Circle, (-40., 0.), (100., 100.)),
            outline(NodeShape::Rect, (120., 140.), (210., 70.)),
            outline(NodeShape::Circle, (430., 250.), (45., 45.)),
        ];
        let centers: Vec<_> = outlines.iter().map(centre).collect();
        let extent = outlines
            .iter()
            .map(|outline| {
                let bounds = outline.bounds();
                bounds.width.max(bounds.height)
            })
            .fold(0., f64::max);
        let padding = extent + 24.;
        let hash = SpatialHash::new(&centers, padding);
        let segments: [((f64, f64), (f64, f64), f64); 3] = [
            ((-400., -50.), (400., 250.), 24.),
            ((-40., -500.), (-40., 500.), 0.),
            ((-100_000., 0.), (100_000., 0.), 24.),
        ];
        for (from, to, margin) in segments {
            let min = (from.0.min(to.0) - padding, from.1.min(to.1) - padding);
            let max = (from.0.max(to.0) + padding, from.1.max(to.1) + padding);
            let mut candidates = Vec::new();
            hash.query_bounds(min, max, &mut candidates);
            for (index, outline) in outlines.iter().enumerate() {
                if outline.segment_enters(from.0, from.1, to.0, to.1, margin) {
                    assert!(
                        candidates.contains(&index),
                        "missed segment blocker {index} for {from:?} -> {to:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn moving_a_node_updates_and_restores_its_bucket() {
        let start = [(0., 0.), (15., 0.), (400., 0.)];
        let mut hash = SpatialHash::new(&start, 20.);
        assert_eq!(hash.next_neighbor(start[0], 1, 0), Some(1));
        hash.update(2, (18., 0.));
        assert_eq!(hash.next_neighbor(start[0], 1, 0), Some(1));
        let mut candidates = Vec::new();
        hash.query_bounds((17., -1.), (19., 1.), &mut candidates);
        assert!(candidates.contains(&2));
        hash.update(2, start[2]);
        hash.query_bounds((17., -1.), (19., 1.), &mut candidates);
        assert!(!candidates.contains(&2));
    }
}

#[cfg(test)]
mod neighbors_after_tests {
    use super::SpatialHash;

    #[test]
    fn neighbors_after_is_the_sequence_of_next_neighbor() {
        let mut seed: u32 = 7;
        let mut random = || {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            seed as f64 / 4294967296. * 1000.
        };
        let points: Vec<(f64, f64)> = (0..300).map(|_| (random(), random())).collect();
        let hash = SpatialHash::new(&points, 60.);
        let mut batch = Vec::new();
        for i in 0..points.len() {
            let mut one_by_one = Vec::new();
            let mut next = i + 1;
            while let Some(j) = hash.next_neighbor(points[i], next, i) {
                one_by_one.push(j);
                next = j + 1;
            }
            hash.neighbors_after(points[i], i, &mut batch);
            assert_eq!(batch, one_by_one, "neighbours of {i}");
        }
    }
}
