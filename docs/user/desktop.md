# Codifix Desktop

Codifix Desktop connects to an existing Codifix service. Open **Connection
settings**, select your HTTPS service, and choose **Sign in**. Complete sign-in
in your system browser and return to the application. The saved session uses
your OS keyring; sign-out revokes it, while disconnect preserves it for reuse.
Permissions come from the service account and existing grants.

Choose an area before a project. Project selection is navigation and does not
grant access. Services, databases and watchers are configured separately.

Network and Inbox are opt-in features. See [Network and Inbox](network-inbox.md)
and the [candidate runbook](network-inbox-candidate.md) for their authority,
disclosure and activation boundaries. The ARM64 Linux backend release does not
include a desktop installer.

For desktop build dependencies and platform support, see
[desktop setup](../../desktop/README.md). Native platform acceptance and live
account validation must be recorded separately from synthetic tests.
