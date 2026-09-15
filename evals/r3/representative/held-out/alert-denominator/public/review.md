# Willow metric review

Keep the trace panel for diagnosis and label its selection policy. Use aligned
unsampled completed counters for release decisions. Show in-flight work separately;
do not silently treat it as success or failure. For aggregate windows, sum counts
before dividing. Report raw numerator/denominator alongside rounded percentages.

The before/after windows differ in both traffic volume and failure count. They
can describe an observed change, but cannot alone prove the release caused it.
Investigate request types, dependency failures and other concurrent changes.

The packet cannot establish unique users harmed, whether retries all eventually
succeeded, a lost-revenue total or a causal treatment effect. The gate is a
defined operational threshold, not a complete customer-impact measurement.
