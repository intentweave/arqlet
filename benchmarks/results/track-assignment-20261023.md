# Track assignment for a required clearance (2026-10-23)

Change: the legacy router's candidate tracks are offset from each obstacle by `rail_offset`. The prototype uses 12, which was measured from the node boundary before inflation. With a required clearance the obstacle is already inflated by the clearance, so the offset is now the track spacing (6): the first free track sits just beyond the clearance band, not 12 further out. Default output is unchanged (`routing_params` keeps 12 without a required clearance).

Corpus (20 views, one thread): crossings, route length, and contacts at the preferred margin (12).

| Configuration | Routed crossings | Route length | Contacts at 12 | Kept input |
| --- | --- | --- | --- | --- |
| default (unchanged) | 9,256 | 1,561,247 | 2,567 | 0 |
| clearance 9, before this change | 9,712 | 1,689,295 | 0 | 0 |
| clearance 9, this change | 9,543 | 1,682,816 | 0 | 0 |
| clearance 12, before | 10,420 | 1,753,083 | 0 | 0 |
| clearance 12, this change | 10,193 | 1,739,163 | 0 | 0 |
| clearance 9 + `routingCorridors` (existing option, off by default) | 9,478 | 1,678,712 | 0 | 0 |
| clearance 9 + `routingTrackSeparation` (existing option) | 13,350 | 1,709,165 | 0 | 0 |

Reading:
- The change is small (about 1.7 percent fewer crossings at 9, 2.2 percent at 12) and keeps the contacts at zero. The remaining gap to default at clearance 9 is +287 crossings (+3.1 percent).
- Corridor centres help a little more on top (9,478), but they change routes in general and stay off by default.
- Track separation makes the result much worse with a clearance and is not used.
- Still open: the gap itself. The routing change does not address where the extra crossings come from; it only gives the search better tracks.
