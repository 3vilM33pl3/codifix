# Finch score reduction — N14

This fictional score job freezes its inputs, model/configuration and semantic
time. A pseudorandom seed controls sample selection but does not control worker
completion order. Every contribution has a stable unique source ID. Duplicate
IDs must fail explicitly rather than silently contribute twice.

N14 defines deterministic reduction by ascending source ID using the stipulated
rounding arithmetic below, independent of scheduling or worker count. Retain
the selected input digest, reduction revision and resulting score. Freeze the
rule before measurement; do not choose an order after seeing a desired result.

For this fixture only, arithmetic keeps three significant decimal digits after
each addition, rounding ties away from zero. Start at zero. This is an explicit
finite-precision test model, not a claim about a particular hardware float. An
exact sum can differ from the N14 specified finite-precision fold. Consistent
output under N14 is separate from numerical accuracy or predictive quality.
