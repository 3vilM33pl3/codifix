# Held-out coding family: decimal register — DRAFT

Five modules, two public starter tests and three stages operate on fictional
measurement units. This is exact arithmetic, not money, billing or persistence.
Keep all references/probes outside development calibration and agent workspaces.

1. Three probes cover normalized input, rejection of floating-point syntax and
   excess precision, exact large values and both i64 milliunit extrema.
2. Three probes require exact small additions to large values, positive/negative
   overflow rejection, and unchanged state after an invalid operand.
3. Four probes cover signed half ties, repeated explicit rounding, wide products,
   overflow and bounded factors. A rounded division followed by multiplication
   need not recover the original; the contract rounds each command once.

Reference parsing, register and scale modules must pass all ten probes and the
cumulative sequence. Starter and no-op controls must fail every stage. Accept
any contract-compliant implementation, not source strings. Labels stay draft
until approved; deterministic verification is not a held-out model run.
