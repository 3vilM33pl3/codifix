# Codifix for macOS

The Xcode project is `Codifix.xcodeproj` and builds a native macOS 15+
SwiftUI app. Open the shared `Codifix` scheme in Xcode or build it with:

```sh
xcodebuild -project apps/macos/Codifix.xcodeproj -scheme Codifix \
  -configuration Debug -destination 'platform=macOS' \
  -derivedDataPath /tmp/codifix-macos CODE_SIGNING_ALLOWED=NO build
```

See [VALIDATION.md](VALIDATION.md) for implementation acceptance results and
the latest native UI-test run.

The app does not start a service, create a project, or populate a demo database.
Connection profiles are saved as metadata in UserDefaults; remote tokens are
stored in Keychain under the profile UUID. Local profiles must point to a
loopback service. The UI suggests port `4250` in Debug and `4040` in Release,
but the user must review and connect a profile explicitly.

## Shared feature contract

Feature sources live under `Codifix/Sources/Features/<area>/`; unit tests
live under `CodifixTests/<area>/`. Each screen receives an immutable
`FeatureContext`, which carries the active `MemoryServiceClient`, profile,
`/v1/auth/me` snapshot, effective project permissions, and connection revision.
Use the revision to cancel/reload screen work:

```swift
struct MemoriesView: View {
    let context: FeatureContext
    @State private var records: [MemorySummary] = []

    var body: some View {
        MemoryList(records: records)
            .task(id: context.revision) {
                do {
                    let route = APIRoute<ProjectMemoriesResponse>(
                        path: "/v1/projects/\(URLPathSegment.escape(context.profile.projectSlug))/memories"
                    )
                    let response = try await context.client.get(route, context: context.requestContext)
                    try Task.checkCancellation()
                    records = response.items
                } catch is CancellationError {
                    return
                } catch {
                    // Present the error in this feature's loading state.
                }
            }
    }
}
```

Writes should use one `APIRoute<Response>` through `client.send`. For example,
delete is `DELETE /v1/memory` with JSON `{ "memory_id": "…" }`. Encode its body
with `JSONEncoder.codifix`. Binary export endpoints use `client.sendData`
with method, path, query, headers, body, and the same `context.requestContext`.
`HTTPTransport` is injectable for deterministic tests; it never retries a
write. Dynamic path values must be escaped one segment at a time with
`URLPathSegment.escape`, and query values should be passed as `URLQueryItem`s.

`AuthMeResponse` matches the service response: authentication mode and
read-only state are top-level; role, global permission strings, project roles,
and project permission strings are inside `principal`. Gate screens and actions
with `context.permissions`, and also account for `context.permissions.readOnly`
for writes. Access administration requires a global `admin` role. Unknown role
values decode safely and grant no capabilities.

Local profiles use `/v1/auth/session/bootstrap` only after checking
`/v1/auth/me` says `single_user`. The bootstrap call is restricted to loopback;
the client stores its `codifix_session` and `codifix_csrf` cookies in a private,
per-profile memory jar. It sends the CSRF cookie as `x-csrf-token` on writes.
Remote profiles attach their profile-scoped Keychain token as a bearer header.
Redirects are rejected, credentials never cross the profile's configured
origin, and request or response errors do not include credentials.

Run focused unit tests with:

```sh
xcodebuild -project apps/macos/Codifix.xcodeproj -scheme Codifix \
  -destination 'platform=macOS' -derivedDataPath /tmp/codifix-macos \
  CODE_SIGNING_ALLOWED=NO -only-testing:CodifixTests test
```

The UI suite uses a separate runner and requires Developer Tools Security to be
enabled in macOS. Xcode may need standard administrator authorization the first
time it enables UI automation on a host:

```sh
xcodebuild -project apps/macos/Codifix.xcodeproj -scheme Codifix \
  -destination 'platform=macOS' -derivedDataPath /tmp/codifix-macos-ui \
  CODE_SIGNING_ALLOWED=YES CODE_SIGN_IDENTITY=- DEVELOPMENT_TEAM= \
  -only-testing:CodifixUITests test
```

This uses local ad-hoc signing for the test runner, not distribution signing.
The UI-test target alone disables App Sandbox so it can launch its Python
loopback fixture server; the production app target is unchanged. UI tests
isolate connection history in memory and launch their own loopback fixture
server for each fixture-backed case. The suite covers all seven native flows:
connection/project selection and memory inspection, memory filters/provenance/
history, cited Query answers and source inspection, replacement Review approval
and rejection, automation dry-run and approval decisions, bundle import/export
previews, and global-admin Access token/membership workflows. Tests assert
visible outcomes and fixture-recorded routes; bundle tests compare safe SHA-256
digests so preview and import use the same archive. UI-test fixtures are
in-memory and loopback-only. If Xcode reports that it timed out enabling
automation mode, enable Developer Tools Security through the standard macOS
authorization flow and rerun. See [VALIDATION.md](VALIDATION.md) for the exact
test mapping and latest results, or the [fixture service guide](CodifixUITests/Fixtures/README.md)
for manual checks.

Debug builds accept `--ui-window-size=1440x960` and
`--ui-window-size=1024x768` for repeatable window-layout inspection. For example:

```sh
open -n -a '/tmp/codifix-macos/Build/Products/Debug/Codifix.app' \
  --args --ui-window-size=1024x768
```

Optional live read smoke tests use the existing development service and never
perform feature mutations. Enable them with
`TEST_RUNNER_CODIFIX_LIVE_READ_SMOKE=1` when running the unit suite.
