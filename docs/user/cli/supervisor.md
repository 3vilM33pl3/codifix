# Supervisor inspection

`memory supervisor` reads this installation's metadata-only policy, budget and
ordered history. It requires **global `memory_read`**; a project administrator
cannot inspect other projects' instance-wide supervisor activity.

```sh
memory supervisor
memory supervisor --json
memory supervisor --after 123 --limit 50
```

The service uses an explicitly configured durable identity:

```toml
[supervisor]
instance_id = "REPLACE_WITH_DURABLE_INSTANCE_UUID"
area = "workshop"
```

This is an inactive configuration example, not an activation procedure. The
matching database binding must be provisioned separately through the reviewed
deployment path. Configuration does not create a binding, install policy, enroll
a participant, start Pi or allow model disclosure. The process ID from `/healthz`
is regenerated on startup and is **not** this durable identity.

`execution: not_integrated` is deliberate in this build. A stored enabled policy
is not a heartbeat or proof that an agent is running. Missing configuration,
unbound database identity, missing policy, paused/revoked policy, access denial,
offline storage and identity mismatch are separate states.

Amounts are exact millionths of USD, using UTC accounting days. Reserved amounts
include in-flight and uncertain attempts; do not subtract them from prior spend
or reset them after restart. Decimal-string cursors avoid integer precision loss.

Web: **More → Supervisor**. TUI: select **Supervisor**, press `r` to inspect,
`PageDown` for more history, `Home` for the first page and `j`/`k` to scroll.
The project selector does not change this view. Refresh clears old results before
making a request so denied/offline state is not shown as current healthy state.

These surfaces do not expose questions, private proposals, source text, evidence
bytes, model prompts or provider bodies. They never invoke a model or mutate
policy. Pause/resume, approvals, supervisor chat and actual scheduling remain
separate implementation work, not implied by this read-only panel.

## Disposable verification

The ignored service test `supervisor_real_browser_cli_rehearsal` creates its own
synthetic PostgreSQL database, paused policy and read-only service on an allocated
loopback port. It runs the real CLI, TUI through a PTY, and built Web UI through
Playwright, verifies their durable identity against storage, then confirms no
actions were created. It requires built Web assets, a TUI-enabled CLI, Chromium,
Python 3 with `pexpect`, and the explicitly configured disposable test database.
Set `MEMORY_SUPERVISOR_REHEARSAL_CLI` to that CLI's absolute path before running
the test with `--ignored --nocapture`. Never point it at an existing deployment.
