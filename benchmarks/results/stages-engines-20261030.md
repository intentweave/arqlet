# Phase ablation: dagre and elkjs on the comparison views, 2026-10-30

Machine: Apple M3, 8 logical CPUs, macOS 25.0.0, Node 24.21.0; dagre 3.1.1, elkjs 0.12.0 (pinned). Load about 5 before the runs,
machine quiet but not idle. Views: `--limit 150`, same as the comparison. Each time is the median of three runs of the engine's own
layout call (graph construction is outside the timer).

Method: a phase is switched to a cheaper method and the whole layout is timed again. The difference to the default is the phase's cost on
that input. It is an approximation: a cheaper phase can change the work of later phases too. Neither engine reports its phases, and the
phases are not timed inside the engine.

## Inputs

| dataset | nodes | containers | leaves | drawn edges | edges to containers (dropped) |
|---|---:|---:|---:|---:|---:|
| activity-order | 31 | 4 | 27 | 40 | 0 |
| bpmn-pools | 26 | 5 | 21 | 24 | 0 |
| cloud-architecture | 34 | 14 | 20 | 32 | 0 |
| er-diagram | 44 | 12 | 32 | 12 | 12 |
| layerweave-code | 182 | 32 | 150 | 303 | 15 |
| layerweave-pipeline | 32 | 6 | 26 | 33 | 0 |
| mixed-layouts | 18 | 3 | 15 | 16 | 0 |
| nested-tree-radial | 160 | 35 | 125 | 121 | 21 |
| network-vlan | 37 | 5 | 32 | 47 | 0 |
| organic-100 | 100 | 0 | 100 | 119 | 0 |
| organic-200 | 150 | 0 | 150 | 177 | 0 |
| organic-25 | 25 | 0 | 25 | 29 | 0 |
| organic-50 | 50 | 0 | 50 | 59 | 0 |
| petri-net | 24 | 0 | 24 | 41 | 0 |
| state-order | 11 | 1 | 10 | 14 | 3 |
| tree-100 | 100 | 0 | 100 | 99 | 0 |
| uml-class | 43 | 20 | 23 | 24 | 24 |

## dagre (ms; the difference column is variant minus default)

| dataset | flat default | tight-tree ranker | Δ | longest-path ranker | Δ | no order heuristic | Δ | compound default | compound longest-path Δ | compound no order heuristic Δ |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| activity-order | 5.3 | 4.3 | -1.0 | 4.8 | -0.6 | 4.1 | -1.2 | 12.1 | -1.4 | -0.8 |
| bpmn-pools | 4.2 | 4.0 | -0.2 | 3.9 | -0.3 | 3.2 | -1.0 | 15.7 | -0.9 | -0.8 |
| cloud-architecture | 6.1 | 5.3 | -0.8 | 7.8 | +1.7 | 5.2 | -0.9 | 22.1 | +1.3 | -2.9 |
| er-diagram | 1.5 | 0.9 | -0.5 | 0.7 | -0.8 | 1.0 | -0.4 | 5.6 | -1.3 | -0.9 |
| layerweave-code | 99.1 | 80.7 | -18.4 | 165.6 | +66.5 | 93.6 | -5.5 | 286.7 | – | +31.0 |
| layerweave-pipeline | 6.4 | 5.6 | -0.8 | 5.7 | -0.7 | 5.8 | -0.7 | 15.0 | +2.5 | -1.6 |
| mixed-layouts | 2.5 | 1.9 | -0.6 | 1.7 | -0.8 | 1.9 | -0.6 | 5.8 | +0.7 | -1.2 |
| nested-tree-radial | 11.4 | 9.5 | -1.9 | 8.8 | -2.6 | 10.2 | -1.2 | 53.4 | -10.0 | -0.3 |
| network-vlan | 8.2 | 7.4 | -0.8 | 11.0 | +2.8 | 7.9 | -0.3 | 18.8 | -2.0 | -1.3 |
| organic-100 | 23.4 | 17.7 | -5.7 | 16.9 | -6.5 | 16.3 | -7.1 | 16.7 | -0.6 | +0.7 |
| organic-200 | 37.1 | 27.3 | -9.8 | 40.1 | +3.0 | 26.4 | -10.6 | 31.8 | +10.9 | -0.3 |
| organic-25 | 4.1 | 3.8 | -0.3 | 3.3 | -0.8 | 3.6 | -0.5 | 3.5 | -0.3 | -0.1 |
| organic-50 | 8.4 | 7.5 | -1.0 | 7.5 | -0.9 | 7.4 | -1.0 | 7.5 | -0.1 | +0.1 |
| petri-net | 10.7 | 10.6 | -0.1 | 12.7 | +1.9 | 10.9 | +0.2 | 10.7 | +2.0 | -1.1 |
| state-order | 1.9 | 1.2 | -0.7 | 1.2 | -0.7 | 1.2 | -0.7 | 2.3 | -0.3 | -0.4 |
| tree-100 | 9.6 | 8.1 | -1.5 | 8.7 | -0.9 | 8.5 | -1.1 | 8.6 | -1.1 | -1.0 |
| uml-class | 1.7 | 0.8 | -0.8 | 0.7 | -1.0 | 0.9 | -0.8 | 9.2 | -2.2 | -0.2 |

## elkjs (ms; the difference column is variant minus default)

| dataset | default | polyline routing Δ | no crossing minimization Δ | simple placement Δ | longest-path layering Δ |
|---|---:|---:|---:|---:|---:|
| activity-order | 32.3 | -10.8 | – | -10.9 | -10.0 |
| bpmn-pools | 24.4 | -7.9 | – | -10.5 | -10.2 |
| cloud-architecture | 38.7 | -9.7 | – | -13.4 | -11.7 |
| er-diagram | 17.5 | -7.1 | – | -7.6 | -5.9 |
| layerweave-code | 224.3 | -27.9 | – | -30.6 | -15.4 |
| layerweave-pipeline | 39.6 | -22.1 | – | -22.6 | -22.1 |
| mixed-layouts | 14.3 | -5.1 | – | -6.5 | -5.5 |
| nested-tree-radial | 35.0 | -10.7 | – | -13.5 | -12.7 |
| network-vlan | 29.5 | -8.2 | – | -10.5 | -9.2 |
| organic-100 | 30.5 | -12.2 | -14.4 | -15.0 | -11.3 |
| organic-200 | 50.7 | -16.5 | -29.4 | -21.8 | -21.2 |
| organic-25 | 11.8 | -3.9 | -4.1 | -4.9 | -4.5 |
| organic-50 | 21.7 | -8.0 | -11.0 | -11.0 | -10.7 |
| petri-net | 19.2 | -7.1 | -11.0 | -8.6 | -7.4 |
| state-order | 11.0 | -3.3 | – | -4.2 | -3.3 |
| tree-100 | 23.8 | -8.0 | -10.4 | -9.9 | -7.3 |
| uml-class | 24.8 | -7.2 | – | -8.9 | -9.1 |

## Failed ablations (findings)

- elkjs without crossing minimization: `UnsupportedGraphException` on every hierarchical view (11 datasets). The hierarchy-aware
  processor needs LAYER_SWEEP in the child graphs, so this phase cannot be switched off there. Only the flat views have a value.
- dagre compound with the longest-path ranker: "Not possible to find intersection inside of the rectangle" on layerweave-code.

## Reading (for the question "why are they faster?")

- The import of Layerweave on the layered sets (3–20 ms) is in the same range as ELK's whole layout (11–39 ms). The gap is in the
  refinement: 31–130 ms on the same sets, 42 s on layerweave-code (see `compare-20261030.md`).
- Inside ELK, the orthogonal routing is 12–56 % of the layout time on the layered sets (median about 32 %; polyline Δ over default). Node
  placement with NETWORK_SIMPLEX and the layering phase together are about as large, so ELK spends its time on a fixed sequence of
  phases, each run once. Its routing is one pass per edge in channels.
- dagre has no edge routing (straight edges). Its ranker is the only large choice: `tight-tree` is 5–47 % faster on the layered
  sets (10–20 % on most) and `longest-path` is slower on layerweave-code (+66 ms). The order heuristic costs about 1 ms.
- Layerweave's refinement spends its time in the routing search over candidate tracks (`routing` in `stage-sweep-20261030.md`:
  120 ms of 138 ms on petri-net, 6.8 s of 7.3 s on layerweave-code). That is the part that ELK and dagre do not have.
- These are the costs of the phases, measured by switching them off. ELK and dagre do not report their own phases, so the
  deltas are approximations, and they do not add up to the default time.
