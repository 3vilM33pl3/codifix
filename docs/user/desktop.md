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
response is reported as uncertain. Browser sign-in returns you automatically to the app with your existing account permissions. Its session
is saved in your OS keyring and checked again when the app reopens; passwords
stay in your browser. Pasted service tokens remain in memory only.

## Sign in (0.1.3)

Click **Sign in**, log in in your default browser, and return to Codifix automatically.
There are no device codes, fingerprint comparisons or project approval screens.
Your account keeps its existing permissions. If needed, choose a project after
signing in; this is navigation, not an access grant.

The installed Workshop build has its HTTPS service preset. Other services and
local/token access are under **Connection settings**. Sign out revokes the saved
app session; Disconnect keeps it for next time. Old managed-device credentials
remain separate, with an explicit removal option in Connection settings.

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

## Previous device enrollment (0.1.1–0.1.2, superseded)

In **Manage connection**, enter your public Codifix HTTPS service origin and
choose **Sign in through browser**. Compare the code and fingerprint, select
projects in the browser and approve Codifix Desktop. The app completes the
connection automatically. A configured device-onboarding service and an unlocked
desktop keyring are required. Use the advanced local/token option for a local
single-user service; browser access does not authenticate the separate dev service.

In the browser, choose **Sign in to review this device** before **Review device
request**. The review button is unavailable until the browser identity is checked.
An authenticated account without project grants can review the device but will see
no available projects; an administrator must review its existing membership and
project onboarding policy. A denied identity check is a service error, not proof
that you are signed out. Expired device requests must be restarted in the desktop.

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

### Browser identity check repair (25 October 2026)

The Workshop service hotfix at `79dcf24f` removes an incorrect memory-read
permission requirement from identity inspection and logout. Previously, a valid
OIDC account with no project grants received HTTP 403 from `/v1/auth/me`; the
approval page mislabeled that response as a missing login and disabled review.
Identity failures now distinguish signed-out, denied and connection states.

Verification: 92 backend authentication tests passed against a disposable
PostgreSQL database, including zero-grant identity/empty review, unauthorized
read/approval rejection, logout CSRF and invalid/expired/revoked/disabled sessions.
All 296 web tests and the web/release builds passed. The optimized service and
web assets were installed only on Workshop, with previous assets retained;
health reported `ok` and database `up`. Configuration, grants and schema were
unchanged. In the real Chrome approval page, the current account became visible
and clicking **Review device request** returned the expected expired-request
message. Signing into the intended account and approving a fresh device request
remain human actions; no project access was granted by this repair.

The 0.1.1 DEB was installed per-user on the current host and launched with no
startup log errors. A second launch exited successfully while the original
process remained running and owned the single-instance D-Bus name. Package and
installed executable checksums match the preserved release artifacts; the 0.1.0
installation is retained for rollback. This checks installation and process
lifecycle, not completion of the user's browser approval.

## Private certificate authorities (0.1.2)

Desktop HTTPS now uses the operating system trust store. The earlier bundled
public-root list rejected Workshop's already trusted Robot Motel CA with
`InvalidCertificate(UnknownIssuer)`, before any device request reached the server.
Certificate and hostname checks remain enabled. Connection failures now distinguish
TLS, network and timeout errors and no longer imply that onboarding is disabled.

Verification: the old transport reproduced the certificate failure; the updated
transport reached Workshop's unauthenticated endpoint and received HTTP 401.
A local TLS regression using the production client passed trusted-CA success,
untrusted-CA rejection and hostname-mismatch rejection, without changing system
trust. Seven native unit tests and Clippy passed; the frontend and DEB built.

After installing 0.1.2, a native WebKit WebDriver click on **Sign in through
browser** reached Workshop's real enrollment endpoint and opened the default
Chrome browser at the matching device approval page. The app displayed the same
user code and waited for browser approval without a connection error. The test
driver and its listening ports were stopped while the desktop remained open.
Human sign-in and selection of project grants remain the user's steps; they
were not completed automatically or claimed as tested.

## Simplified human login verification (0.1.3, 25 October 2026)

The normal desktop flow now uses the human OIDC account with PKCE and an
origin-bound native session. Device onboarding remains available separately for
managed devices; normal desktop login no longer depends on its project manifests.

Validation: 94 service authentication tests passed against a disposable database;
14 renderer workflows and 10 native tests passed. Independent review checked
native/browser credential separation, transactional exchange and audit, current
permissions, callback validation, cancellation and failed-sign-out recovery.
Clippy, the host native-keyring write/read/delete test, all three TLS regression
cases and the frontend/DEB build passed. The installed native WebKit window shows
one Sign in button with Workshop preset. The live browser reaches Authentik's
username login directly, including when a stale Codifix browser cookie exists.
Completion with the user's credentials is still pending; automated callback tests
are not evidence that the real account has signed in.

Workshop was upgraded to schema 86 with the existing application role granted
CRUD only on the new auth_desktop_codes table. The service remains verify-only;
configuration and human/project grants were unchanged. The old executable and a
PostgreSQL custom-format backup are retained under
`~/.local/state/codifix-simple-login-20261025/`. Health reports database up/status
ok. Because the old executable verifies schema 85, rollback after any native
logins requires a reviewed schema/data recovery, not just swapping binaries.

The per-user 0.1.3 package, install receipt and native sign-in screenshot are in
`~/Documents/ChatGPT/Codifix/linux-desktop-build/0.1.3/`; 0.1.2 remains available
for desktop rollback. The build presets only public service URL/name through
Vite environment variables; saved user connection preferences take precedence.
