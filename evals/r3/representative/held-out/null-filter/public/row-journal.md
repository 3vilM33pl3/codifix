# Selected relation contents

| Item project | ID | Active |
| --- | --- | --- |
| lab | 1 | yes |
| lab | 2 | yes |
| lab | 3 | yes |
| lab | 4 | no |
| game | 2 | yes |

| Completion project | Item ID |
| --- | --- |
| lab | 1 |
| lab | NULL |
| game | 2 |

The request addresses lab. v11 returns no rows. A disposable q27 run that filters
out NULL but does not correlate project returns only lab item 3. No other
completion facts exist in this selected fixture; a null completion is unresolved
import linkage. There is no inventory of all production pending work.
