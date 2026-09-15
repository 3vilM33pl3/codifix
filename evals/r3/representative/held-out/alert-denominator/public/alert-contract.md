# Willow request alert M6

The release gate measures failed completed requests divided by ALL completed
requests for the selected service and fixed interval. Incomplete/in-flight work
is reported separately. Retries are distinct requests for this operational rate;
they are not distinct affected users. The gate threshold is strictly greater
than2%. An exactly2% request failure rate does not cross it.

Trace sampling keeps every failed request and only10% of successful requests.
The sampling policy is for diagnosis, not direct population-rate arithmetic.
The unsampled request counter is the primary rate source. If weighting sampled
successes, use the actual policy and report estimation uncertainty; never divide
sampled failures by sampled total and call it the population rate.

Counts must share service, interval and completion definition. Do not divide a
whole-day numerator by a five-minute denominator or mix pre/post-change windows.
User impact requires a separate deduplicated user/session dataset.
