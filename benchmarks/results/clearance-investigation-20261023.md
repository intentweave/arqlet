# Where the remaining cost of clearance 9 comes from (2026-10-23)

Question: clearance 9 costs +287 routed crossings over default (9,543 against 9,256) after track assignment. Which views carry it, and which mechanism causes it?

## The worst views (clearance 9 against default)

| View | Crossings | Length |
| --- | --- | --- |
| synthetic-2k 100 | +222 | +5,078 |
| intentweave-export 100 | +59 | +95,916 |
| activity-order 100 | +47 (the ordering candidate lost to routing-only) | -1,876 |
| layerweave-code 50 | +30 | +2,341 |
| petri-net 100 | +27 | +992 |

## intentweave-export: a detour, not a crossing count

Length is the cost here. 81 edges each gain 2,200 to 2,440 units, 99 percent of the total change. The clearance-9 route of one edge drops to the bottom of the drawing (y = 2,781 of 2,880) and comes back, where the default route is a short step at y = 1,536. Node positions are unchanged in this view, so the change is in the router's track choice, not in the placement.

## Hypotheses tested

| Change | Result (20 views, clearance 9) | Verdict |
| --- | --- | --- |
| Placement gap 2 x (clearance + track) = 30 instead of 2 x clearance + track = 24 | 10,687 crossings (against 9,543); intentweave-export 5,338 | rejected |
| Obstacles inflated for leaves only, containers not inflated | 10,047 crossings; intentweave-export still 544k length; layerweave-code 50 much worse | rejected (containers keep foreign routes out; not the detour's cause) |
| Corridor centres (existing option) on top of the track offset | 9,478 crossings, zero contacts | kept off by default |

Raw rows: `clearance-investigation-gap30-20261023.jsonl`, `clearance-investigation-leaves-only-20261023.jsonl`.

## What is not yet known

The straight track at y = 1,536 is not covered by a leaf band in the x-range of the detoured edge, so the router's candidate set for that edge is the cause, not the obstacles alone. The next step is a trace of the candidate tracks the legacy search evaluates for that edge at the first vertical choice (the default takes x = 1,408; clearance 9 takes x = 1,402 and then goes down). That trace is a bounded debugging step, not a heuristic change, and it decides whether the fix is in candidate generation (which tracks are offered) or in scoring (which is chosen).

## Conclusion for the thread

The remaining +3 percent is not a single effect. The length detours in intentweave-export are the largest single cost; the crossing differences are spread across views. Neither placement space nor the container rule explains the detours, so the next change should follow the trace, not another global adjustment.
