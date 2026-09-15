# Diagnostic trace panel W1

Retained traces:200 failures and980 successes, totaling1,180 records. The panel
displays200/1180 as16.95% after rounding to two decimal places. The collector's
policy retained all failures and10% of successes. The displayed ratio describes
this selected diagnostic trace sample, not every completed request.

The panel title says "request errors" but does not show its sampling denominator.
A second screenshot for W2 is missing. Absence of that screenshot does not mean
W2 has zero errors. One trace can correspond to one retry request by a user who
also made other attempts; the panel does not deduplicate people.
