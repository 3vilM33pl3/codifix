# Linux Desktop

Use Codifix Desktop to pick up work on a Linux machine. **Home** shows a project briefing, **Memories** lets you inspect evidence, and **Sessions** shows agent activity. The app connects to a Codifix service you already have access to.

## Connect to your area

1. Install your team's Linux Desktop build or follow the [desktop build guide](https://github.com/3vilM33pl3/codifix/blob/v3.1.0/desktop/README.md). Desktop has its own release cycle. The desktop source at the Codifix 3.1.0 tag is **0.2.0-rc.1**. The core Debian packages install the service tools; obtain the graphical app separately.
2. Choose **Area** beside the project selector. Select the area your administrator assigned. For a manually configured service, use **Connection settings** and its approved HTTPS address.
3. Choose **Sign in** and complete sign-in in your browser. When you return to Desktop, check the selected area and connection. The native session uses your existing account permissions and is saved in the OS keyring. **Disconnect** keeps that session for your next connection. **Sign out** revokes it.
4. Select an accessible project, or use **Open project folder** for a repository configured with an area route. A project slug is scoped to the selected area. If two areas have the same slug, confirm the area before opening the project.

For routed areas the client checks the service's area UUID before sending credentials. If the identity is unexpected or the area is unavailable, check the route with your administrator. See [Areas and projects](https://codifix.dev/docs/areas).

## Pick up a project

Open **Home** for the resume briefing, then open **Memories** to inspect the source and replacement history for a decision. **Sessions** shows agent activity for the selected folder or project. Use the session reference and project briefing when returning to the agent.

Use **Activity** to inspect recent events, failed jobs and job details. Confirm an authorized retry once, then check for its receipt. An interrupted response can leave an operation uncertain. Refresh the view before deciding what to do next.

Open **Network** or **Inbox** to contact an enrolled specialist for your signed-in area. The [Network and Inbox guide](https://codifix.dev/docs/network-inbox) explains how to inspect a message and the work it triggered.

## Platform scope

The desktop build depends on WebKitGTK 4.1 and a compatible Linux system library baseline. The package workflow targets Linux amd64. Platform claims for the core Debian, macOS and Windows packages are listed in [release compatibility](https://codifix.dev/docs/help/known-limitations).

### Connection and routing checks

For each configured area, the bound client TOML must contain a matching `[area]` identity and `[auth].public_base_url` for its exact HTTPS origin. The native bridge checks the `area_instance_id` from `/healthz` before restoring a saved credential. HTTPS uses the OS trust store. Organizations using a private certificate authority must provision its trust through their normal device-management process.

If a folder is selected before sign-in, Desktop keeps the selection pending. Switching areas clears the previous area's cached data. Recent projects stay bound to their origin and area UUID, including after reopening the app. If the wrong recent is selected, choose the intended area and reopen the mapped folder.

### Acceptance evidence

The 0.1.3 human sign-in workflow was exercised on one Linux installation. Its tests cover browser callback validation, persistence in the OS keyring, restoration across restart and failed sign-out recovery. The later routing tests cover equal project names across areas, unavailable services and mismatched UUIDs. Native behavior on other desktops, accessibility tools and identity providers needs its own acceptance run. See the [desktop validation guide](https://github.com/3vilM33pl3/codifix/blob/v3.1.0/desktop/README.md#validation-and-visual-review).
