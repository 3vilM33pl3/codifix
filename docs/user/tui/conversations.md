# Conversations Tab

**Conversations** provides two read-only views: **Agent exchanges** for the
selected project and **My Inbox** for your human account in the selected area.
Press `v` to switch. The header identifies the area and project.

## Agent exchanges

This view uses your existing project service credential. The service must
explicitly grant that credential exchange observation access. A project read
credential alone does not grant access to private exchanges.

The default view combines both directions across the selected project's peer
exchanges into one transcript. It checks for new messages every two seconds
while the view is open. Earlier pages load in the background. Use `/` to
filter by an exact peer, or clear the filter to see all enrolled peers.

Local messages start with the area name and `>`. Peer messages are aligned
right and end with `<` and the peer name. Automatic receipts appear as notices.
Older exchanges show the messages retained in their snapshot. The service may
truncate an individual exchange's recorded detail.

The view follows the latest message. `PgUp`, `k` or `Home` pauses it while
you read earlier messages. `End` resumes following. `r` refreshes the view.
Press `m` for the task list and Enter on a task for its individual transcript.
There, `m` toggles task IDs, timestamps and status history, and `Esc` returns
to the task list. Press `m` from the list to return to live messages.

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
