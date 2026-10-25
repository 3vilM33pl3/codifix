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

1. Click **Sign in**, log in through your default browser, and return to the app automatically. There is no device code or project approval step.
2. Choose an existing project slug or its local folder if no recent project is available. Sign-in uses your human account's existing permissions; it does not grant access to new projects.

**Connection settings** contains the service's public HTTPS origin and advanced local/token connection. Distribution builds can preset a service using `VITE_CODIFIX_SERVICE_URL` and `VITE_CODIFIX_SERVICE_NAME` at build time. Saved user preferences take precedence. Without a preset, development defaults to `http://127.0.0.1:4250`; use the advanced local connection for a single-user service.

The service needs schema 86 and the native human login endpoints, with its existing OIDC provider configured. No additional identity-provider client or callback registration is required. Browser login uses a fresh OIDC login and PKCE-protected, one-time code exchange through an ephemeral IPv4 loopback listener. The listener is bound before opening the browser and closes on cancellation or after ten minutes. The app verifies the returned human identity before saving its session in the OS keyring and focusing its window.

Passwords, PKCE verifiers, callback codes, and native credentials never enter the renderer or localStorage. Only Linux has been exercised. The keyring must be available and unlocked; there is no plaintext fallback. Restart restores the origin-bound saved session and validates it with the service. Expired or revoked sessions return to sign-in.

**Cancel sign-in** stops a pending login. **Sign out** revokes this app's session before removing it; **Disconnect** retains it for next time. If revocation cannot be confirmed, retry sign-out. If both revocation and keyring persistence fail, keep the app open so its in-memory cleanup credential remains available. Explicitly confirmed local-forget recovery removes local state without claiming remote revocation. Older managed-device credentials stay separate and can be revoked under **Connection settings → Older device sign-ins**.

Pasted service tokens stay in native process memory only. Preferences store appearance, notifications and endpoint/name; recent projects store endpoint, slug and optional folder path. Connection changes clear cached query data. HTTPS uses the OS trust store, including already installed company CAs, with certificate and hostname validation enabled. A second app launch focuses the existing window.

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
| [Tauri single-instance](https://v2.tauri.app/plugin/single-instance/) | Prevents concurrent app processes from racing over saved sessions. A second launch focuses the existing window. |
| Tauri dialog / notification / window-state | Native folder selection, optional desktop feedback and window restoration. Renderer capabilities stay narrow. |

Electron would standardise the rendering engine but ship a bundled Chromium runtime. GTK/libadwaita would give Linux-native widgets but require substantially separate UI implementations on other OSes. The selected approach shares the interface and service bridge, with OS-specific packaging and integration tests.

## Validation and visual review

```sh
npm test
npm run build
cargo test --manifest-path src-tauri/Cargo.toml --locked
python3 tests/native_tls.py
cargo fmt --manifest-path src-tauri/Cargo.toml --check
npm run desktop:build
```

`npm run dev` also serves `/review.html`, an explicitly illustrative fixture for comparing the design without accessing real project data. Its IPC mock and sample records are **not included** in the packaged `index.html` entry. Fixture actions that require native integrations report that they are unavailable.

Validation for 0.1.3: fourteen renderer workflow tests and ten native tests cover browser handoff, automatic connection, cancellation, retry, saved-session reuse, PKCE, callback host/state/path validation, provider denial, listener cleanup and recovery state. Service tests exercise real PostgreSQL session issuance, replay, expiry, permissions, cookie separation and transactional audit failures. These checks do not substitute for a user's real identity-provider login. See `docs/user/desktop.md` for installed-build evidence and acceptance limits.

## Figma Code Connect

`code-connect/*.figma.ts` contains parserless mappings for Button, Navigation and Badge in `src/ui.tsx`. Figma returned an Organization/Enterprise-plan requirement when saving mappings, so no published Code Connect association is claimed. The local templates retain the exact component/property mappings for later publication.

The Linux TLS regression uses the production Rust client against a local HTTPS
fixture in separate processes. A temporary CA bundle must succeed, an unrelated
CA must fail, and a mismatched hostname must fail. It does not modify system trust
or contact production services.
