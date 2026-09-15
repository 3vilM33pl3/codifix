# Exporter v5 inspection

The currently deployed exporter reads each micro-unit integer separately. It
divides each by 1,000 using truncation toward zero, stores that milli-unit integer
in a temporary row and sums the temporary rows for the displayed batch number.
The original readings remain in the raw table for current batches.

A proposed patch p17 replaces each per-row division with nearest rounding,
including half-away-from-zero ties, but still sums the already rounded rows.
Another suggestion is to parse the original integers into binary floating point.
Neither suggestion changes where in the pipeline conversion occurs. Neither has
been approved or deployed. The report renderer prints the integer it receives;
it does not perform another conversion.

The accumulator currently has no explicit checked-overflow outcome. Inspection
did not exercise its maximum range, so it establishes a missing safeguard, not
an observed overflow incident.
