# Linear project tracking

Enable automatic issue tracking from the Project tab with `L`, or use the CLI
with the same area and project that you use for normal Codifix work.

Set `AREA` and `PROJECT` to the enrolled area and project slug. Check the mapping in the status output before enabling tracking.

```sh
codifix --area "$AREA" linear status --project "$PROJECT"
codifix --area "$AREA" linear enable --project "$PROJECT"
codifix --area "$AREA" linear status --project "$PROJECT"
```

To advance one pending event manually, run:

```sh
codifix --area "$AREA" linear sync --project "$PROJECT"
```

The operator installs the resident companion's approved project binding before
enabling tracking. The central companion retains the Linear credential. The
local switch delegates only to the enrolled project and shows the mapped
project, the last synchronization result and the last verified issue link.
To pause queued events while preserving their issue links, run
`codifix --area "$AREA" linear disable --project "$PROJECT"`.
A request the companion already accepted can finish.

Approved plans and explicit implementation task starts enqueue tracking after
Codifix records the start. Pass `--skip-linear` to `checkpoint start-task` or
`checkpoint start-execution` for a subtask already represented by a parent
issue. Read-only commands and dry runs do not queue Linear writes. Supply a
short shareable work description with `--linear-summary` at checkpoint start.
The original prompt and plan stay in Codifix.

Verified plan completion closes the same issue. For direct work, record the
implementation with `codifix remember --title … --test-passed …` and the same
task title as at start. If you supplied a custom thread key when starting, pass
it as `--linear-thread-key` to `remember`. Failed or absent verification
does not close the issue. To report a blocker, run:

```sh
codifix --area "$AREA" linear block --project "$PROJECT" --thread-key my-task --reason "Waiting for provider access"
```

The resident companion periodically drains pending events. A service outage can
delay synchronization while Codifix task work continues. The local status
shows pending work or a retry. A proposal requiring review appears as
**Needs review in the companion**.

To draft an individual issue with wording approval, use [Manual Linear review](https://codifix.dev/docs/specialists/linear-review). Operators can inspect the [3.1.0 resident companion enrollment guide](https://github.com/3vilM33pl3/codifix/blob/v3.1.0/docs/user/linear-homelab-enrollment.md) for the private peer binding.
