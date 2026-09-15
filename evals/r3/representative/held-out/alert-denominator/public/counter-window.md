# Unsampled counters, service willow-api

Window W1,10:00 through10:05 UTC:10,000 completed requests, of which200 failed.
Forty additional requests were still in flight at10:05 and excluded from both
completed counts. The counter definition includes client retries as requests.

Window W2,10:05 through10:10 UTC:8,000 completed requests, of which240 failed.
Its completed failure rate uses its own denominator, not W1's10,000. The combined
ten-minute interval has18,000 completed requests and440 failures; a combined
rate must be computed from those totals, not an unweighted mean of percentages.

These are complete synthetic request counts for the two windows, not user counts.
There is no per-user join, unique-customer count or cause-of-failure classification
in this counter export. W2 followed a release, but no randomized control exists.
