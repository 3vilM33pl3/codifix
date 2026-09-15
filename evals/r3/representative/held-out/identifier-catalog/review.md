# Held-out coding family: identifier catalog — DRAFT

Five modules and two public tests define a small explicit N2 rule table, not a
complete Unicode library or authentication system. Original labels stay distinct
from normalized lookup keys. This family remains excluded from development.

1. Three probes cover fullwidth Latin and acute-e composition, preserved original
   display spelling, distinct plain/acute e, allowed alphabet and key length.
2. Three probes require exact replay, ID conflict, unique normalized ownership
   and non-mutating invalid input. Equal lookup keys do not authorize merging IDs.
3. Four probes require collision-safe rename, equivalent display spelling changes,
   release/reuse and unchanged missing/invalid operations. Earlier checks persist.

Ten private probes and reference key/catalog modules stay outside the agent
workspace. Reference/cumulative runs must pass; starter/no-op controls must fail
every stage. Accept any contract-compliant implementation. Draft labels require
later approval; deterministic fixture checks are not model calibration.
