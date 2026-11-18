# Conversations Tab

**Conversations** provides two read-only views: **Agent exchanges** for the
selected project and **My Inbox** for your human account in the selected area.
Press `v` to switch. The header identifies the area and project.

## Agent exchanges

This view uses your existing project service credential. The service must
explicitly grant that credential exchange observation access. A project read
credential alone does not grant access to private exchanges.

Use `/` to enter an exact peer filter, or clear it to see all peers. Select an
exchange with `j/k` or the arrow keys and press `Enter` to open its transcript.
Local messages start with the area name and `>`. Peer messages are aligned
right and end with `<` and the peer name. The transcript shows requests,
clarifications, answers and results in their recorded order. Automatic receipts
appear as notices. Older exchanges show the messages retained in their snapshot.

Press `m` to switch between the transcript and technical details, including task
IDs, timestamps, status changes and verification. `Esc` returns to the list.
`PgDn` reads an older list page and `Home` returns to the first page. In an open
exchange, `j/k`, `d/u` and `PgUp/PgDn` scroll. Press `r` to refresh.

Disabled, unavailable, denied and stale reads are shown explicitly. A recorded
result does not establish independent verification or current peer reachability.

## My Inbox

Press `i` to sign in through your browser using the selected area's configured
HTTPS browser origin. Complete sign-in in that browser; `Esc` cancels a pending
request. The native session stays in memory. Login URLs, authorization codes and
tokens are never displayed in the terminal or copied into your project token.

After sign-in, press `r` to load conversations. Select one with `j/k` and press
`Enter` to inspect its participants, disclosure summaries, messages and task
cards. `n` reads older messages; `Esc` returns to the list. Scroll with `j/k`,
`d/u` or `PgUp/PgDn`.

Inbox is personal and area-wide. Changing the selected project does not turn it
into a project conversation list. Reading messages does not mark them read.
The TUI provides no compose, reply, membership, permission, approval, retry or
task-action controls.

Press `o` to clear the local session and request server revocation. If the
service cannot confirm revocation, press `o` again to retry; failed revocations
remain in memory for cleanup. Changing area, signing out, session expiry and
unauthorized replies clear private content and fence older responses. Exiting
the TUI also clears the local session and attempts bounded server revocation.

For custom service targets without a selected area and trusted browser-origin
metadata, My Inbox stays unavailable. Agent exchanges continue to use the
project credential independently.

Remote content is shown as untrusted plain text. Links and terminal escape
sequences in messages are not executed.
