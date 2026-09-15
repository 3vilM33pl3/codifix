# Selected lookup records in candidate order

All records are active. The request is lab `service:api` with limit two.

| Order | Project | ID | Service | Code | Notes |
| --- | --- | --- | --- | --- | --- |
| 1 | game | g1 | api | E7 | remote incident |
| 2 | lab | l1 | capital | E8 | startup note |
| 3 | lab | l2 | API-gateway | E9 | checked route |
| 4 | lab | l3 | worker | E10 | investigate api timeout |
| 5 | lab | l4 | api_v2 | E11 | checked retry |

v12's first-two substring candidates are g1/l1; the final project filter leaves
l1. No result from l2/l4 reaches that limit. The sample is synthetic and contains
no raw sessions or hidden project content beyond this supplied table. It does
not measure overall retrieval precision across a real corpus.
