# Native app, web and credential identity slice

The native product, project, scheme, module and bundle now use Codifix and
`dev.codifix.macos`; browser/service/CLI cookies use `codifix_session` and
`codifix_csrf`. Skill filters agree with the renamed backend registry. Native
Keychain namespaces use `dev.codifix.*`, preserving account/profile binding.
Semantic memory resources and routes are unchanged. Historical validation
claims retain their original names and evidence paths.

Validation on 17 October 2026:

- Web: 49 test files, 293 tests passed; TypeScript/Vite production build passed.
  Existing bundle-size warnings remain.
- macOS: isolated source copied to
  `~/.local/state/codifix-rename-20261017/source` on laptop.metatao.net;
  `xcodebuild -project apps/macos/Codifix.xcodeproj -scheme Codifix
  -destination platform=macOS -only-testing:CodifixTests test` with separate
  derived data and signing disabled passed 55 tests, four explicit live-service
  skips, zero failures. This is unit/build evidence, not signed UI acceptance.
- `scripts/codifix-cutover/macos-state.swift` typechecks with the laptop SDK.
  Read-only metadata inspection found one saved app profile and zero items in
  the old app Keychain service. No preference or credential writes occurred.
  CLI credential services require an exact company origin and separate review.

The release-specific transfer copies original profile bytes/UUIDs and exact
service/account credentials; it refuses destination conflicts and retains all
originals. Inspection does not read credential values. Apply requires stopped
clients and a recovery snapshot; signed-client Keychain access must be verified
at cutover. No runtime compatibility fallback was introduced.

Rust cookie/credential callers still need the consolidated post-integration
Rust suite; the federation worker owns matching federation test fixtures.
