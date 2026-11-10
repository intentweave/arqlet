//! Provisional (single-pass mode): coordinates across the flow by block alignment and compaction, after Brandes and Köpf
//! ("Fast and Simple Horizontal Coordinate Assignment", GD 2001), with the class shifts of the erratum by Brandes, Walter
//! and Zink (2020) and with port offsets inside elements, as ELK's layered algorithm does. Written from the papers; no
//! code copied.
//!
//! Every element of a rank keeps its order. Elements joined by a segment between adjacent ranks may form a block, inside
//! which every segment is straight: the two ports lie on one line across the flow. Segments between two dummies (inner
//! segments) have priority over segments that cross them. The four variants (aligned upwards or downwards, compacted to
//! either side) are combined by the average of the two median places of every element, when that keeps every rank's
//! order and gaps; otherwise the narrowest variant is used.

/// A segment between elements of adjacent ranks (in either order), each with its port's offset from its start. Others
/// are ignored.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Segment {
    pub upper: usize,
    pub upper_offset: f64,
    pub lower: usize,
    pub lower_offset: f64,
}

pub(crate) struct Input<'a> {
    pub ranks: &'a [Vec<usize>],
    pub size: &'a [f64],
    /// Dummies: segments between two of them are inner segments.
    pub dummy: &'a [bool],
    pub segments: &'a [Segment],
    /// The least distance between two neighbours of a rank, the one with the smaller coordinate first.
    pub space: &'a dyn Fn(usize, usize) -> f64,
}

/// Starts across the flow for the elements of `ranks` (others stay `None`); the leftmost start is 0.
pub(crate) fn place(input: &Input<'_>) -> Vec<Option<f64>> {
    let n = input.size.len();
    let mut rank_of = vec![usize::MAX; n];
    for (r, ids) in input.ranks.iter().enumerate() {
        for &id in ids {
            rank_of[id] = r;
        }
    }
    let mut variants = Vec::with_capacity(4);
    for downwards in [true, false] {
        for leftwards in [true, false] {
            variants.push(variant(input, &rank_of, downwards, leftwards));
        }
    }
    let width = |x: &[f64]| {
        let (lo, hi) = extent(input, x);
        hi - lo
    };
    let narrowest = (0..4)
        .min_by(|&a, &b| {
            width(&variants[a])
                .total_cmp(&width(&variants[b]))
                .then(a.cmp(&b))
        })
        .expect("four variants");
    // Align the left variants on the narrowest one's left side, the right variants on its right side.
    let (lo, hi) = extent(input, &variants[narrowest]);
    for (i, x) in variants.iter_mut().enumerate() {
        let (l, h) = extent(input, x);
        let delta = if i % 2 == 0 { lo - l } else { hi - h };
        for v in x.iter_mut() {
            *v += delta;
        }
    }
    let mut balanced = vec![0.; n];
    for v in 0..n {
        let mut values = [
            variants[0][v],
            variants[1][v],
            variants[2][v],
            variants[3][v],
        ];
        values.sort_by(f64::total_cmp);
        balanced[v] = (values[1] + values[2]) / 2.;
    }
    let mut chosen = if keeps_order(input, &balanced) {
        balanced
    } else {
        variants.swap_remove(narrowest)
    };
    straighten(input, &rank_of, &mut chosen);
    let (lo, _) = extent(input, &chosen);
    let mut out = vec![None; n];
    for ids in input.ranks {
        for &id in ids {
            out[id] = Some(chosen[id] - lo);
        }
    }
    out
}

/// Edge straightening: an element none of whose segments is straight moves onto the line of one of its neighbours'
/// ports, the nearest one its rank's order and gaps allow. Moving it straightens a segment and bends none.
fn straighten(input: &Input<'_>, rank_of: &[usize], x: &mut [f64]) {
    let n = input.size.len();
    let mut ends: Vec<Vec<(usize, f64, f64)>> = vec![Vec::new(); n];
    for s in input.segments {
        if rank_of[s.upper] == usize::MAX || rank_of[s.lower] == usize::MAX {
            continue;
        }
        ends[s.upper].push((s.lower, s.upper_offset, s.lower_offset));
        ends[s.lower].push((s.upper, s.lower_offset, s.upper_offset));
    }
    let straight = |x: &[f64], v: usize| {
        ends[v]
            .iter()
            .any(|&(u, own, theirs)| (x[v] + own - x[u] - theirs).abs() < 1e-6)
    };
    for _ in 0..2 {
        for ids in input.ranks {
            for (i, &v) in ids.iter().enumerate() {
                if ends[v].is_empty() || straight(x, v) {
                    continue;
                }
                let low = if i > 0 {
                    let p = ids[i - 1];
                    x[p] + input.size[p] + (input.space)(p, v)
                } else {
                    f64::NEG_INFINITY
                };
                let high = if i + 1 < ids.len() {
                    let q = ids[i + 1];
                    x[q] - input.size[v] - (input.space)(v, q)
                } else {
                    f64::INFINITY
                };
                let best = ends[v]
                    .iter()
                    .map(|&(u, own, theirs)| x[u] + theirs - own)
                    .filter(|&t| t >= low - 1e-9 && t <= high + 1e-9)
                    .min_by(|a, b| {
                        (a - x[v])
                            .abs()
                            .total_cmp(&(b - x[v]).abs())
                            .then(a.total_cmp(b))
                    });
                if let Some(t) = best {
                    x[v] = t;
                }
            }
        }
    }
}

fn extent(input: &Input<'_>, x: &[f64]) -> (f64, f64) {
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for ids in input.ranks {
        for &id in ids {
            lo = lo.min(x[id]);
            hi = hi.max(x[id] + input.size[id]);
        }
    }
    if lo.is_finite() {
        (lo, hi)
    } else {
        (0., 0.)
    }
}

fn keeps_order(input: &Input<'_>, x: &[f64]) -> bool {
    input.ranks.iter().all(|ids| {
        ids.windows(2)
            .all(|w| x[w[1]] + 1e-6 >= x[w[0]] + input.size[w[0]] + (input.space)(w[0], w[1]))
    })
}

/// One variant. Downwards aligns every element with a median neighbour in the rank above (upwards: below); leftwards
/// compacts to the left (rightwards: mirrored). Returns starts in the original coordinates.
fn variant(input: &Input<'_>, rank_of: &[usize], downwards: bool, leftwards: bool) -> Vec<f64> {
    let n = input.size.len();
    // Ranks in sweep order, each in compaction order.
    let ranks: Vec<Vec<usize>> = {
        let mut ranks: Vec<Vec<usize>> = input.ranks.to_vec();
        if !downwards {
            ranks.reverse();
        }
        if !leftwards {
            for ids in &mut ranks {
                ids.reverse();
            }
        }
        ranks
    };
    let mut pos = vec![0usize; n];
    let mut level = vec![usize::MAX; n];
    for (r, ids) in ranks.iter().enumerate() {
        for (i, &id) in ids.iter().enumerate() {
            pos[id] = i;
            level[id] = r;
        }
    }
    // Offsets in compaction coordinates: mirrored elements measure their ports from the other side.
    let offset = |id: usize, o: f64| if leftwards { o } else { input.size[id] - o };
    // For every element, its neighbours in the previous rank of the sweep: (neighbour, own offset, neighbour's offset).
    let mut previous: Vec<Vec<(usize, f64, f64)>> = vec![Vec::new(); n];
    for s in input.segments {
        let (mut u, mut v, mut u_o, mut v_o) = (s.upper, s.lower, s.upper_offset, s.lower_offset);
        if rank_of[u] == usize::MAX || rank_of[v] == usize::MAX {
            continue;
        }
        if rank_of[v] + 1 == rank_of[u] {
            (u, v, u_o, v_o) = (v, u, v_o, u_o);
        }
        if rank_of[u] + 1 != rank_of[v] {
            continue;
        }
        let (u_o, v_o) = (offset(u, u_o), offset(v, v_o));
        if downwards {
            previous[v].push((u, v_o, u_o));
        } else {
            previous[u].push((v, u_o, v_o));
        }
    }
    for list in &mut previous {
        list.sort_by(|a, b| pos[a.0].cmp(&pos[b.0]).then(a.2.total_cmp(&b.2)));
    }
    let inner = |a: usize, b: usize| input.dummy[a] && input.dummy[b];
    // Type 1 conflicts: a non-inner segment that crosses an inner segment between the same ranks gives way.
    let mut marked: Vec<Vec<bool>> = previous.iter().map(|l| vec![false; l.len()]).collect();
    let pos_ref = &pos;
    for r in 1..ranks.len() {
        let inners: Vec<(usize, usize)> = ranks[r]
            .iter()
            .flat_map(|&v| {
                previous[v]
                    .iter()
                    .filter(move |&&(u, _, _)| inner(u, v))
                    .map(move |&(u, _, _)| (pos_ref[u], pos_ref[v]))
            })
            .collect();
        if inners.is_empty() {
            continue;
        }
        for &v in &ranks[r] {
            for (k, &(u, _, _)) in previous[v].iter().enumerate() {
                if inner(u, v) {
                    continue;
                }
                let (pu, pv) = (pos[u], pos[v]);
                marked[v][k] = inners
                    .iter()
                    .any(|&(a, b)| (a < pu && b > pv) || (a > pu && b < pv));
            }
        }
    }
    // Vertical alignment: root and next element of every block (a cycle), and each element's start relative to the
    // start of its predecessor in the block.
    let mut root: Vec<usize> = (0..n).collect();
    let mut align: Vec<usize> = (0..n).collect();
    let mut link = vec![0.; n];
    for ids in ranks.iter().skip(1) {
        let mut r: Option<usize> = None;
        for &v in ids {
            let d = previous[v].len();
            if d == 0 {
                continue;
            }
            for m in [(d - 1) / 2, d / 2] {
                if align[v] != v {
                    break;
                }
                let (u, own, theirs) = previous[v][m];
                // `u` must still end its block, and the segment must not cross one aligned before.
                if marked[v][m] || r.is_some_and(|r| r >= pos[u]) || align[u] != root[u] {
                    continue;
                }
                align[u] = v;
                root[v] = root[u];
                align[v] = root[v];
                link[v] = theirs - own;
                r = Some(pos[u]);
            }
        }
    }
    // Each element's start relative to its block's, the least of them 0.
    let mut inner_shift = vec![0.; n];
    for v in 0..n {
        if root[v] != v || level[v] == usize::MAX {
            continue;
        }
        let mut w = v;
        let mut least: f64 = 0.;
        loop {
            let next = align[w];
            if next == v {
                break;
            }
            inner_shift[next] = inner_shift[w] + link[next];
            least = least.min(inner_shift[next]);
            w = next;
        }
        let mut w = v;
        loop {
            inner_shift[w] -= least;
            w = align[w];
            if w == v {
                break;
            }
        }
    }
    let x = compact(input, &ranks, &root, &align, &inner_shift, leftwards);
    if leftwards {
        x
    } else {
        (0..n).map(|v| -x[v] - input.size[v]).collect()
    }
}

/// Horizontal compaction: blocks as far left as their left neighbours allow, then every class (the blocks that share a
/// sink) as near to its right neighbour classes as their gaps allow.
fn compact(
    input: &Input<'_>,
    ranks: &[Vec<usize>],
    root: &[usize],
    align: &[usize],
    inner_shift: &[f64],
    leftwards: bool,
) -> Vec<f64> {
    let n = input.size.len();
    let mut pred = vec![usize::MAX; n];
    for ids in ranks {
        for w in ids.windows(2) {
            pred[w[1]] = w[0];
        }
    }
    // The ranks run mirrored in a rightward variant: the neighbour before is the one after in the original order, and the
    // space (which may differ by side) is asked in the original order.
    let space = |a: usize, b: usize| {
        if leftwards {
            (input.space)(a, b)
        } else {
            (input.space)(b, a)
        }
    };
    let mut sink: Vec<usize> = (0..n).collect();
    let mut x: Vec<Option<f64>> = vec![None; n];
    // Blocks in an order in which every left neighbour's block comes first (iteratively, no recursion).
    let roots: Vec<usize> = ranks
        .iter()
        .flatten()
        .copied()
        .filter(|&v| root[v] == v)
        .collect();
    let mut waiting_on = vec![false; n];
    for &start in &roots {
        if x[start].is_some() {
            continue;
        }
        let mut stack = vec![start];
        waiting_on[start] = true;
        while let Some(&v) = stack.last() {
            // Every left neighbour's block placed?
            let mut waiting = None;
            let mut w = v;
            loop {
                if pred[w] != usize::MAX {
                    let u = root[pred[w]];
                    // A block already waiting further down the stack would be a cycle; it counts as placed at 0.
                    if x[u].is_none() && !waiting_on[u] {
                        waiting = Some(u);
                        break;
                    }
                }
                w = align[w];
                if w == v {
                    break;
                }
            }
            if let Some(u) = waiting {
                stack.push(u);
                waiting_on[u] = true;
                continue;
            }
            stack.pop();
            waiting_on[v] = false;
            if x[v].is_some() {
                continue;
            }
            let mut place: f64 = 0.;
            let mut w = v;
            loop {
                if pred[w] != usize::MAX {
                    let p = pred[w];
                    let u = root[p];
                    if sink[v] == v {
                        sink[v] = sink[u];
                    }
                    if sink[v] == sink[u] {
                        let needed =
                            x[u].unwrap_or(0.) + inner_shift[p] + input.size[p] + space(p, w)
                                - inner_shift[w];
                        place = place.max(needed);
                    }
                }
                w = align[w];
                if w == v {
                    break;
                }
            }
            x[v] = Some(place);
        }
    }
    // Class shifts (erratum): a class lies left of the classes it has a neighbour in; it moves right until the first gap
    // closes. Classes are visited from the right, so that every right neighbour class has its shift.
    let mut constraints: Vec<(usize, usize, f64)> = Vec::new(); // (left class, right class, slack)
    for ids in ranks {
        for w in ids.windows(2) {
            let (p, q) = (w[0], w[1]);
            let (cp, cq) = (sink[root[p]], sink[root[q]]);
            if cp == cq {
                continue;
            }
            let xp = x[root[p]].unwrap_or(0.) + inner_shift[p];
            let xq = x[root[q]].unwrap_or(0.) + inner_shift[q];
            constraints.push((cp, cq, xq - xp - input.size[p] - space(p, q)));
        }
    }
    let mut shift: Vec<Option<f64>> = vec![None; n];
    if !constraints.is_empty() {
        let classes: Vec<usize> = {
            let mut c: Vec<usize> = (0..n).filter(|&v| sink[v] == v && root[v] == v).collect();
            c.sort_unstable();
            c
        };
        // Longest path from the right: shift(c) = min over right classes d of shift(d) + slack(c, d); classes without
        // a right neighbour stay. Relaxed until stable (the class graph is acyclic for order-preserving blocks).
        let mut changed = true;
        let mut rounds = 0;
        while changed && rounds <= classes.len() + 1 {
            changed = false;
            rounds += 1;
            for &(c, d, slack) in &constraints {
                let candidate = shift[d].unwrap_or(0.) + slack;
                let current = shift[c].unwrap_or(f64::INFINITY);
                if candidate < current - 1e-9 {
                    shift[c] = Some(candidate);
                    changed = true;
                }
            }
        }
    }
    (0..n)
        .map(|v| {
            let r = root[v];
            x[r].unwrap_or(0.) + inner_shift[v] + shift[sink[r]].unwrap_or(0.)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(ranks: &[Vec<usize>], size: &[f64], dummy: &[bool], segments: &[Segment]) -> Vec<f64> {
        let space = |_: usize, _: usize| 10.;
        place(&Input {
            ranks,
            size,
            dummy,
            segments,
            space: &space,
        })
        .into_iter()
        .map(|x| x.expect("placed"))
        .collect()
    }

    fn seg(upper: usize, upper_offset: f64, lower: usize, lower_offset: f64) -> Segment {
        Segment {
            upper,
            upper_offset,
            lower,
            lower_offset,
        }
    }

    #[test]
    fn a_chain_of_dummies_is_straight_between_its_ports() {
        // 0 (wide, port at 70) -> 2 -> 4 (dummies) -> 5 (port at 10); 1 and 3 are neighbours in the ranks.
        let ranks = vec![vec![0, 1], vec![2, 3], vec![4], vec![5]];
        let size = [100., 40., 6., 40., 6., 60.];
        let dummy = [false, false, true, false, true, false];
        let segments = [
            seg(0, 70., 2, 3.),
            seg(2, 3., 4, 3.),
            seg(4, 3., 5, 10.),
            seg(1, 20., 3, 20.),
        ];
        let x = run(&ranks, &size, &dummy, &segments);
        let line = x[0] + 70.;
        assert!((x[2] + 3. - line).abs() < 1e-9, "{x:?}");
        assert!((x[4] + 3. - line).abs() < 1e-9, "{x:?}");
        assert!((x[5] + 10. - line).abs() < 1e-9, "{x:?}");
        assert!(keeps_order(
            &Input {
                ranks: &ranks,
                size: &size,
                dummy: &dummy,
                segments: &segments,
                space: &|_, _| 10.
            },
            &x
        ));
    }

    #[test]
    fn crossing_segments_keep_the_order_and_the_gaps() {
        // Two crossing segments: 0 -> 3 and 1 -> 2; only one can be straight.
        let ranks = vec![vec![0, 1], vec![2, 3]];
        let size = [20., 20., 20., 20.];
        let dummy = [false; 4];
        let segments = [seg(0, 10., 3, 10.), seg(1, 10., 2, 10.)];
        let x = run(&ranks, &size, &dummy, &segments);
        assert!(
            x[1] >= x[0] + 30. - 1e-9 && x[3] >= x[2] + 30. - 1e-9,
            "{x:?}"
        );
        assert!(x.iter().copied().fold(f64::INFINITY, f64::min).abs() < 1e-9);
    }

    #[test]
    fn parallel_and_unconnected_elements_are_placed() {
        let ranks = vec![vec![0], vec![1, 2], vec![3]];
        let size = [50., 20., 20., 50.];
        let dummy = [false; 4];
        let segments = [
            seg(0, 10., 1, 10.),
            seg(0, 40., 1, 10.),
            seg(1, 10., 3, 25.),
        ];
        let x = run(&ranks, &size, &dummy, &segments);
        assert!(x[2] >= x[1] + 30. - 1e-9, "{x:?}");
        assert!(x.iter().all(|v| v.is_finite()));
    }

    /// A space that differs by side (port leads, `portSides`) holds in every variant, the mirrored ones too: the gap after
    /// element 0 is wider than the one after element 1.
    #[test]
    fn a_space_that_differs_by_side_holds_in_mirrored_variants() {
        let ranks = vec![vec![0, 1, 2], vec![3]];
        let size = [20.; 4];
        let dummy = [false; 4];
        let segments = [seg(1, 10., 3, 10.)];
        let space = |a: usize, b: usize| if (a, b) == (0, 1) { 40. } else { 10. };
        let x: Vec<f64> = place(&Input {
            ranks: &ranks,
            size: &size,
            dummy: &dummy,
            segments: &segments,
            space: &space,
        })
        .into_iter()
        .map(|x| x.expect("placed"))
        .collect();
        assert!(x[1] >= x[0] + 20. + 40. - 1e-9, "{x:?}");
        assert!(x[2] >= x[1] + 20. + 10. - 1e-9, "{x:?}");
    }
}
