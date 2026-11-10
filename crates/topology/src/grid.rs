//! Grid positions from a canonical ordering (Chrobak and Payne, 1995), implemented from the algorithm description.
//!
//! Phase 1 inserts the vertices in canonical order. Each vertex stores only a relative x-offset `delta_x` and its
//! height `y`, and every contour vertex has a left and right tree child: the relative position of a vertex is the sum
//! of offsets along the tree. Inserting `v_k` stretches the gap between its extreme contour neighbours `wp` and `wq`
//! by one unit, places `v_k` where a slope +1 line from `wp` and a slope -1 line from `wq` meet, and reattaches the
//! trees. Phase 2 walks the trees from `v1` to produce absolute coordinates. For a triangulation with `N` vertices the
//! result lies on the `(2N - 4) x (N - 2)` grid, the bound stated for this construction (checked by the tests).
use crate::ordering::CanonicalOrder;
use crate::TopologyError;

/// Grid coordinates of all vertices of the triangulation, indexed by vertex id.
pub fn positions(n: usize, order: &CanonicalOrder) -> Result<Vec<(i64, i64)>, TopologyError> {
    if order.len() != n || n < 3 {
        return Err(TopologyError::InvalidInput(
            "ordering must cover at least three vertices".into(),
        ));
    }
    let mut delta_x = vec![0i64; n];
    let mut y = vec![0i64; n];
    let mut left: Vec<Option<usize>> = vec![None; n];
    let mut right: Vec<Option<usize>> = vec![None; n];

    let (v1, v2, v3) = (order[0].0, order[1].0, order[2].0);
    delta_x[v1] = 0;
    y[v1] = 0;
    right[v1] = Some(v3);
    delta_x[v2] = 1;
    y[v2] = 0;
    delta_x[v3] = 1;
    y[v3] = 1;
    right[v3] = Some(v2);

    for (vk, contour) in order.iter().skip(3) {
        let vk = *vk;
        if contour.len() < 2 {
            return Err(TopologyError::Internal(
                "insertion without contour neighbours".into(),
            ));
        }
        let wp = contour[0];
        let wp1 = contour[1];
        let wq = contour[contour.len() - 1];
        let wq1 = contour[contour.len() - 2];
        let multiple = contour.len() > 2;

        delta_x[wp1] += 1;
        delta_x[wq] += 1;
        let span: i64 = contour[1..].iter().map(|&u| delta_x[u]).sum();

        delta_x[vk] = (-y[wp] + span + y[wq]).div_euclid(2);
        y[vk] = (y[wp] + span + y[wq]).div_euclid(2);
        delta_x[wq] = span - delta_x[vk];
        if multiple {
            delta_x[wp1] -= delta_x[vk];
        }

        right[wp] = Some(vk);
        right[vk] = Some(wq);
        if multiple {
            left[vk] = Some(wp1);
            right[wq1] = None;
        } else {
            left[vk] = None;
        }
    }

    let mut pos = vec![(0i64, 0i64); n];
    let mut assigned = vec![false; n];
    pos[v1] = (0, y[v1]);
    assigned[v1] = true;
    let mut stack = vec![v1];
    while let Some(parent) = stack.pop() {
        for tree in [&left, &right] {
            if let Some(child) = tree[parent] {
                if assigned[child] {
                    return Err(TopologyError::Internal("tree child reached twice".into()));
                }
                pos[child] = (pos[parent].0 + delta_x[child], y[child]);
                assigned[child] = true;
                stack.push(child);
            }
        }
    }
    if assigned.iter().any(|&a| !a) {
        return Err(TopologyError::Internal(
            "some vertex has no position".into(),
        ));
    }
    // Shift to nonnegative coordinates; v1 is the leftmost vertex and has x = 0 by construction.
    let min_y = pos.iter().map(|p| p.1).min().unwrap_or(0);
    for p in &mut pos {
        p.1 -= min_y;
    }
    Ok(pos)
}
