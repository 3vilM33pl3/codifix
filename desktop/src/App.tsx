import { Button, Navigation } from "./ui";
import { useEffect, useRef, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { ArrowRight, ChevronDown, FolderOpen, Search } from "lucide-react";
import {
  isPermissionGranted,
  sendNotification,
} from "@tauri-apps/plugin-notification";
import { Home } from "./Home";
import { Memories } from "./Memories";
import { Sessions } from "./Sessions";
import { Activity } from "./Activity";
import { Review } from "./Review";
import { Settings, type Preferences } from "./Settings";
import { Badge, Intro, Modal } from "./ui";
import {
  chooseProject,
  connect,
  disconnect,
  errorText,
  startBrowserLogin,
  pollBrowserLogin,
  openBrowserLogin,
  cancelBrowserLogin,
  forgetBrowserLogin,
  time,
  type BrowserLoginProgress,
  projectPath,
  request,
  type AuthMeResponse,
  type ProjectOverviewResponse,
} from "./service";
type Recent = { project: string; endpoint: string; path: string | null };
const defaultPrefs: Preferences = {
  theme: "system",
  notifications: false,
  endpoint: "http://127.0.0.1:4250",
  connectionName: "Local service",
};
function readPrefs(): Preferences {
  try {
    const p = JSON.parse(
      localStorage.getItem("codifix.desktop.preferences") ?? "null",
    );
    return {
      ...defaultPrefs,
      ...p,
      theme: ["system", "light", "dark"].includes(p?.theme)
        ? p.theme
        : "system",
    };
  } catch {
    return defaultPrefs;
  }
}
function readRecents(): Recent[] {
  try {
    const r: unknown = JSON.parse(
      localStorage.getItem("codifix.desktop.projects") ?? "[]",
    );
    return Array.isArray(r)
      ? r
          .filter(
            (x): x is Recent =>
              x &&
              typeof x.project === "string" &&
              typeof x.endpoint === "string" &&
              (x.path === null || typeof x.path === "string"),
          )
          .slice(0, 12)
      : [];
  } catch {
    return [];
  }
}
export function App() {
  const [prefs, setPrefs] = useState(readPrefs);
  const [recents, setRecents] = useState(readRecents);
  const [project, setProject] = useState("");
  const [repoRoot, setRepoRoot] = useState<string | null>(null);
  const [auth, setAuth] = useState<AuthMeResponse | null>(null);
  const [scope, setScope] = useState("initial");
  const [page, setPage] = useState("Home");
  const [initialId, setInitialId] = useState<string>();
  const [modal, setModal] = useState<
    "connection" | "project" | "palette" | "help" | null
  >(null);
  const [connectionError, setConnectionError] = useState("");
  const [connectionInfo, setConnectionInfo] = useState("");
  const [connectBusy, setConnectBusy] = useState(false);
  const [endpoint, setEndpoint] = useState(prefs.endpoint);
  const [connectionName, setConnectionName] = useState(prefs.connectionName);
  const [token, setToken] = useState("");
  const [browserLogin, setBrowserLogin] = useState<BrowserLoginProgress | null>(
    null,
  );
  const [polling, setPolling] = useState(false);
  const [forgetConfirmed, setForgetConfirmed] = useState(false);
  const [slug, setSlug] = useState("");
  const [folder, setFolder] = useState<string | null>(null);
  const [projectError, setProjectError] = useState("");
  const [projectBusy, setProjectBusy] = useState(false);
  const [search, setSearch] = useState("");
  const [commandIndex, setCommandIndex] = useState(0);
  const [toast, setToast] = useState("");
  const client = useQueryClient();
  const generation = useRef(0);
  const started = useRef(false);
  const main = useRef<HTMLElement>(null);
  const health = useQuery({
    queryKey: [scope, "health"],
    queryFn: () => request<AuthMeResponse>("/v1/auth/me"),
    enabled: !!auth,
    retry: false,
    refetchInterval: 15_000,
  });
  const currentAuth = health.data ?? auth;
  const connected = !!auth && !health.error;
  function savePrefs(value: Preferences) {
    setPrefs(value);
    localStorage.setItem("codifix.desktop.preferences", JSON.stringify(value));
  }
  useEffect(() => {
    document.documentElement.dataset.theme = prefs.theme;
  }, [prefs.theme]);
  useEffect(() => {
    const fn = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setModal((m) => (m === "palette" ? null : "palette"));
        setSearch("");
        setCommandIndex(0);
      }
    };
    window.addEventListener("keydown", fn);
    return () => window.removeEventListener("keydown", fn);
  }, []);
  useEffect(() => {
    if (toast) {
      const id = setTimeout(() => setToast(""), 6000);
      return () => clearTimeout(id);
    }
  }, [toast]);
  function navigate(target: string, id?: string) {
    setPage(target);
    setInitialId(id);
    setModal(null);
    requestAnimationFrame(() =>
      main.current?.querySelector<HTMLElement>("h1")?.focus(),
    );
  }
  async function setActiveProject(
    value: string,
    path: string | null,
    connection = prefs.endpoint,
    expected = generation.current,
  ) {
    const clean = value.trim();
    if (!clean) throw new Error("Enter a Codifix project slug.");
    const loaded = await request<ProjectOverviewResponse>(
      projectPath(clean, "overview"),
    );
    if (expected !== generation.current) return;
    if (loaded.project !== clean)
      throw new Error(
        "The service returned a different project. Selection was not changed.",
      );
    await client.cancelQueries();
    client.clear();
    setProject(clean);
    setRepoRoot(path);
    setPage("Home");
    setInitialId(undefined);
    setModal(null);
    const next = [
      { project: clean, endpoint: connection, path },
      ...readRecents().filter(
        (r) => !(r.project === clean && r.endpoint === connection),
      ),
    ].slice(0, 12);
    setRecents(next);
    localStorage.setItem("codifix.desktop.projects", JSON.stringify(next));
  }
  async function acceptConnection(
    identity: AuthMeResponse,
    address: string,
    label: string,
    version: number,
    restore = true,
  ) {
    if (version !== generation.current) return;
    const normalized = address.trim().replace(/\/$/, "");
    savePrefs({
      ...prefs,
      endpoint: normalized,
      connectionName: label.trim() || "Codifix service",
    });
    setEndpoint(normalized);
    setAuth(identity);
    setScope(`${normalized}:${version}`);
    if (identity.mode === "multi_user" && !identity.authenticated) {
      setConnectionError(
        "This service requires authentication. Sign in through your browser to connect this desktop session.",
      );
      setModal("connection");
      return;
    }
    setModal(null);
    const last = restore
      ? readRecents().find((r) => r.endpoint === normalized)
      : null;
    if (last) {
      try {
        await setActiveProject(last.project, last.path, normalized, version);
      } catch (e) {
        if (version === generation.current) setProjectError(errorText(e));
      }
    }
  }
  async function beginBrowserLogin() {
    const version = ++generation.current;
    setConnectBusy(true);
    setConnectionError("");
    setConnectionInfo("");
    setBrowserLogin(null);
    setPolling(false);
    setAuth(null);
    setProject("");
    setRepoRoot(null);
    setToken("");
    await client.cancelQueries();
    client.clear();
    try {
      const progress = await startBrowserLogin(endpoint.trim());
      if (version !== generation.current) return;
      if (progress.status === "approved" && progress.auth) {
        await acceptConnection(
          progress.auth,
          endpoint,
          connectionName,
          version,
        );
        setConnectBusy(false);
      } else {
        setBrowserLogin(progress);
        setPolling(true);
        try {
          await openBrowserLogin(progress.request_id);
        } catch (e) {
          if (version === generation.current) setConnectionError(errorText(e));
        }
      }
    } catch (e) {
      if (version === generation.current) {
        setConnectionError(errorText(e));
        setConnectBusy(false);
      }
    }
  }
  async function signOutBrowser(forget = false) {
    const version = ++generation.current;
    setPolling(false);
    setBrowserLogin(null);
    setConnectBusy(true);
    setAuth(null);
    setProject("");
    setRepoRoot(null);
    setConnectionError("");
    setConnectionInfo("");
    await client.cancelQueries();
    client.clear();
    try {
      if (forget) {
        await forgetBrowserLogin(endpoint.trim());
        setForgetConfirmed(false);
        setConnectionInfo(
          "Saved sign-in removed from this device. Remote revocation was not confirmed.",
        );
      } else {
        await cancelBrowserLogin(endpoint.trim());
        setConnectionInfo("Browser access signed out on this device.");
      }
    } catch (e) {
      if (version === generation.current) setConnectionError(errorText(e));
    } finally {
      if (version === generation.current) setConnectBusy(false);
    }
  }
  useEffect(() => {
    if (!browserLogin || !polling) return;
    const version = generation.current;
    let stopped = false;
    let timer: ReturnType<typeof setTimeout>;
    const poll = async () => {
      try {
        const progress = await pollBrowserLogin(browserLogin.request_id);
        if (stopped || version !== generation.current) return;
        if (progress.status === "approved" && progress.auth) {
          setPolling(false);
          setBrowserLogin(null);
          await acceptConnection(
            progress.auth,
            endpoint,
            connectionName,
            version,
          );
          if (version === generation.current) setConnectBusy(false);
        } else {
          timer = setTimeout(poll, 3000);
        }
      } catch (e) {
        if (!stopped && version === generation.current) {
          setConnectionError(errorText(e));
          setPolling(false);
        }
      }
    };
    timer = setTimeout(poll, 3000);
    return () => {
      stopped = true;
      clearTimeout(timer);
    };
  }, [browserLogin, polling]);
  async function connectToService(
    address: string,
    credential: string | null,
    label: string,
    restore = false,
  ) {
    const version = ++generation.current;
    setBrowserLogin(null);
    setPolling(false);
    setConnectBusy(true);
    setConnectionError("");
    setConnectionInfo("");
    setAuth(null);
    setProject("");
    setRepoRoot(null);
    setToken("");
    await client.cancelQueries();
    client.clear();
    try {
      const identity = await connect(
        address.trim(),
        credential?.trim() || null,
      );
      if (version !== generation.current) return;
      await acceptConnection(identity, address, label, version, restore);
    } catch (e) {
      if (version === generation.current) {
        setConnectionError(errorText(e));
        setModal("connection");
      }
    } finally {
      if (version === generation.current) setConnectBusy(false);
    }
  }
  useEffect(() => {
    if (!started.current) {
      started.current = true;
      void connectToService(prefs.endpoint, null, prefs.connectionName, true);
    }
  }, []);
  async function selectProject(value: string, path: string | null) {
    setProjectBusy(true);
    setProjectError("");
    try {
      await setActiveProject(value, path);
    } catch (e) {
      setProjectError(errorText(e));
    } finally {
      setProjectBusy(false);
    }
  }
  async function pickFolder() {
    setProjectError("");
    try {
      const result = await chooseProject();
      if (result) {
        setFolder(result.path);
        if (result.slug) setSlug(result.slug);
        else {
          setSlug("");
          setProjectError(
            "No Codifix project marker was found. Enter the existing service project slug below.",
          );
        }
      }
    } catch (e) {
      setProjectError(errorText(e));
    }
  }
  async function notify(message: string) {
    setToast(message);
    if (prefs.notifications) {
      try {
        if (await isPermissionGranted())
          sendNotification({ title: "Codifix", body: message });
      } catch {
        /* The successful service operation remains successful if desktop notification fails. */
      }
    }
  }
  const commands = [
    {
      label: "Go to Memories",
      hint: "Find project knowledge",
      action: () => navigate("Memories"),
      needsProject: true,
    },
    {
      label: "Review pending updates",
      hint: "Compare proposed knowledge changes",
      action: () => navigate("Review"),
      needsProject: true,
    },
    {
      label: "Go to Sessions",
      hint: "Resume a project briefing",
      action: () => navigate("Sessions"),
      needsProject: true,
    },
    {
      label: "Go to Activity",
      hint: "Captures, reviews and background work",
      action: () => navigate("Activity"),
      needsProject: true,
    },
    {
      label: "Switch project",
      hint: "Choose a recent project or open a folder",
      action: () => {
        setModal("project");
        setProjectError("");
      },
      needsProject: false,
    },
    {
      label: "Open Settings",
      hint: "Desktop and project preferences",
      action: () => navigate("Settings"),
      needsProject: false,
    },
  ].filter(
    (c) =>
      c.label.toLowerCase().includes(search.toLowerCase()) &&
      (!c.needsProject || !!project),
  );
  const connectionLabel = connectBusy
    ? "Connecting…"
    : health.error
      ? "Connection unavailable"
      : auth
        ? auth.mode === "multi_user" && !auth.authenticated
          ? "Sign-in required"
          : "Connected"
        : "Not connected";
  return (
    <div className="app">
      <a href="#main-content" className="skip-link">
        Skip to content
      </a>
      <aside className="sidebar">
        <div className="brand">
          <span className="brand-mark">c</span>
          <span>Codifix</span>
        </div>
        <Button
          className="project-selector"
          onClick={() => {
            setModal("project");
            setProjectError("");
          }}
        >
          <span className="row">
            <strong>{project || "Choose a project"}</strong>
            <ChevronDown size={16} />
          </span>
          <span className="caption">{prefs.connectionName}</span>
        </Button>
        <nav aria-label="Main navigation">
          {["Home", "Memories", "Sessions", "Activity"].map((item) => (
            <Navigation
              key={item}
              className={`nav-item ${page === item || (item === "Home" && page === "Review") ? "selected" : ""}`}
              aria-current={page === item ? "page" : undefined}
              onClick={() => navigate(item)}
            >
              {item}
            </Navigation>
          ))}
        </nav>
        <div className="sidebar-footer">
          <Button
            className={`connection-chip ${connected ? "positive" : "warning"}`}
            onClick={() => setModal("connection")}
          >
            {connectionLabel}
          </Button>
          <Button
            className={`nav-item ${page === "Settings" ? "selected" : ""}`}
            onClick={() => navigate("Settings")}
          >
            Settings
          </Button>
          <Button className="ghost help" onClick={() => setModal("help")}>
            Help & keyboard shortcuts
          </Button>
        </div>
      </aside>
      <div className="workspace">
        <header className="toolbar">
          <span className="breadcrumb">
            {prefs.connectionName} <span>/</span>{" "}
            {project || "No project selected"}
          </span>
          <Button
            className="search-jump"
            onClick={() => {
              setSearch("");
              setCommandIndex(0);
              setModal("palette");
            }}
          >
            Search or jump… <kbd>Ctrl K</kbd>
          </Button>
        </header>
        <main ref={main} id="main-content" className="content" tabIndex={-1}>
          {health.error && auth && (
            <div className="notice warning" role="status">
              Connection unavailable. Previously loaded results may be out of
              date. Actions are disabled until the connection recovers.{" "}
              <Button onClick={() => void health.refetch()}>Reconnect</Button>
            </div>
          )}
          {page === "Settings" ? (
            <Settings
              scope={scope}
              project={project}
              prefs={prefs}
              onPreferences={savePrefs}
              auth={currentAuth}
              onConnect={() => setModal("connection")}
              onProject={() => setModal("project")}
            />
          ) : !project ? (
            <>
              <Intro
                title="Bring your project knowledge together"
                subtitle="Start with a project folder, then connect the agents you use."
              />
              <section className="card welcome">
                <div className="welcome-mark">
                  <FolderOpen size={30} />
                </div>
                <h2>A calmer place to pick up your work</h2>
                <p className="muted">
                  Find previous decisions, follow active sessions and review
                  what your agents have learned.
                </p>
                <div className="welcome-steps">
                  {[
                    ["1", "Choose a project", "Use a folder on this computer."],
                    [
                      "2",
                      "Connect your agents",
                      "Review the available integrations and permissions.",
                    ],
                    [
                      "3",
                      "Build useful memory",
                      "Captured knowledge appears here as you work.",
                    ],
                  ].map(([number, title, body]) => (
                    <div className="welcome-step" key={number}>
                      <Badge>{number}</Badge>
                      <div>
                        <strong>{title}</strong>
                        <p className="muted">{body}</p>
                      </div>
                    </div>
                  ))}
                </div>
                <Button
                  className="primary"
                  onClick={() => setModal(auth ? "project" : "connection")}
                >
                  {auth ? "Choose project" : "Connect to Codifix"}
                  <ArrowRight size={16} />
                </Button>
                {projectError && <p role="alert">{projectError}</p>}
                <p className="caption">
                  Your project knowledge will appear after the first capture.
                </p>
              </section>
            </>
          ) : (
            <div
              className="page"
              key={`${scope}:${project}:${page}:${initialId ?? ""}`}
            >
              {page === "Home" && (
                <Home scope={scope} project={project} navigate={navigate} />
              )}{" "}
              {page === "Memories" && (
                <Memories
                  scope={scope}
                  project={project}
                  initialId={initialId}
                />
              )}{" "}
              {page === "Sessions" && (
                <Sessions
                  scope={scope}
                  project={project}
                  repoRoot={repoRoot}
                  connected={connected}
                />
              )}{" "}
              {page === "Activity" && currentAuth && (
                <Activity
                  scope={scope}
                  project={project}
                  auth={currentAuth}
                  connected={connected}
                  onReview={() => navigate("Review")}
                />
              )}{" "}
              {page === "Review" && currentAuth && (
                <Review
                  scope={scope}
                  project={project}
                  auth={currentAuth}
                  connected={connected}
                  onBack={() => navigate("Home")}
                  notify={(message) => void notify(message)}
                />
              )}
            </div>
          )}
        </main>
      </div>
      <Modal
        open={modal === "connection"}
        title="Connect to Codifix"
        description="Choose the service that holds your project knowledge."
        onClose={() => !connectBusy && setModal(null)}
      >
        <form
          className="form"
          onSubmit={(e) => {
            e.preventDefault();
            void connectToService(endpoint, token, connectionName, true);
          }}
        >
          <label>
            Connection name
            <input
              value={connectionName}
              onChange={(e) => setConnectionName(e.target.value)}
              placeholder="Workshop"
              disabled={connectBusy}
            />
          </label>
          <label>
            Service URL
            <input
              type="url"
              required
              value={endpoint}
              onChange={(e) => {
                setEndpoint(e.target.value);
                setForgetConfirmed(false);
              }}
              disabled={connectBusy}
            />
          </label>
          {connectionError && (
            <p className="notice danger" role="alert">
              {connectionError}
            </p>
          )}
          {connectionInfo && (
            <p className="notice" role="status">
              {connectionInfo}
            </p>
          )}
          {browserLogin ? (
            <section className="card" aria-label="Browser sign-in">
              <h3>
                {polling ? "Waiting for browser approval…" : "Sign-in paused"}
              </h3>
              <p>
                Sign in and approve <strong>Codifix Desktop</strong> in your
                browser. Match this code and fingerprint, then select the
                projects you want to access.
              </p>
              <p>
                User code: <strong>{browserLogin.user_code}</strong>
              </p>
              <p className="caption break">
                Credential SHA-256: {browserLogin.credential_sha256}
              </p>
              <p className="caption break">Device: {browserLogin.device_id}</p>
              <p className="caption">Expires {time(browserLogin.expires_at)}</p>
              <p className="caption break">
                Approval address: {browserLogin.verification_uri}
              </p>
              <div className="actions">
                <Button
                  type="button"
                  onClick={() =>
                    void openBrowserLogin(browserLogin.request_id).catch((e) =>
                      setConnectionError(errorText(e)),
                    )
                  }
                >
                  Open browser again
                </Button>
                {!polling && (
                  <Button
                    type="button"
                    onClick={() => {
                      setConnectionError("");
                      setConnectionInfo("");
                      setPolling(true);
                    }}
                  >
                    Check approval again
                  </Button>
                )}
                <Button type="button" onClick={() => void signOutBrowser()}>
                  Cancel sign-in
                </Button>
              </div>
            </section>
          ) : (
            <>
              <Button
                className="primary"
                type="button"
                disabled={connectBusy}
                onClick={() => void beginBrowserLogin()}
              >
                {connectBusy ? "Connecting…" : "Sign in through browser"}
              </Button>
              <p className="caption">
                Use your service’s public HTTPS address. Browser sign-in
                connects this desktop and saves its credential in your operating
                system keyring. Your password stays in the browser.
              </p>
              <details>
                <summary>Local service or service token</summary>
                <label>
                  Service token <span className="caption">(if required)</span>
                  <input
                    type="password"
                    autoComplete="off"
                    value={token}
                    onChange={(e) => setToken(e.target.value)}
                    disabled={connectBusy}
                  />
                </label>
                <p className="caption">
                  Pasted tokens stay in memory for this session. Local
                  single-user services use a native session.
                </p>
                <Button type="submit" disabled={connectBusy}>
                  Connect locally or with token
                </Button>
              </details>
              <div className="actions">
                {auth && (
                  <Button
                    type="button"
                    disabled={connectBusy}
                    onClick={() =>
                      void (async () => {
                        generation.current++;
                        await disconnect();
                        await client.cancelQueries();
                        client.clear();
                        setAuth(null);
                        setProject("");
                        setConnectionError("");
                        setConnectionInfo("");
                        setModal(null);
                      })().catch((e) => setConnectionError(errorText(e)))
                    }
                  >
                    Disconnect
                  </Button>
                )}
                <Button
                  type="button"
                  disabled={connectBusy}
                  onClick={() => void signOutBrowser()}
                >
                  Sign out browser access
                </Button>
              </div>
              {connectionError && (
                <details>
                  <summary>Recover an unavailable sign-in</summary>
                  <p className="caption">
                    If the service cannot confirm cancellation, you can remove
                    the saved sign-in here. This does not revoke server access.
                    Resolve any outstanding device approval in your browser or
                    with your administrator.
                  </p>
                  <label className="check">
                    <input
                      type="checkbox"
                      checked={forgetConfirmed}
                      onChange={(e) => setForgetConfirmed(e.target.checked)}
                    />
                    I understand that remote access may remain active.
                  </label>
                  <Button
                    type="button"
                    disabled={connectBusy || !forgetConfirmed}
                    onClick={() => void signOutBrowser(true)}
                  >
                    Forget saved sign-in locally
                  </Button>
                </details>
              )}
              <p className="caption">
                Sign out revokes this desktop’s browser-approved access and
                removes its saved credential. Disconnect keeps it available for
                next time.
              </p>
            </>
          )}
        </form>
      </Modal>
      <Modal
        open={modal === "project"}
        title="Choose a project"
        description="The selected project scopes memories, sessions and actions."
        onClose={() => !projectBusy && setModal(null)}
      >
        {!connected ? (
          <>
            <p className="notice warning">
              Connect to your Codifix service before choosing a project.
            </p>
            <Button onClick={() => setModal("connection")}>
              Manage connection
            </Button>
          </>
        ) : (
          <>
            <div className="project-list">
              {recents
                .filter((r) => r.endpoint === prefs.endpoint)
                .map((r) => (
                  <Button
                    key={r.project}
                    className={`result ${r.project === project ? "selected" : ""}`}
                    disabled={projectBusy}
                    onClick={() => void selectProject(r.project, r.path)}
                  >
                    <strong>{r.project}</strong>
                    <span className="caption">
                      {prefs.connectionName}
                      {r.project === project ? " · Current project" : ""}
                    </span>
                  </Button>
                ))}
            </div>
            <form
              className="form"
              onSubmit={(e) => {
                e.preventDefault();
                void selectProject(slug, folder);
              }}
            >
              <Button
                type="button"
                disabled={projectBusy}
                onClick={() => void pickFolder()}
              >
                <FolderOpen size={18} />
                Open project folder
              </Button>
              {folder && <p className="caption break">{folder}</p>}
              <label>
                Project slug
                <input
                  autoComplete="off"
                  required
                  placeholder="e.g. codifix"
                  value={slug}
                  onChange={(e) => {
                    setSlug(e.target.value);
                    setFolder(null);
                  }}
                  list="granted-projects"
                  disabled={projectBusy}
                />
                <datalist id="granted-projects">
                  {currentAuth?.principal.projects.map((p) => (
                    <option key={p.project} value={p.project} />
                  ))}
                </datalist>
              </label>
              <p className="caption">
                Use an existing project slug from this service. A folder’s
                Codifix marker can fill it in for you.
              </p>
              {projectError && (
                <p role="alert" className="notice danger">
                  {projectError}
                </p>
              )}
              <Button className="primary" disabled={projectBusy}>
                {projectBusy ? "Opening…" : "Open project"}
              </Button>
            </form>
          </>
        )}
      </Modal>
      <Modal
        open={modal === "palette"}
        title="Search or jump"
        description="One keyboard shortcut for projects, memories and actions."
        onClose={() => setModal(null)}
      >
        <label className="search-field">
          <Search size={18} />
          <input
            autoFocus
            aria-label="Search commands"
            value={search}
            placeholder="Search projects, memories or actions…"
            onChange={(e) => {
              setSearch(e.target.value);
              setCommandIndex(0);
            }}
            onKeyDown={(e) => {
              if (e.key === "ArrowDown") {
                e.preventDefault();
                setCommandIndex((i) => Math.min(i + 1, commands.length - 1));
              }
              if (e.key === "ArrowUp") {
                e.preventDefault();
                setCommandIndex((i) => Math.max(i - 1, 0));
              }
              if (e.key === "Enter" && commands[commandIndex]) {
                e.preventDefault();
                commands[commandIndex].action();
              }
            }}
            aria-controls="command-results"
            aria-activedescendant={
              commands[commandIndex] ? `command-${commandIndex}` : undefined
            }
            role="combobox"
            aria-expanded="true"
            aria-autocomplete="list"
          />
        </label>
        <div id="command-results" role="listbox" aria-label="Commands">
          {commands.map((c, i) => (
            <Button
              id={`command-${i}`}
              role="option"
              aria-selected={commandIndex === i}
              key={c.label}
              className={`result ${commandIndex === i ? "selected" : ""}`}
              onClick={c.action}
            >
              <strong>{c.label}</strong>
              <span className="caption">{c.hint}</span>
            </Button>
          ))}
        </div>
        {!commands.length && (
          <p className="muted">
            No matching actions. Open Memories to search project knowledge.
          </p>
        )}
      </Modal>
      <Modal
        open={modal === "help"}
        title="Help & keyboard shortcuts"
        description="Codifix keeps everyday work within four destinations."
        onClose={() => setModal(null)}
      >
        <dl>
          <dt>Search or jump</dt>
          <dd>Ctrl K (Command K on macOS)</dd>
          <dt>Move through controls</dt>
          <dd>Tab / Shift Tab</dd>
          <dt>Command palette</dt>
          <dd>Up / Down arrows, then Enter</dd>
          <dt>Close a dialog</dt>
          <dd>Escape</dd>
        </dl>
        <p className="muted">
          Settings contains project configuration, integrations, connections and
          advanced service controls.
        </p>
      </Modal>
      {toast && (
        <div className="toast" role="status">
          {toast}
        </div>
      )}
    </div>
  );
}
