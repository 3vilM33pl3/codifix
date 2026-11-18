# Conversations

## Purpose

`v` switches between **Agent exchanges** and **My Inbox**. Both are read only.
The header identifies the selected area and project.

## Layout

A scope header stays above a scrollable list or detail view. Each view shows
loading, unavailable, disabled and stale state. Details replace the list.

## Controls

### Agent exchanges

The live transcript combines both directions across the project's exchanges.
New messages appear automatically. Use `/` to choose a peer or clear the
filter. `PgUp` or `k` scrolls back and pauses following. `End` follows the
latest messages again. Press `m` for the task list and Enter to inspect one
exchange. There, `m` toggles technical details and `Esc` returns to the
task list. From the list, `m` returns to the live transcript.
The project credential needs the companion observation grant.

### My Inbox

Inbox is personal and area-wide. `i` starts browser sign-in; complete it in your
browser. `Esc` cancels a pending sign-in. `o` clears the local session and asks
the service to revoke it; use `o` again if server revocation fails. Sessions
stay in memory and never replace your configured project credential.

Once signed in, `r` loads conversations; `j/k` selects and `Enter` opens one.
Details show participants, disclosure summaries, messages and task cards.
`n` reads older messages, `Esc` returns, and `j/k`, `d/u`, `PgUp/PgDn` scroll.
Reading does not mark messages read. Area changes, sign-out, session expiry and
unauthorized replies clear private content. No compose, task action, membership,
permission, retry or approval controls are available here.

Remote text is displayed as untrusted plain text. Credentials and login URLs
are never displayed in the terminal.

## Workflows

Use Watchers & Companions to select a peer, then Enter to inspect its project
exchanges. Use My Inbox for personal conversations after a separate browser
sign-in. Inspect observations here; perform human task actions in the Web UI.
