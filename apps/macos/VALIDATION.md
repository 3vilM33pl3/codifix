# Native macOS acceptance — 2026-10-16

Built with Xcode 27.0 (27A266a) on macOS 26.6. Deployment target is macOS 15.
Debug and Release builds produce universal arm64/x86_64 applications. Native
runtime checks were performed on the local Apple silicon Mac; an Intel Mac and
macOS 15 runtime were not available for this run.

## Source and integration

All 15 destinations are wired through an exhaustive navigation switch. Feature
workers used GPT-6-luna with high reasoning in separate worktrees, with shared
interfaces established first and small commits reviewed and integrated by the
lead. No service API, schema, or backend source changed. Existing untracked files
in the primary checkout were preserved.

The app uses real HTTP service data. Synthetic content lives under test folders
and is served only by the explicitly started loopback acceptance server. It is
not a production fallback. Destructive and administrative acceptance actions
used that in-memory fixture service, never the existing project's data.

## Native flow checks

| Flow | Observed result |
| --- | --- |
| Existing service and explicit project | Connected through local session bootstrap; service and project remain visible. Saved real and fixture connections stayed separate. |
| Memories | Automatic first selection, filtering, canonical content, provenance, related records, and version history rendered. A selection cleanup race found during acceptance was fixed and retested. |
| Query | Returned an answer with citations and evidence confidence; citation/source inspection opened memory details. Insufficient evidence disabled unavailable source actions. |
| Replacement Review | Both approve and reject required confirmation, removed the fixture proposal, and displayed the server's outcome. |
| Automations | Preview run returned a dry-run inspector. The separate approval queue supported JSON edits, approval, and confirmed rejection. A sheet confirmation presentation bug was fixed and retested. |
| Bundles | Export preview showed counts and warnings before saving through the native save panel. Import preview showed changes and warnings before confirmation; the result reported imported/replaced/skipped counts. |
| Access | Created a dummy token, displayed and copied it once, dismissed it, and verified the list contained only its prefix. Token revocation and membership grant/revocation worked with confirmation. |

Agents, Activity, Resume, Overview, Errors, Watchers, Skills, and Embeddings were
inspected in the native app. Activity and Resume generated fixture briefings
with actions, next steps, warnings, referenced memories, and checkpoint content.
Skills displayed inventory and content. Existing service read checks found real
memories, activity, agent data, skills, and embedding backend metadata.

The live RealityKit graph rendered 29 nodes and 52 connections: 30 provenance
edges and 22 memory relationships. Node, source, and edge inspectors, layers,
filters, orbit drag, rotation, zoom, and keyboard search were exercised. No graph
editing or command execution controls are exposed. Pinch gestures were not
available through the single-pointer automation surface; zoom controls were
tested instead.

## Automated tests and service contracts

The combined **54-test unit/integration suite passed with zero failures and
zero skips**, with the opt-in development-service read smokes enabled. Coverage
includes typed decoding, escaped routes and request bodies, cookie/CSRF handling,
token isolation, permissions and read-only gates, rejected redirects, request
cancellation, late connection responses, preview invalidation, and write-failure
behavior without automatic retries.

The live reads decoded 10 memories (plus detail/history), 29 graph nodes, nine
automations, one principal, eight skills, one embedding backend, one agent
session, and 22 activity events. Only session bootstrap and reads ran against
the existing service; feature writes used injectable transports or the fixture
server.

Loading, disconnected, authentication-required, restricted, empty-project,
no-filter-match, insufficient-evidence, stale-agent, unavailable-backend, and
failed-operation views were exercised in the native app. A 15-second fixture
delay made the loading spinner observable before the records appeared. Empty
and failure checks confirmed
service/project identity, connection review, and retention of previously loaded
data where appropriate. Disabled embedding maintenance was verified when the
fixture backend was not ready.

## Layout and interaction

Memories, Query, and Graph were inspected at 1440 × 960 and 1024 × 768 logical
points (retina captures of 2880 × 1920 and 2048 × 1536). Compact panes, scrolling,
source inspection, and graph controls remained usable. Native system typography
and controls replace Figma's substitute fonts. This is a native adaptation of
the wireframes, not a pixel-identical browser rendering.

Command-F focused Memories search and Query input; Command-comma opened settings,
Command-R refreshed, and Command-Option-I toggled the inspector. Tab moved
between filter controls, Return preserved the current search result, and Escape
dismissed settings. Sidebar navigation, selection, and confirmation controls
were checked. Accessibility labels and native control
roles were inspected through the automation surface. A complete VoiceOver audit
was not performed.

## Automated UI tests

The signed `MemoryLayerUITests` suite passed **2 tests with 0 failures** on
macOS 26.6 using Xcode 27.0. The tests cover the unconfigured connection prompt
and a real UI connection to the isolated loopback fixture, including memory
detail, provenance, and version history. This is navigation smoke coverage; the
broader feature flows described above were verified manually in the native app.

The test runner uses an ad-hoc signature and a target-scoped entitlement that
allows the test process to launch the fixture server. The production app target
is unchanged. The UI-test profile store is in memory, so these tests do not
read or modify saved service profiles, the development service, or its data.
The fixture URL is entered via a temporary pasteboard item; the test restores
the previous pasteboard items and formats afterward.

The first run did not initialize automation because macOS Developer Tools
Security was disabled. After enabling it through the standard administrator
authorization prompt, Xcode ran the suite. That initial run then exposed two
test issues: the runner sandbox blocked the system Python shim, and simulated
typing dropped URL punctuation. The UI-test-only entitlement and pasteboard
entry resolved these issues. The final result bundle is
`target/macos-ui-signed/Logs/Test/Test-MemoryLayer-2026.10.16_06-38-44-+0100.xcresult`.
See [README.md](README.md) for the command and
[the fixture guide](MemoryLayerUITests/Fixtures/README.md) for reproducible
manual checks.
