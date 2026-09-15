# E3 exposure sample

At11:50 u17 in session s1 receives treatment using bucket42.
At11:55 u17 in session s2 receives control using bucket734.
At11:58 u88 receives treatment using its authenticated account bucket55.
At11:59 u91 receives control using its authenticated account bucket145.
At12:01 u91 receives treatment under rollout revision r2, still bucket145.

The first two requests used session-based assignment in client build c8; the
later three used the account-keyed server path. One anonymous request at12:02
was recorded as an experiment exposure after c8 substituted its session ID.
The sample has no completed outcome window or unique-account conversion report.

There are six exposure rows, not six independent authenticated participants.
Their assignment-path differences must be retained; silently relabeling earlier
exposures to what the account should have seen would falsify actual treatment.
