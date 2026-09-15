# Selected queue and trace records

| Item | Publisher carrier intended | Consumer ambient context | Queue receipt |
| --- | --- | --- | --- |
| j1 | trace tA, publishing span p1 | trace tOld, span pOld | completed |
| j2 | trace tB, publishing span p2 | none | completed |

j1 exports a processing span under tOld. j2's processing span is unsampled and
absent from the export viewer, but its durable job receipt records completion.
j1 has a first processing attempt that failed and a second that completed;
both currently appear under processing span s7. The receipt journal distinguishes
those attempts and does not claim two successful business executions.

There is no complete end-to-end clock alignment or latency measurement in this
sample. No j1 business payload was delivered to the tOld request in the recorded
receipts; incorrect diagnostic association is not itself such a delivery record.
