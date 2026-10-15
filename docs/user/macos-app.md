# Memory Layer for macOS

Memory Layer for macOS is a native client for an existing Memory Layer service.
It requires macOS 15 or later. The application does not install a service or
create a database.

## Build locally

With Xcode installed, run from the repository root:

```sh
xcodebuild -project apps/macos/MemoryLayer.xcodeproj -scheme MemoryLayer \
  -configuration Debug -derivedDataPath target/macos \
  CODE_SIGNING_ALLOWED=NO build
open 'target/macos/Build/Products/Debug/Memory Layer.app'
```

This creates a local development application. Distribution signing and
notarization are not configured.

## Connect to your existing project

Open **Connection settings**, enter your service address and project slug, and
optionally provide the repository path used by repository-aware operations.
The repository path is interpreted by the service; for a remote service, use a
path on that service's machine.

The source development service normally uses `http://127.0.0.1:4250`. A packaged
service normally uses `http://127.0.0.1:4040`. Choose the address that holds your
project's memories. Previously entered connections can be selected again;
automatic project discovery is not assumed.

- **Local owner session** connects to a loopback, single-user service using its
  existing session bootstrap endpoint.
- **Remote service token** uses a scoped token stored in macOS Keychain. Project
  permissions are supplied by the service.

The selected service, project, connection status, and development indicator are
visible in the application. If a project is empty, review the connection and
project slug. An empty result does not mean a database was created.

Interactive Authentik sign-in is not part of this version. Service tokens cannot
receive the administrator role, so remote token connections do not expose the
global **Access** screen.

## Explore and act on project knowledge

**Memories** supports search, filters, canonical content, provenance, related
records, version history, and confirmed deletion when permitted. **Query** shows
the answer alongside its citations and evidence sufficiency. Its query history
is session context rather than a persistent conversation.

**Graph** displays a read-only 3D neighborhood. Use layers, filters, camera
controls, and node selection to inspect its evidence. **Review** shows existing
and proposed memory content and supports approving or rejecting replacement
proposals.

**Agents**, **Activity**, and **Resume** expose session metrics, event details,
and deterministic or LLM-generated briefings. Agent and watcher information is
observational; the app does not start or stop those processes.

**Overview**, **Errors**, **Watchers**, **Skills**, and **Embeddings** expose
project health and the maintenance actions supported by the service. Errors are
derived from existing service diagnostics. Archiving means archiving low-value
memories. Displayed command suggestions are reference text.

**Automations** keeps its approval queue separate from memory replacement
Review. **Preview run** requests a dry run. **Bundles** requires a preview before
importing or exporting. **Access** is visible only to global administrators;
new service tokens are displayed once, then discarded from the display when
dismissed.

Actions follow service permissions and read-only mode. Failure messages remain
visible, and the app does not automatically retry writes. Destructive actions
require confirmation.

## Development validation

Run the macOS tests with:

```sh
xcodebuild -project apps/macos/MemoryLayer.xcodeproj -scheme MemoryLayer \
  -destination 'platform=macOS' -derivedDataPath target/macos-tests \
  CODE_SIGNING_ALLOWED=NO test
```

Use injectable transport fixtures for destructive and administrative test
scenarios. Live-service smoke checks should only inspect existing project data.
Sample data belongs in tests and previews, not as a fallback for failed live
requests.
