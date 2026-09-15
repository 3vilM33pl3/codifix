# Selected batch journal

These are synthetic selected records, not a random fleet sample. The values in
the second column are the complete raw readings for that selected batch, in
micro-units. The last column is the integer milli-unit display emitted by v5.

| Batch | Original readings | v5 display |
| --- | --- | --- |
| a | 600, 600, 600 | 0 |
| b | -600, -600, -600 | 0 |
| c | 1,500, -500 | 1 |
| d | 500 | 0 |
| e | -500 | 0 |
| f | 1,000, 2,000 | 3 |

Every row in this table has retained raw readings and a known v5 export revision.
Older batch z has only a display of 2 milli-units, with neither raw readings nor
a raw count. Its exporter revision is unknown. No missing data has been filled
with zero. There is no sensor calibration record or complete historical batch
inventory attached to this journal.
