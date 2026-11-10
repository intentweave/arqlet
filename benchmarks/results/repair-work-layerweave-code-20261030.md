# Where the repair search spends its work: layerweave-code, 2026-10-30

Machine: Apple M3, 8 logical CPUs, macOS 25.0.0, release profile, revision 0bcfd56 plus uncommitted changes, load about 5 (not idle).
`bench --graph layerweave-code --limit 100 --threads 1 --repair-work 1`. Grid pops of the refinement, attributed to the edge whose route the
repair was working on (per route id). Raw per-edge data: `repair-work-layerweave-code-20261030.json`.

Total pops: 2251396 (the refinement counter: 2251396).

| class | edges | edges with repair search | pops | share | length, sum |
|---|---:|---:|---:|---:|---:|
| not constructed: CrossesArea | 71 | 53 | 2107218 | 93.6 % | 173840 |
| not constructed: ContainerEndpoint | 15 | 7 | 131410 | 5.8 % | 16968 |
| constructed | 30 | 3 | 4791 | 0.2 % | 8441 |
| not constructed: NoFreeTrack | 18 | 3 | 4680 | 0.2 % | 10745 |
| not constructed: PortSide | 47 | 1 | 3297 | 0.1 % | 28525 |
| not constructed: SelfLoop | 3 | 0 | 0 | 0.0 % | 436 |

Largest single edges:

| edge | pops | bends | length | class |
|---|---:|---:|---:|---|
| e901 | 105319 | 9 | 4554 | CrossesArea |
| e920 | 95568 | 10 | 5016 | CrossesArea |
| e922 | 94773 | 14 | 4938 | CrossesArea |
| e903 | 94755 | 10 | 4399 | CrossesArea |
| e790 | 90468 | 14 | 4622 | CrossesArea |
| e915 | 89135 | 13 | 4693 | CrossesArea |
| e888 | 80464 | 6 | 3416 | CrossesArea |
| e887 | 80017 | 6 | 3299 | CrossesArea |

Reading: 94 % of the repair work sits on edges that connect two different containers (CrossesArea). They are long (3,300–6,100
units) and bend 4 to 16 times. The constructed edges account for 4,791 pops in total. The pre-computation therefore has to cover
edges between containers, which the constructor does not yet handle (it needs the container boundary as an endpoint, or the route
split per area), before the repair work can drop.
