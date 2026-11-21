//! A segment tree over elementary pieces (indexes `0..pieces`) that combines values over ranges: `raise(a, b, v)` combines
//! `v` into every piece of `[a, b)`, `extreme(a, b)` gives the combination of everything raised over any piece of `[a, b)`.
//! With `max` (or `min`) as the combination this answers "the greatest value among the intervals given so far that overlap
//! this one" in `O(log n)`, where a scan over the intervals given so far is linear.

#[derive(Clone)]
pub(crate) struct RangeTree<T: Copy> {
    pieces: usize,
    neutral: T,
    pick: fn(T, T) -> T,
    /// The combination of the values raised over a node's whole range.
    tag: Vec<T>,
    /// The combination of everything raised within a node's range (its tag included).
    sub: Vec<T>,
}

impl<T: Copy> RangeTree<T> {
    pub(crate) fn new(pieces: usize, neutral: T, pick: fn(T, T) -> T) -> Self {
        let size = 4 * pieces.max(1);
        RangeTree {
            pieces,
            neutral,
            pick,
            tag: vec![neutral; size],
            sub: vec![neutral; size],
        }
    }

    pub(crate) fn pieces(&self) -> usize {
        self.pieces
    }

    pub(crate) fn raise(&mut self, a: usize, b: usize, v: T) {
        if a < b.min(self.pieces) {
            self.raise_at(1, 0, self.pieces, a, b.min(self.pieces), v);
        }
    }

    fn raise_at(&mut self, node: usize, lo: usize, hi: usize, a: usize, b: usize, v: T) {
        if b <= lo || hi <= a {
            return;
        }
        if a <= lo && hi <= b {
            self.tag[node] = (self.pick)(self.tag[node], v);
            self.sub[node] = (self.pick)(self.sub[node], v);
            return;
        }
        let mid = (lo + hi) / 2;
        self.raise_at(2 * node, lo, mid, a, b, v);
        self.raise_at(2 * node + 1, mid, hi, a, b, v);
        let children = (self.pick)(self.sub[2 * node], self.sub[2 * node + 1]);
        self.sub[node] = (self.pick)(self.tag[node], children);
    }

    pub(crate) fn extreme(&self, a: usize, b: usize) -> T {
        let b = b.min(self.pieces);
        if a >= b {
            return self.neutral;
        }
        self.extreme_at(1, 0, self.pieces, a, b)
    }

    fn extreme_at(&self, node: usize, lo: usize, hi: usize, a: usize, b: usize) -> T {
        if b <= lo || hi <= a {
            return self.neutral;
        }
        if a <= lo && hi <= b {
            return self.sub[node];
        }
        let mid = (lo + hi) / 2;
        let children = (self.pick)(
            self.extreme_at(2 * node, lo, mid, a, b),
            self.extreme_at(2 * node + 1, mid, hi, a, b),
        );
        (self.pick)(self.tag[node], children)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tree_answers_as_a_scan_over_the_raised_intervals() {
        let mut state = 5_u64;
        let mut next = |n: u64| {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (state >> 33) % n
        };
        for _ in 0..300 {
            let pieces = 1 + next(40) as usize;
            let mut tree = RangeTree::new(pieces, 0usize, usize::max);
            let mut given: Vec<(usize, usize, usize)> = Vec::new();
            for _ in 0..30 {
                let a = next(pieces as u64) as usize;
                let b = a + 1 + next((pieces - a) as u64) as usize;
                let (qa, qb) = {
                    let x = next(pieces as u64) as usize;
                    (x, x + 1 + next((pieces - x) as u64) as usize)
                };
                let scan = given
                    .iter()
                    .filter(|g| g.0 < qb && qa < g.1)
                    .map(|g| g.2)
                    .max()
                    .unwrap_or(0);
                assert_eq!(tree.extreme(qa, qb), scan);
                let v = 1 + next(100) as usize;
                tree.raise(a, b, v);
                given.push((a, b, v));
            }
        }
    }
}
