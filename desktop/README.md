# Codifix Desktop

Linux-first implementation of the [approved Figma design](https://www.figma.com/design/XLDPSlu5iPKQGqu8SBIEPT?node-id=4-2), using one Tauri/React codebase for future OS-specific desktop packages.

## Run and build

Use Node 22+ and Rust stable. On Debian/Ubuntu install Tauri's build prerequisites:

```sh
sudo apt install build-essential pkg-config libwebkit2gtk-4.1-dev libgtk-3-dev libdbus-1-dev libssl-dev librsvg2-dev patchelf
npm ci
npm run desktop:dev
```

From this directory, `npm run desktop:build` produces the executable and a `.deb` under `src-tauri/target/release/` (or `$CARGO_TARGET_DIR/release/`). Install the local package with `sudo apt install ./path/to/package.deb`, or run `codifix-desktop` directly from the build output. No backend is bundled or started. The app appears as **Codifix Desktop** in the application menu after installation. Closing the window exits the app.

The desktop crate has its own Cargo workspace/lockfile. It does not add GTK requirements to the service workspace. The renderer imports **types only** from `../web/src`; keep the repository checkout intact for development.

## Connect

1. Enter the public **HTTPS origin** of your Codifix service and a connection name. Click **Sign in through browser**.
2. Sign in to Codifix in your default browser. Compare the user code and credential fingerprint with the desktop, select the projects and reader/writer roles you want, and approve **Codifix Desktop**. The app connects automatically after validating the approval and the resulting identity.
3. Choose an existing project slug, or open its local folder. The folder chooser reads `.codifix/project.toml`; it does not initialise projects or infer names from folder names.

Browser sign-in uses the service's existing device enrollment API. It requires a writable primary multi-user service with a configured HTTPS browser origin, explicit area identity and reviewed project manifests. Use its public origin, not an internal proxy address or the development endpoint. Device access is bounded by the human's current permissions and approved reader/writer roles; administrative operations still belong in the browser.

Credentials and polling proofs are generated in Rust and saved **before** sending the request to the OS credential store: Linux Secret Service (e.g. GNOME Keyring), macOS Keychain or Windows Credential Manager. Only Linux has been exercised. Unlock the desktop keyring if prompted; there is no plaintext fallback. No password or browser cookie transfers to the renderer. Reopening the app restores the saved credential for the exact origin and verifies it with the service. Pending requests can be resumed without generating another credential.

**Cancel sign-in** cancels the saved request, or revokes the device if approval already happened. **Sign out browser access** revokes access before deleting the keyring record. An uncertain response retains the record for retry. If an unreachable service never created the request, the recovery section offers explicitly confirmed **Forget saved sign-in locally**; this does not claim remote revocation. **Disconnect** clears the current session but keeps browser access saved for next time. Closing the window also retains saved browser access. A second launch focuses the existing app, preventing competing enrollment processes.

For local writable single-user services, expand **Local service or service token** and use **Connect locally or with token**; this establishes the existing loopback owner session. Explicitly pasted tokens stay only in native process memory and must be re-entered after restart. The local development endpoint defaults to `http://127.0.0.1:4250`; it is not a substitute for your area's public browser sign-in origin.

Preferences contain appearance/notification settings and the last successfully connected endpoint/name. Recent projects contain endpoint, slug and an optional folder path. Credentials never enter localStorage. Project and connection switches clear cached query data. No service auth or CORS configuration changes are required.

## Workflows

- **Home:** recent knowledge, pending review count and a route to the project resume briefing.
- **Memories:** complete paged browse (up to 100,000 records), local text/tag/type filtering, readable content, provenance, replacement history and optional related-knowledge view. Relationships load the selected record directly. A changing/incomplete page sequence asks for refresh instead of silently presenting partial results.
- **Sessions:** live agent monitor filtered by exact selected folder, or by project name when no folder is selected. Capture time is explicitly project-wide because the monitoring API has no per-session capture state. Resume loads an existing service briefing; it does not launch or resume another agent process.
- **Review:** current/proposed memory comparison and permission-gated approve/reject. Success requires a matching service receipt. Sources/history remain service-owned.
- **Activity:** latest 100 events, errors within that window, paged jobs, failure details and explicit retry confirmation. A retry preserves its request ID after an uncertain outcome and retires it after a matching receipt.
- **Settings:** system/light/dark themes, opt-in native notifications, current project/connection, read-only diagnostics. Integrations, sharing and advanced service administration open the existing web interface in the default browser. They are not duplicate native administration forms.
- **Keyboard:** Ctrl/Command K, Tab/Shift Tab, arrow keys/Enter in the command palette, Escape in dialogs, skip-to-content link. Native window decorations and persisted window size/position belong to the OS.

Connection errors are distinct from empty results. Previously loaded data is labelled stale; write controls are disabled while health checks fail. Requests are bounded, redirect-free and restricted to an explicit route/method allowlist. No generic renderer shell, filesystem or HTTP access is enabled.

## Framework choices

| Choice | Reason and tradeoff |
| --- | --- |
| Tauri 2 / Rust | Shares desktop code while using the OS webview and a small native bridge. It avoids bundling Chromium, but Linux WebKitGTK and desktop integration still require testing on each supported distribution. |
| React 19 / TypeScript / Vite | Matches the existing web ecosystem, reuses API types and builds static bundled assets. The separate desktop UI can follow the approved design without disrupting the current web shell. |
| CSS variables / Inter | Implements the Figma design tokens directly, including OS-following themes. Inter is bundled; no remote font requests. |
| Radix Dialog | Focus trapping, Escape dismissal and accessible dialog semantics. Automated tests supplement rather than replace Orca testing. |
| TanStack Query | Scoped loading, caching, refresh and stale/error states. No persistent knowledge cache or second memory database. |
| Lucide | Small SVG interface icons with labels; decorative icons do not replace text. |
| [keyring](https://docs.rs/keyring/3.6.3/keyring/) | Uses the OS credential store rather than inventing password storage. Linux needs an available, unlocked Secret Service; failures are explicit. |
| [Tauri single-instance](https://v2.tauri.app/plugin/single-instance/) | Prevents concurrent app processes from racing over saved enrollment. A second launch focuses the existing window. |
| Tauri dialog / notification / window-state | Native folder selection, optional desktop feedback and window restoration. Renderer capabilities stay narrow. |

Electron would standardise the rendering engine but ship a bundled Chromium runtime. GTK/libadwaita would give Linux-native widgets but require substantially separate UI implementations on other OSes. The selected approach shares the interface and service bridge, with OS-specific packaging and integration tests.

## Validation and visual review

```sh
npm test
npm run build
cargo test --manifest-path src-tauri/Cargo.toml --locked
cargo fmt --manifest-path src-tauri/Cargo.toml --check
npm run desktop:build
```

`npm run dev` also serves `/review.html`, an explicitly illustrative fixture for comparing the design without accessing real project data. Its IPC mock and sample records are **not included** in the packaged `index.html` entry. Fixture actions that require native integrations report that they are unavailable.

Validation: twelve renderer workflow tests, seven native boundary/protocol tests, a separately run disposable host-keyring roundtrip, TypeScript/production frontend build and Linux DEB build. Browser tests cover approval handoff, late approval after cancellation, saved access reuse and explicitly confirmed local recovery. Native tests check origin/request binding, typed approval grants, identity mismatch, credential isolation and cancel/approval reconciliation. See `docs/user/desktop.md` for acceptance limitations.

## Figma Code Connect

`code-connect/*.figma.ts` contains parserless mappings for Button, Navigation and Badge in `src/ui.tsx`. Figma returned an Organization/Enterprise-plan requirement when saving mappings, so no published Code Connect association is claimed. The local templates retain the exact component/property mappings for later publication.
