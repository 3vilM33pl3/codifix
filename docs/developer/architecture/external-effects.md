# External-effect journal (R1)

An external effect has a committed intent before execution and a separate
observed outcome. A failed PostgreSQL acknowledgement does not roll back a file,
provider request or service restart. The private `external_effects` and
`external_effect_outcomes` tables contain bounded resource locators, SHA-256
fingerprints and trusted actor IDs, not configuration contents or credentials.
They are not project timeline or Jobs API payloads.

## Connected file changes

Embedding activation/creation, LLM-audit configuration and project replacement
policy use this journal. Configuration edits serialize by normalized file path
across service processes, read the previous file after taking that lock, then
write a uniquely named temporary file, sync it, rename it and sync the directory.
No database transaction spans the filesystem write. A dedicated connection
closes on cancellation, releasing its session lock rather than returning a
locked connection to the pool.

If the file changed but the outcome transaction fails, the service retains the
observed in-memory setting and returns an explicit applied-but-unconfirmed
error with the operation ID. It does not revert the runtime setting while
pretending the file was reverted. Environment-only embedding settings remain
ephemeral and warn that they will not survive restart.

Project policy writes require the registered repository root and reject a
policy-file symlink outside that root. The journal does not turn untrusted
filesystem owners into a security boundary: administrators with host access
can still modify files outside the service.

## Provider requests and watcher restarts

Strict-JSON LLM calls and local/remote watcher restart requests record an intent
before sending the request. Successful responses confirm that operation; errors
or cancellation leave an uncertain outcome, not a claim that the remote action
was undone. The journal stores a request fingerprint, purpose or watcher ID and
service identity, never the prompt, credentials or remote response body.

The fake-service test observes the committed intent before responding and
demonstrates cancellation followed by reconciliation without another request.
Watcher health stays stale after an ambiguous restart. Subsequent watchdog
policy decisions are new attempts, not replay of an old journal entry.

## Recovery and operator inspection

A primary, writable service scans its own host's pending intents every 30
seconds, independently of the background-job enablement setting. Intents become
eligible after ten minutes. Database reconnect resumes scanning. Matching file
bytes confirm an interrupted write; mismatched or inaccessible bytes leave an
explicit uncertain outcome. Recovery never repeats a provider call, restart or
repository operation merely because its receipt is missing.

Operators may inspect the private journal through their protected database
administration connection. Filter by exact operation UUID before inspecting a
resource locator; do not publish these rows in project exports or logs. For an
uncertain file outcome, compare the actual configuration with the intended
setting and issue a fresh, explicitly chosen configuration change if needed.
There is no automatic overwrite of the current file with an older request.

A failed local write may leave an owner-only `.memory-effect-<UUID>.tmp` file in
the target directory. Inspect the exact journal entry and target before removing
that temporary file. It may contain private configuration. Do not use a broad
cleanup command or treat it as the authoritative configuration.

Other external-effect producers and interrupted loop/provider recovery remain
tracked in the [R1 inventory](../../reviews/federation-first/r1-coverage.md).
This slice is not the completed R1 gate or a disaster-recovery demonstration.
