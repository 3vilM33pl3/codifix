# W7 repair proposal — not released

Replace the file-object-only observer with directory hints and bounded periodic
reconciliation of the selected name. Reopen and fully validate the candidate,
compare its digest and atomically install valid changed bytes. Record applied
digest/generation plus sanitized failure state. Keep the old valid configuration
when the new candidate is invalid, missing or unreadable. A heartbeat alone must
not assert current configuration adoption.

Test atomic replacement, equal-size/equal-timestamp changed bytes, duplicate
hints, invalid candidates, missing names, overflow and unrelated sibling edits.
After overflow reconcile current state; do not fabricate the missed event order.
An accepted unchanged candidate is a no-op, not an extra generation. A sibling
edit may wake reconciliation but must not become policy content.

Approval and deployment remain pending. A restart may reopen the current name,
but it is not a general fix for the observer. In the journal's final state a
restart must not turn invalid dC into an accepted policy. No proposal authorizes
discarding validation to make a reload succeed.
