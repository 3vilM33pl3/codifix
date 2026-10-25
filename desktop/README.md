# Codifix Desktop

Linux-first implementation of the [approved Figma design](https://www.figma.com/design/XLDPSlu5iPKQGqu8SBIEPT?node-id=4-2), using one Tauri/React codebase for future OS-specific desktop packages.

## Run and build

Use Node 22+ and Rust stable. On Debian/Ubuntu install Tauri's build prerequisites:

```sh
sudo apt install build-essential pkg-config libwebkit2gtk-4.1-dev libgtk-3-dev libssl-dev librsvg2-dev patchelf
npm ci
npm run desktop:dev
```

From this directory, `npm run desktop:build` produces the executable and a `.deb` under `src-tauri/target/release/` (or `$CARGO_TARGET_DIR/release/`). Install the local package with `sudo apt install ./path/to/package.deb`, or run `codifix-desktop` directly from the build output. No backend is bundled or started. The app appears as **Codifix Desktop** in the application menu after installation. Closing the window exits the app.

The desktop crate has its own Cargo workspace/lockfile. It does not add GTK requirements to the service workspace. The renderer imports **types only** from `../web/src`; keep the repository checkout intact for development.

## Connect

1. Connect to your existing service origin, e.g. `http://127.0.0.1:4250`. Change this to the endpoint for the intended knowledge area and give the connection a meaningful name.
2. Local writable single-user services establish a local-owner cookie session through the existing loopback bootstrap API. Protected services accept an explicitly supplied service token. HTTPS is required for non-loopback endpoints.
3. Choose an existing project slug, or open its local folder. The folder chooser reads `.codifix/project.toml` to discover the slug; it does not initialise projects or infer names from folder names.

Credentials and session cookies live only in native process memory. Tokens are never written to preferences. After restart, protected endpoints need the token again. Preferences contain only appearance/notification settings and the last endpoint/name; recent projects contain endpoint, slug and an optional folder path. Project and connection switches clear cached query data.

Browser login opens the service's existing sign-in page. Its session does **not** transfer into the desktop process: desktop multi-user authentication currently uses a service token. The app does not change the service's authentication or CORS configuration.

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

Initial validation: eight renderer workflow tests (connection failure, restoration, keyboard palette, complete paging, linked detail, review receipts/permission gates, retry identity), three Rust boundary tests, TypeScript/production frontend build and Linux DEB build. See `docs/user/desktop.md` for acceptance limitations.

## Figma Code Connect

`code-connect/*.figma.ts` contains parserless mappings for Button, Navigation and Badge in `src/ui.tsx`. Figma returned an Organization/Enterprise-plan requirement when saving mappings, so no published Code Connect association is claimed. The local templates retain the exact component/property mappings for later publication.
