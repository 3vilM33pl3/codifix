# Area supervisor

## Purpose

This view is local and instance-wide, not scoped by the project selector. It
requires global memory_read. Inspection never enrolls peers, changes policy or
calls a model. Explicit policy controls require separate global administration.
An enabled policy is not evidence of an active agent: this build reports full
Pi/model execution not integrated, separately from deterministic observations.

## Layout

The panel shows the durable area identity, policy revision, exact budget totals
and committed metadata history. It never shows private prompts or source text.

## Controls

- `v`: open project-private proposals. In this pane, `q` opens questions (not quit).
- Questions: `n` starts typing one exact question; Enter prepares disclosure consent.
  Enter again confirms a potentially paid attempt; Escape cancels an unsent preview.
  After uncertain submission, Enter retries the **same** request and `r` inspects
  it. No new ID is generated. `l` lists your retained questions and Enter opens
  status or an available cited answer. No history or files are attached.
- A loopback single-user question client exchanges authenticated owner access for
  a revocable in-memory session. No session cookies are printed or written to disk.

- `r`: refresh current history page, clearing stale state before the request.
- `PageDown`: read the next committed history page, if available.
- `Home`: return to the first history page.
- `j` / `k` or down/up arrows: scroll.
- `p` / `e`: prepare pause/resume if this caller may control the exact policy.
- `Enter`: confirm the displayed request, or retry that same ID after failure.
- `c`: close confirmation and inspect fresh history; it does not undo a sent request.
- `h` / `Esc`: shared help controls; Tab/Shift+Tab change tabs.

## Workflows

The journal contains IDs, event codes and timestamps, not private questions,
sources, prompts or proposal contents. Budgets show UTC accounting days and exact
millionths of USD; uncertain provider charges remain reserved. Read-only,
unconfigured, forbidden, offline and identity-mismatch states remain distinct.
Controls require current global `system_admin`. They never create an identity,
grant disclosure permission, reset budgets or launch Pi. The exact instance and
policy revision are shown before confirmation. Pausing prevents future work under
the policy but cannot recall already transmitted bytes. An uncertain request ID
is retained until confirmed or explicitly dismissed for history inspection.
