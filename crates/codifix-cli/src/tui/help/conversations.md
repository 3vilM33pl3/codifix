# Conversations

## Purpose

`v` switches between **Agent exchanges** and **My Inbox**. Both are read only.
The header identifies the selected area, instance and project.

## Layout

A scope header stays above a scrollable list or detail view. Each view shows
loading, unavailable, disabled and stale state. Details replace the list.

## Controls

### Agent exchanges

Exchange observations use the selected project's existing service credential.
They require the dedicated observation grant. `/` edits an exact peer filter;
empty input means all peers. `j/k` or arrows select an exchange, `Enter` opens
its journal, and `Esc` returns to the list. `PgDn` loads an older list page;
`Home` loads the first page. Inside detail, `j/k`, `d/u` and `PgUp/PgDn` scroll.
`r` refreshes. Disabled, denied, unavailable and stale observations are explicit.
A reported result or legacy snapshot is not independent proof of success.

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
