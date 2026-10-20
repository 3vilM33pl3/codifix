# Background jobs

These commands describe current `main` (`3.0.0`). Use the
[Web UI](https://codifix.dev/docs/web-ui) for the equivalent inspection workflow.

Curation returns after its mutations, timeline events and enrichment intents
commit together. It does not wait for embedding or consolidation. The service
recovers pending work from PostgreSQL after restart; polling is authoritative.

```bash
codifix jobs --project example-project
codifix jobs --project example-project list --status blocked --limit 20
codifix jobs --project example-project inspect JOB_UUID
codifix jobs --project example-project --json inspect JOB_UUID
codifix jobs --project example-project retry JOB_UUID --request-id REQUEST_UUID --yes
```

Use the returned `next_after` UUID with `list --after UUID` for another bounded
page. Inspect shows dependencies, sibling stages, attempts, next eligibility,
sanitized failure codes and explicit skip reasons. Queue counts and oldest-ready
age are project-scoped; the worker timestamp reports the serving process's poll.
Raw prompts, source contents and provider responses are never included.

For timeline recovery, `GET /v1/projects/{slug}/activities?after_seq=0&limit=100`
returns ascending commit order and `cursor.next_after`, `has_more`,
`retention_gap` and `deleted_through`. Save the cursor only after processing the
page. A retention gap requires snapshot recovery, not silent cursor advancement.
Cursor mode accepts 1–200 items and cannot be combined with time/kind filters;
omitting `after_seq` preserves the legacy newest-first activity view.

Retry only blocked or failed jobs after addressing their cause. It needs
`LoopsRun`, the operation's execution permissions, and the original execution
identity's current authority. Read-only services reject retries. Completed stages
and original inputs are retained; a new bounded retry generation is opened.
Running and successful jobs cannot be restarted here. Retry cannot transfer
authority from a revoked requester to an operator or bypass configuration policy.

Pre-cutover enrichment has no reconstructed historical jobs. Database operators
can explicitly request fresh, version-pinned repair work using the
[R1 rollout and repair procedure](https://github.com/3vilM33pl3/codifix/blob/main/docs/developer/architecture/r1-rollout.md).

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
Inspection replaces the list; Escape returns to it. The confirmation names the
project, job and stable request UUID. Refresh to see
subsequent status changes. Read-only services disable retry.
