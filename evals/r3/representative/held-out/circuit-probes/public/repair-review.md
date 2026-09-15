# G5 repair review

Claim the half-open slot atomically against shared generation/state. Bind result
acceptance to that claim and generation, and ignore stale completion as a state
transition while retaining sanitized attempt evidence. Keep ordinary API liveness
separate from dependency readiness and visible guard state.

Test two replicas racing for the one slot, failure followed by stale success,
success of the current probe, and excluded client/permission errors. A blanket
deny would suppress both harmful traffic and valid recovery, so the positive
current-probe path must still work. Verify state transitions, not just status200.

The packet cannot establish the dependency's actual recovery time, customer error
rate or optimal cooldown length. Ten seconds is the configured policy, not a
measurement of recovery or proof that future calls will succeed.
