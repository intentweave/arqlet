# ADR 0014: Self-loops follow a loop rule in every routing style

Date: 2026-10-27.
Status: accepted and implemented. The rule is behaviour, not an option: it has no switch and no entry in the options or the report.

Amends [ADR 0004](0004-organic-placement-and-straight-routing.md) (straight routing refused self-loops).

## Context

A self-loop has no second end to draw a segment to, so straight routing refused any graph that had one. Cycles, self-loops and parallel
edges are not edge cases to add later. A loop on a circle also had both ends at the same point (`attachment_toward` the node itself is
undefined), and complete bundling merged the loops of one node into one route.

## Decision

- A self-loop is never drawn straight. In an area that asks for straight routing, the ordinary edges stay straight and the loop is drawn as
  a loop (the orthogonal seed with the loop clearance, then the router). In an area that admits several styles the loop takes the others,
  else the orthogonal loop (`styles::edge_choices`).
- On a circle the two ends of a loop lie on the sides the loop rule picks (right and top, or bottom and right for a vertical area), and the
  loops of one node are turned against each other (`LOOP_TURN`).
- Complete bundling keeps the loops of one node as separate routes, each selectable.
- Tree areas ignore loops in the tree skeleton (a loop is no second parent); the router draws them.

## Consequences

- Tests: straight with a loop, a loop on a circle (two distinct ports), no bundling of loops, and several loops in `none`, organic, tree and
  radial areas.
- No fingerprint of the 19 sweep configurations changes (none of them has a loop); fixtures with loops are not part of that check.
- Open: loop room is not reserved in the placement (the footprint of a node with loops is not enlarged); a visible switch for the rule
  would need a second variant first.
