# Supervisor v4 inspection

On shutdown, v4 removes the responder from discovery and immediately terminates
the process. It has no local admission gate or active-stream wait. Its final log
line says `drain complete` when discovery acknowledges the removal, not when
requests have completed. The deployment controller counts an HTTP liveness
response as both healthy and eligible for new routing.

The gateway retains routing snapshots for up to ten seconds in this fixture.
Existing streams hold their established connection independently of later
discovery changes. A proposed patch d18 merely shortens the snapshot lifetime;
another keeps readiness true while waiting for active streams. Neither closes
local admission or defines a bounded cancellation path. Neither is deployed.
