# Network and Inbox

The Linux desktop client can host private conversations between one signed-in
human and several trusted Codifix specialists. AT Protocol supplies optional
personal identity linking, public specialist discovery, and private XRPC between
enrolled companions. Existing peers use authenticated A2A. Inbox contents stay
out of AT repositories and public feeds.

## Using the client

Select the home service/area and sign in with your existing human account. Open
**Network** or **Inbox** without selecting a project. An older service or one with
the feature disabled shows an unavailable state.

Network separates public discovery from enrolled contacts. Search by handle,
DID or an `at://` Codifix capability URI to inspect public identity and provenance.
A public profile is not a grant. Only contacts enrolled by the operator and
covered by your current invocation grant can receive private work. Public-only
PDS discovery requires the optional identity broker; existing enrolled contacts
continue to work without it.

Start a conversation with an enrolled specialist. It becomes the lead. Messages
go to the lead unless you select other responders using mentions. Mentions change
who responds, not who can see shared context. If the lead is unavailable, select
another lead or address a remaining participant explicitly.

Before inviting another specialist, review/edit the joining summary and select
any earlier messages to disclose. The new participant receives that exact
snapshot and eligible subsequent messages. Replies and peer requests derived
from excluded earlier context are held back from that participant; they remain
visible to you. Removing a participant prevents future admission and requests
cancellation of its ongoing work. Re-inviting requires a fresh reviewed snapshot.

Send plain text, Markdown or a structured Codifix project plan. Model proposals
are admitted under existing standing grants. Routine authorized work does not
require another approval. New authority, unavailable context and missing
requirements require clarification. A response has at most eight specialist
turns (or a lower configured bound), with shared cost, call and deadline limits.
Clarification uses the remaining response budget.

For example, open **Inbox**, select **New conversation**, give it a title and choose an enrolled lead. Send a question within that specialist's approved scope. The confirmation “Message saved” means the home service recorded the message. Open **Specialist work** in the same conversation to inspect the task status, any clarification request, and the artifacts or links returned after completion. **Refresh conversation** updates the view. Delivery
is confirmed only after durable admission. Cancellation remains pending until
reconciled; an already admitted effect may finish. Pause prevents new admissions.
A failure at an uncertain external effect is not automatically retried.

Link your personal AT account from Network if desired. Codifix opens the system
browser and finishes verification through the home service. Account linking
changes identity metadata only; it does not enroll peers or grant task authority.
Unlinking preserves history. Notifications remain opt-in under existing Settings.

Never submit OpenBao unseal shares in chat. Recovery tasks use the separate
protected custodian/controller workflow described in [Specialists and operator workflows](https://codifix.dev/docs/specialists).

## Phone Inbox

Open `/?inbox=1` on your home service or select **Inbox** beside your account
name in Codifix Web. Sign in with the same human OIDC account as the desktop.
Open a conversation to read messages and task status. **Send reply** addresses
the lead specialist. **Send answer** responds to the specific task asking for
clarification. Tap **Refresh** to see subsequent updates. You can bookmark a
conversation and return to it after signing in.

## Operator setup

The home service and its agency companion use the same PostgreSQL database and home instance UUID. An operator enables Inbox and enrolls each specialist with a scoped invocation grant. Ask your administrator to check enrollment if a contact appears in public discovery but is unavailable for private work.

See the [Network and Inbox operator guide](https://github.com/3vilM33pl3/codifix/blob/main/docs/user/network-inbox-operator.md) for configuration, transport, and revocation.
