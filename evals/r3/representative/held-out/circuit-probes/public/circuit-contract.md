# Spruce dependency guard G5

This fixture's guard protects the shared invoice-render dependency across API
replicas. Three consecutive eligible dependency failures open it for ten seconds.
Client validation failures and local permission denials do not count as dependency
failures. A successful eligible dependency request while closed resets the
consecutive-failure count. Service liveness is a separate cached response.

After cooldown, admit one half-open probe across the participating replicas.
Other dependency work receives explicit temporarily-unavailable until that probe
settles. A valid successful current probe closes the guard; a valid failure
reopens it for another ten seconds. Passage of time alone does not prove recovery.

Every admitted probe carries the guard generation and claim identity. Completion
can change guard state only if both still match current state. A stale success
cannot close a newer open generation. These are the explicit product guard rules,
not a universal circuit-breaker implementation requirement.
