# Recorded profiles and generations

All model names and vectors in this fixture are synthetic. The `spruce-latest`
alias resolved to spruce-r4 yesterday and spruce-r5 today.

| Profile | Model revision | Dimensions | Normalization | Prefix rule | Metric |
| --- | --- | --- | --- | --- | --- |
| pA | spruce-r4 | 3 | n1 | role1 | cosine |
| pB | spruce-r5 | 3 | n1 | role1 | cosine |
| pC | spruce-r5 | 3 | n2 | role1 | cosine |
| pD | spruce-r5 | 3 | n2 | role2 | cosine |
| pE | spruce-r5 | 3 | n2 | role2 | dot |

Generation g17 was created with pA. Its manifest retains all pA fields, although
the current service configuration contains only the alias and dimension. The
new query process resolved the alias to r5 and currently emits pB queries.
Proposed generation g18 is intended to use pB over corpus snapshot s12. It is
not active and has not passed coverage review. No profile in this table is
declared interchangeable with another.
