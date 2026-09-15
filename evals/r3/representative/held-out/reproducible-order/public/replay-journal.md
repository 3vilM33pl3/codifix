# Selected arithmetic replay

Both runs use selection seed 47 and identical source/config/input digests. The
selected contributions are a=1000, b=-1000, c=1 in the N14 three-significant-digit
arithmetic. There is no random model invocation in this diagnostic.

| Run | Worker completion order | Recorded final score |
| --- | --- | --- |
| r1 | a,b,c | 1 |
| r2 | a,c,b | 0 |

In a separate replay, worker a sends its result twice; the current loop includes
both. No canonical N14 output has been published for that invalid replay. The
exact mathematical sum of the three distinct contributions is not stored in
the run metadata and no larger accuracy dataset is supplied.

Input selection is already frozen here. These observations do not show seed
drift, model changes or corpus writeback. They isolate a changing reduction
order and a distinct duplicate-message handling flaw.
