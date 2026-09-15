# Guard timeline, seconds from fixture start

t=0.0,0.1,0.2: three eligible dependency failures occur consecutively. The guard
opens generation7 at0.2 with cooldown until10.2.
t=10.3: replica east admits probe p1 carrying generation7.
t=10.4: replica west also admits p2 carrying generation7 while p1 is unsettled.
t=10.5: cached API liveness responds200 without contacting the dependency.
t=11.0: p1 returns an eligible failure; generation8 opens until21.0.
t=12.0: p2 returns success; west's current implementation sets its guard closed.

No generation8 successful probe is recorded. The two probes were concurrent
logical claims, not duplicate log lines for one request. Later customer traffic
counts and a complete dependency recovery measurement are absent.
