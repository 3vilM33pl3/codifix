# Linux desktop

Codifix Desktop provides the approved streamlined interface: Home, Memories,
Sessions and Activity, with project context above and Settings below. Start with
[desktop setup and framework decisions](../../desktop/README.md).

The application connects to an existing service. It does not install a database,
start watchers, change sharing permissions or configure agents. Select the right
service/knowledge area before choosing a project. A project folder must already
have a Codifix marker or an existing service slug.

Review and job retry controls use the service's current permissions. Approvals
and rejections refresh the queue only after a matching receipt. An interrupted
response is reported as uncertain. Browser sign-in connects the desktop through a device approval. Its credential
is saved in your OS keyring and checked again when the app reopens; passwords
stay in your browser. Pasted service tokens remain in memory only.

## Current acceptance limits

The first package is built on the current Linux host. Linux builds depend on
WebKitGTK 4.1 and the host's system library baseline; a binary built on a newer
distribution is not an assurance of compatibility with older distributions.
The included CI workflow builds on Ubuntu 24.04, but is not claimed to have run
until CI executes it.

Automated checks and browser visual review do not establish GNOME/KDE, Wayland,
Orca, native file chooser and notification acceptance on every desktop. Windows
and macOS share the source architecture but have not been built or validated in
this increment. Native external-agent chat handoff is unavailable; use the
session reference and project resume briefing. Advanced service administration
remains available through explicit browser links.

Code Connect templates are saved locally. Figma requires an Organization or
Enterprise plan to save the associations for this library.

## Implementation verification (25 October 2026)

Built on Ubuntu 26.04.1 amd64 with WebKitGTK 2.52.6 and GTK 3.24.52.
Eight renderer workflow tests and three native transport tests pass. The
production frontend and DEB build succeed. A native process under an isolated
Xvfb display stayed alive for the ten-second smoke interval and exited only
when the harness timeout stopped it. Xvfb reported missing DRI3 acceleration;
this check establishes startup, not accelerated graphics or interactive native
acceptance. Light Home, Memories and dark Settings were inspected with the
explicit browser review fixture.

The configured local endpoint returned HTTP 401 without credentials. Live
protected-project reads and writes have therefore not been claimed as verified;
use your area’s public HTTPS address and **Sign in through browser** in the
connection dialog to exercise them.
No production memory was approved, rejected or retried during testing.

## Browser sign-in (0.1.1)

In **Manage connection**, enter your public Codifix HTTPS service origin and
choose **Sign in through browser**. Compare the code and fingerprint, select
projects in the browser and approve Codifix Desktop. The app completes the
connection automatically. A configured device-onboarding service and an unlocked
desktop keyring are required. Use the advanced local/token option for a local
single-user service; browser access does not authenticate the separate dev service.

**Sign out browser access** revokes this desktop credential and removes it from
the keyring. **Disconnect** keeps it for the next connection. If cancellation
cannot be confirmed, retry it. The explicitly confirmed local-forget recovery
option removes only the saved credential and does not revoke remote access.

The 0.1.1 change passes twelve renderer tests and seven native tests, including
late approval cancellation and exact device/principal binding. A disposable
credential was written, read and removed from this host's Linux Secret Service.
Live browser authentication and protected project access still require the user's
identity-provider sign-in and project approval; automated checks do not claim
that approval occurred.
