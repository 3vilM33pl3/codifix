# Held-out coding family: clocked circuit window — DRAFT

Five modules, two ordinary public tests and ten private black-box probes across
three cumulative stages. The contract precedes the probes and uses explicit
logical ticks; no network calls or real dependency health are measured.

1. Consecutive failure accounting, success reset, opening and token invalidation.
2. Single half-open probe, exact cooldown, failed probe and stale-result fencing.
3. Closed permit expiry/capacity, probe expiry based on original deadline, late
   polling and backwards-clock rejection without state changes.

The private reference must pass each cumulative stage; the unfinished starter
and no-op controls must fail. Probe timeout is a conservative admission policy,
not evidence that an external request failed or was canceled. This fixture is
not a distributed lease implementation. All labels remain draft and held out
from model calibration; related circuit knowledge stays in the held-out split.
