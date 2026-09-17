# Area supervisor

This view is local and instance-wide, not scoped by the project selector. It
requires global memory_read. It never enrolls peers, changes policy or calls a
model. An enabled policy is not evidence of an active worker: this build reports
execution not integrated.

- `r`: refresh current history page, clearing stale state before the request.
- `PageDown`: read the next committed history page, if available.
- `Home`: return to the first history page.
- `j` / `k` or down/up arrows: scroll.
- `h` / `Esc`: shared help controls; Tab/Shift+Tab change tabs.

The journal contains IDs, event codes and timestamps, not private questions,
sources, prompts or proposal contents. Budgets show UTC accounting days and exact
millionths of USD; uncertain provider charges remain reserved. Read-only,
unconfigured, forbidden, offline and identity-mismatch states remain distinct.
