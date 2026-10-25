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
response is reported as uncertain. Native credentials are discarded on exit;
browser login does not authenticate the bundled renderer.

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
