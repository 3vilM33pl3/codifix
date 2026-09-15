# Interpretation limits

An export arithmetic error is distinct from a physical sensor error. This case
contains no calibration comparison, downstream decision log or monetary value
per unit. A rounded milli-unit display has finite precision even after Q6 is
implemented; it is not a claim that every individual reading was exact.

For integer raw totals with no overflow, Q6's final rounding differs from the
exact batch total by at most 500 micro-units. That is a bound on one converted
batch, not a bound on each sensor's accuracy or on an unlimited sum of separate
batch displays. Repeated per-reading rounding has a different accumulation
behavior and is not the Q6 contract.

These sources do not determine the number of affected historical batches, total
physical measurement error, customer losses, or a safe automatic correction for
batch z. Claims about those quantities require additional evidence.
