# Background jobs

Curation returns after its mutations, timeline events and enrichment intents
commit together. It does not wait for embedding or consolidation. The service
recovers pending work from PostgreSQL after restart; polling is authoritative.

```bash
memory jobs --project memory
memory jobs --project memory list --status blocked --limit 20
memory jobs --project memory inspect JOB_UUID
memory jobs --project memory --json inspect JOB_UUID
memory jobs --project memory retry JOB_UUID --request-id REQUEST_UUID --yes
```

Use the returned `next_after` UUID with `list --after UUID` for another bounded
page. Inspect shows dependencies, sibling stages, attempts, next eligibility,
sanitized failure codes and explicit skip reasons. Queue counts and oldest-ready
age are project-scoped; the worker timestamp reports the serving process's poll.
Raw prompts, source contents and provider responses are never included.

Retry only blocked or failed jobs after addressing their cause. It needs
`LoopsRun`, the operation's execution permissions, and the original execution
identity's current authority. Read-only services reject retries. Completed stages
and original inputs are retained; a new bounded retry generation is opened.
Running and successful jobs cannot be restarted here. Retry cannot transfer
authority from a revoked requester to an operator or bypass configuration policy.

Choose a request UUID once. If a reply is lost, repeat the **same** request UUID.
Provider execution is not exactly once: an uncertain attempt can be billed again.
Credential, quota, authorization and configuration failures need intervention;
transient failures retry automatically until the generation's limit is reached.

## Web and TUI

Web: **More → jobs**, filter or refresh, inspect a stage, **Review retry**, then
confirm. Switching projects clears the selected details and pending confirmation.
The same request ID is retained after an uncertain response.

TUI: open **Jobs**, press `r` to load, `s` to cycle filters, `j/k` to select,
`Enter` to inspect and `PgDn` for the next page. `R` prepares a failed/blocked
stage's retry; `y` confirms and `n` or Escape cancels. `d`/`PgUp` scroll details.
The confirmation names the project, job and stable request UUID. Refresh to see
subsequent status changes. Read-only services disable retry.
