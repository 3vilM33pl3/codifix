import { Button } from "./ui";
import { useState } from "react";
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";
import { useProjectData } from "./data";
import {
  errorText,
  openService,
  type AuthMeResponse,
  type ProjectOverviewResponse,
} from "./service";
import { Intro, QueryState, Technical } from "./ui";
export type Preferences = {
  theme: "system" | "light" | "dark";
  notifications: boolean;
  endpoint: string;
  connectionName: string;
};
const sections = [
  "Appearance",
  "Notifications",
  "Project",
  "Integrations",
  "Connections & sharing",
  "Advanced",
  "About & diagnostics",
];
export function Settings({
  scope,
  project,
  prefs,
  onPreferences,
  auth,
  onConnect,
  onProject,
}: {
  scope: string;
  project: string;
  prefs: Preferences;
  onPreferences: (prefs: Preferences) => void;
  auth: AuthMeResponse | null;
  onConnect: () => void;
  onProject: () => void;
}) {
  const [section, setSection] = useState("Appearance");
  const [error, setError] = useState("");
  const [message, setMessage] = useState("");
  const overview = useProjectData<ProjectOverviewResponse>(
    scope,
    project,
    "overview",
    !!auth && !!project,
  );
  async function run(action: () => Promise<unknown>) {
    setError("");
    setMessage("");
    try {
      await action();
    } catch (e) {
      setError(errorText(e));
    }
  }
  return (
    <>
      <Intro
        title="Settings"
        subtitle="Desktop preferences, project configuration and deeper controls."
      />
      <div className="settings-layout">
        <nav className="card settings-nav" aria-label="Settings sections">
          {sections.map((item) => (
            <Button
              key={item}
              className={`nav-item ${section === item ? "selected" : ""}`}
              aria-current={section === item ? "page" : undefined}
              onClick={() => {
                setSection(item);
                setError("");
                setMessage("");
              }}
            >
              {item}
            </Button>
          ))}
        </nav>
        <section className="card settings-content">
          <h2>{section}</h2>
          {section === "Appearance" && (
            <>
              <p className="muted">
                Use the same familiar workspace in light and dark.
              </p>
              <h3>Theme</h3>
              <div className="three-columns" role="group" aria-label="Theme">
                {(["system", "light", "dark"] as const).map((theme) => (
                  <Button
                    className={`theme-choice ${prefs.theme === theme ? "selected" : ""}`}
                    key={theme}
                    aria-pressed={prefs.theme === theme}
                    onClick={() => onPreferences({ ...prefs, theme })}
                  >
                    <strong>{theme[0].toUpperCase() + theme.slice(1)}</strong>
                    <span className="caption">
                      {prefs.theme === theme
                        ? "Selected"
                        : theme === "system"
                          ? "Follow desktop"
                          : "Preview theme"}
                    </span>
                  </Button>
                ))}
              </div>
              <hr />
              <h3>Interface density</h3>
              <p className="muted">
                Comfortable — readable labels and generous click targets.
              </p>
              <h3>Motion</h3>
              <p className="muted">
                Follows the desktop’s reduced-motion preference.
              </p>
              <h3>Window behaviour</h3>
              <p className="muted">
                Restore the last project and window size when reopening.
              </p>
              <p className="caption">
                Appearance changes apply to this device only.
              </p>
            </>
          )}
          {section === "Notifications" && (
            <>
              <p className="muted">
                Receive desktop confirmations after you review knowledge
                updates. Message content stays out of notifications.
              </p>
              <label className="check">
                <input
                  type="checkbox"
                  checked={prefs.notifications}
                  onChange={(e) => {
                    const enabled = e.target.checked;
                    void run(async () => {
                      if (
                        enabled &&
                        !(await isPermissionGranted()) &&
                        (await requestPermission()) !== "granted"
                      )
                        throw new Error(
                          "Desktop notifications were not permitted.",
                        );
                      onPreferences({ ...prefs, notifications: enabled });
                    });
                  }}
                />
                Desktop notifications
              </label>
              <Button
                disabled={!prefs.notifications}
                onClick={() =>
                  void run(async () => {
                    if (!(await isPermissionGranted()))
                      throw new Error(
                        "Notification permission is no longer available.",
                      );
                    sendNotification({
                      title: "Codifix",
                      body: "Desktop notifications are enabled.",
                    });
                    setMessage("Test notification sent to the desktop.");
                  })
                }
              >
                Send test notification
              </Button>
            </>
          )}
          {section === "Project" && (
            <>
              <p>
                Selected project: <strong>{project || "None selected"}</strong>
              </p>
              <Button onClick={onProject}>Choose project</Button>
              {project && auth && (
                <QueryState query={overview}>
                  <p className="muted">
                    {overview.data?.active_memories ?? 0} active memories ·{" "}
                    {overview.data?.sessions_total ?? 0} recorded sessions
                  </p>
                  <Technical
                    label="Project configuration & capture status"
                    value={overview.data}
                  />
                </QueryState>
              )}
              <p className="caption">
                Choosing a folder selects an existing Codifix project. It does
                not initialise or modify that folder.
              </p>
            </>
          )}
          {section === "Integrations" && (
            <>
              <p className="muted">
                Codifix’s service manages agent capture, watchers and installed
                skills. Review their permissions in the service interface.
              </p>
              <p>
                The desktop observes sessions and captured knowledge; it does
                not start agents automatically.
              </p>
              <Button
                disabled={!auth}
                onClick={() => void run(() => openService())}
              >
                Open service in browser
              </Button>
            </>
          )}
          {section === "Connections & sharing" && (
            <>
              <h3>{prefs.connectionName || "Codifix service"}</h3>
              <p className="break">
                {prefs.endpoint || "No endpoint configured"}
              </p>
              <p className="muted">
                {auth
                  ? `${auth.principal.display_name} · ${auth.read_only ? "Read only" : auth.authenticated ? "Authenticated" : "Anonymous read access"}`
                  : "Not connected"}
              </p>
              <Button onClick={onConnect}>Manage connection</Button>
              <hr />
              <h3>Sharing & federation</h3>
              <p className="muted">
                Review sharing destinations and permissions in the service
                interface. Changing this connection does not change sharing
                policy.
              </p>
              <Button
                disabled={!auth}
                onClick={() => void run(() => openService())}
              >
                Open service in browser
              </Button>
            </>
          )}
          {section === "Advanced" && (
            <>
              <p className="muted">
                Search indexing, embedding providers and supervisor controls
                belong to the connected service.
              </p>
              {project && auth && (
                <QueryState query={overview}>
                  <dl>
                    <dt>Embedding model</dt>
                    <dd>
                      {overview.data?.active_embedding_model ||
                        "Not configured"}
                    </dd>
                    <dt>Fresh / stale chunks</dt>
                    <dd>
                      {overview.data?.fresh_embedding_chunks ?? 0} /{" "}
                      {overview.data?.stale_embedding_chunks ?? 0}
                    </dd>
                    <dt>Missing embeddings</dt>
                    <dd>{overview.data?.missing_embedding_chunks ?? 0}</dd>
                  </dl>
                </QueryState>
              )}
              <Button
                disabled={!auth}
                onClick={() => void run(() => openService())}
              >
                Open service in browser
              </Button>
            </>
          )}
          {section === "About & diagnostics" && (
            <>
              <h3>Codifix Desktop · 0.1.3</h3>
              <p className="muted">Shared Tauri 2 desktop · Linux first</p>
              <p>
                React and TypeScript provide the shared interface. Rust handles
                service sessions and native integration. Your knowledge stays
                with the Codifix service.
              </p>
              <Technical
                value={{
                  endpoint: prefs.endpoint,
                  project,
                  authenticated: auth?.authenticated,
                  readOnly: auth?.read_only,
                  authMode: auth?.mode,
                  userAgent: navigator.userAgent,
                }}
              />
              <p className="caption">
                Browser-approved credentials are saved in your operating system
                keyring. Pasted service tokens are held in memory only.
              </p>
            </>
          )}
          {error && (
            <p className="notice danger" role="alert">
              {error}
            </p>
          )}
          {message && (
            <p className="notice positive" role="status">
              {message}
            </p>
          )}
        </section>
      </div>
    </>
  );
}
