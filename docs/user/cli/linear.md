# Linear project tracking

Enable automatic issue tracking from the Project tab with `L`, or use the CLI
with the same area and project that you use for normal Codifix work.

```sh
codifix --area workshop linear status --project codifix
codifix --area workshop linear enable --project codifix
codifix --area workshop linear sync --project codifix
codifix --area workshop linear disable --project codifix
```

The operator installs the resident companion's approved project binding before
enabling tracking. The central companion retains the Linear credential. The
local switch delegates only to the enrolled project and shows the mapped
project, the last synchronization result and the last verified issue link.
Disabling tracking pauses queued events and preserves their issue links.
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
codifix --area workshop linear block --project codifix --thread-key my-task --reason "Waiting for provider access"
```

The resident companion periodically drains pending events. A service outage can
delay synchronization while Codifix task work continues. The local status
shows pending work or a retry. A proposal requiring review appears as
**Needs review in the companion**.

See [resident Linear enrollment](../linear-homelab-enrollment.md) for the
private companion path.
