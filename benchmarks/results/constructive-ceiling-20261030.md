# Constructive layered routes, ceiling experiments (plan section 4a), 2026-10-30

Two ceiling experiments on the step 2 construction, measured on the import drawing with
`bench --constructive 1` (import only). Neither runs in import or refinement: `constructive::construct` and `flow_ports`
are called only by that bench mode. Results per dataset: `constructive-ceiling-20261030.jsonl`.

- **orientations**: an `auto` or allowed area takes the orientation the layout chose for it (`area_orientations`).
- **flowPorts**: every constructable edge gets its ports on the flow-facing sides, spread evenly over each side. This
  replaces the automatic ports, so it is valid only for drawings without explicit ports (none of these datasets has any).
- **both**: the two together.

Machine: Apple M3, 8 logical CPUs, macOS 25.0.0, release profile, rustc 1.99.0. Revision: 0bcfd56 plus uncommitted
changes. Load average about 5 (not idle); reps 1.

## Totals (17 datasets, 1101 edges)

| variant | constructed | NoFreeTrack | PortSide | NotForward | NoFixedFlow | CrossesArea | ContainerEndpoint |
|---|---:|---:|---:|---:|---:|---:|---:|
| baseline | 191 | 19 | 144 | 0 | 510 | 164 | 67 |
| orientations | 191 | 19 | 144 | 0 | 510 | 164 | 67 |
| flowPorts | 222 | 111 | 0 | 21 | 510 | 164 | 67 |
| both | 222 | 111 | 0 | 21 | 510 | 164 | 67 |

## Reading

- **Orientations do nothing here.** The areas that report `NoFixedFlow` are not `auto` layered areas: in nested-tree-radial
  they are `rl` and `bt` areas (57 candidate edges, reversed flows the constructor does not support yet) and `auto` areas
  with a radial strategy (17); in mixed-layouts the `auto` areas use the organic strategy. The hypothesis does not hold for
  these datasets.
- **Flow-facing ports remove the port-side block and expose the track block.** PortSide falls from 144 to 0, and 31 more
  edges construct (191 → 222). But NoFreeTrack rises from 19 to 111, and NotForward (backward edges, which the port rule had
  hidden) appears at 21. With the sides fixed, the tracks collide: the corridor assignment does not yet consider the
  slots that the ports took. This is what steps 3 and 4 have to solve, not only the port rule.
- **Ceiling.** A flow-facing port rule alone does not unlock the 144 port-side edges. Of the 113 that leave PortSide, 31
  construct, 92 move to NoFreeTrack and 21 to NotForward. The bound on what the layered rules can reach is therefore set by
  the track assignment as well.
- **CrossesArea is untouched** (164 in every variant). It needs routes that cross container borders, which none of the
  variants does.
