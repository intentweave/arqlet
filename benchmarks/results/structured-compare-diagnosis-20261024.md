> **Superseded.** Its cause is wrong: the flat containers came from the ignored authored container directions, not from a fixed shape. See `structured-compare-correction-20261024.md`.

# Diagnosis: crossings and length growth in the composition (bpmn-pools, network-vlan)

Machine: Apple M3, 8 logical CPUs, 16 GiB, macOS 26.0. Revision: a0c2145 (main, after #61). Program: `crates/runtime/examples/diagnose.rs`, default options, one thread. Drawings: `benchmarks/results/structured-compare/`. Derived values read from the SVG rectangles (container and node centres and sizes), and from the routes recomputed in the program.

## Where the crossings are (innermost container)

| Diagram | Joint | Composed |
|---|---|---|
| bpmn-pools | 3 at root | 6: lane:sales 4, lane:fulfilment 1, pool:customer 1 |
| network-vlan | 83: dc 44, vlan10 24, root 14, vlan20 1 | 70: dc 60, vlan10 6, vlan20 2, vlan30 2 |

The composed drawing of bpmn-pools has no crossings at the root; its extra crossings lie inside containers, four of them in lane:sales. Why they lie there is not yet checked.

## Where the length growth comes from (network-vlan)

Total growth +15699, with 20 of 47 edges shorter. The largest growth is on edges to nodes in vlan99 and vlan30 (for example core-b → sw99: 1006 → 3005; core-a → sw30: 615 → 2304). The composed drawing places vlan30 and vlan99 at x ≈ 2300–3150, while core and firewall nodes lie at x ≈ 130–550.

## The containers are flattened (both diagrams)

| Container | Joint size (aspect w/h) | Composed size (aspect) | Area |
|---|---|---|---|
| vlan10 | 472 × 672 (0.70) | 1632 × 220 (7.42) | ×1.13 |
| vlan20 | 472 × 516 (0.91) | 1224 × 220 (5.56) | ×1.11 |
| vlan30 | 472 × 516 (0.91) | 1224 × 220 (5.56) | ×1.11 |
| vlan99 | 472 × 204 (2.31) | 408 × 220 (1.85) | ×0.93 |
| lane:sales | 636 × 834 (0.76) | 1668 × 258 (6.47) | ×0.81 |
| lane:fulfilment | 636 × 126 (5.05) | 204 × 258 (0.79) | ×0.66 |
| pool:customer | 432 × 362 (1.19) | 692 × 180 (3.84) | ×0.80 |
| pool:logistics | 228 × 362 (0.63) | 692 × 102 (6.78) | ×0.86 |
| pool:shop | 1384 × 906 (1.53) | 1960 × 306 (6.41) | ×0.48 |

The total area of each container stays in the same range; its shape changes. The composed containers are long and flat, because each container's size is taken from its own interior layout, which lays the children out in one row. Outside nodes then sit far from the container they connect to.

## Conclusion

1. **network-vlan**: the length loss comes from the fixed container shape. The outer layout has to place wide, flat containers next to nodes that stay in a column, so their connections run across the drawing. The crossing count is not the problem (70 against 83).
2. **bpmn-pools**: the lanes are flattened in the same way (lane:sales 0.76 → 6.47). The additional crossings lie inside lane:sales, which is consistent with flattened lanes forcing edges through their interior. This is consistent, not yet shown.
3. The shape, not the total area, is what the fixed contract gets wrong here.

## Consequence for the next step

The evidence supports the fallback named in the plan: the fixed geometry from the interior leaves too little room for the outside connections. The next step is a **size (shape) negotiation** for each container: try several interior layouts or orientations for a container, keep the one whose resulting size lets the outer layout place it well, and decide by the full composed key. Boundary track planning is not needed for these two diagrams.

## Limits

- Two diagrams, one run each, one option set.
- The crossing cause in bpmn-pools is not verified.
- The container sizes are read from the rendered rectangles, not from a separate export.
