import { useEffect, useState } from "react";
import { approveDevice, authLoginUrl, deviceReview, getAuthMe } from "../../api";
import type { AuthMeResponse } from "../../types";
import type { DeviceApprovalReceipt, DeviceApprovalSelection, DeviceProjectRole, DeviceReview } from "./types";

const USER_CODE = /^[A-Z0-9]{4}-[A-Z0-9]{4}$/;
function humanSession(auth: AuthMeResponse): boolean {
  return auth.authenticated && auth.mode === "multi_user" && auth.principal.kind === "human_oidc";
}

export function DeviceApprovalPage({ initialCode }: { initialCode: string }) {
  const initial = initialCode.toUpperCase();
  const [code, setCode] = useState(USER_CODE.test(initial) ? initial : "");
  const [auth, setAuth] = useState<AuthMeResponse | null>(null);
  const [checkingIdentity, setCheckingIdentity] = useState(true);
  const [loading, setLoading] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [review, setReview] = useState<DeviceReview | null>(null);
  const [roles, setRoles] = useState<Record<string, DeviceProjectRole>>({});
  const [confirmation, setConfirmation] = useState<DeviceApprovalSelection[] | null>(null);
  const [confirmed, setConfirmed] = useState(false);
  const [receipt, setReceipt] = useState<DeviceApprovalReceipt | null>(null);
  const [now, setNow] = useState(Date.now());

  useEffect(() => {
    let cancelled = false;
    void getAuthMe(true).then((identity) => { if (!cancelled) setAuth(identity); })
      .catch(() => { if (!cancelled) setError("Sign in to verify your human browser session before reviewing this device."); })
      .finally(() => { if (!cancelled) setCheckingIdentity(false); });
    return () => { cancelled = true; };
  }, []);
  useEffect(() => { const timer = window.setInterval(() => setNow(Date.now()), 1000); return () => window.clearInterval(timer); }, []);

  const canReview = auth !== null && humanSession(auth);
  const readOnly = auth?.read_only ?? true;
  const validCode = USER_CODE.test(code);
  const returnTo = `/?device_code=${validCode ? encodeURIComponent(code) : ""}`;
  const expired = review !== null && (!Number.isFinite(Date.parse(review.expires_at)) || now >= Date.parse(review.expires_at));
  const disabled = !canReview || readOnly || expired || loading || busy;

  async function loadReview() {
    if (!canReview || !validCode || busy || readOnly) return;
    setLoading(true); setError(null); setReview(null); setRoles({}); setConfirmation(null); setConfirmed(false); setReceipt(null);
    try {
      const currentAuth = await getAuthMe(true);
      setAuth(currentAuth);
      if (!humanSession(currentAuth) || currentAuth.read_only) throw new Error("A writable OIDC human browser session is required. Local-owner and service credentials cannot approve devices.");
      const next = await deviceReview(code);
      if (next.user_code !== code || !/^[0-9a-f]{64}$/i.test(next.credential_sha256)) throw new Error("Device review identity or fingerprint mismatch. Check the requesting device and start again.");
      if (next.projects.some(({ allowed_roles }) => allowed_roles.some((role) => role !== "reader" && role !== "writer"))) throw new Error("Device review contains an unsupported project role. Review current project access before proceeding.");
      setReview(next); setNow(Date.now());
    } catch (nextError) { setError((nextError as Error).message); }
    finally { setLoading(false); }
  }

  function prepareApproval() {
    if (disabled || !review) return;
    const selected = review.projects.filter(({ manifest }) => roles[manifest.project_id]).map(({ manifest, allowed_roles }) => {
      const role = roles[manifest.project_id];
      if (!allowed_roles.includes(role)) throw new Error("Selected role is no longer available. Review current project access.");
      return { project_id: manifest.project_id, role, revision: manifest.revision };
    });
    if (selected.length) { setConfirmation(selected); setConfirmed(false); setError(null); }
  }

  async function submitApproval() {
    if (disabled || !review || !confirmation?.length || !confirmed) return;
    setBusy(true); setError(null);
    try {
      const currentAuth = await getAuthMe(true);
      setAuth(currentAuth);
      if (!humanSession(currentAuth) || currentAuth.read_only) throw new Error("A writable OIDC human browser session is required. Review this request after signing in.");
      const result = await approveDevice({ user_code: review.user_code, projects: confirmation, confirmed: true });
      if (result.status === "expired") throw new Error("Device request expired. Start a new request on the device.");
      if (result.status === "denied") throw new Error("Device request denied. Review your current human session and project membership.");
      if (result.status !== "approved" || result.device_id !== review.device_id || !result.principal_id
        || result.projects?.length !== confirmation.length || new Set(result.projects.map((project) => project.manifest.project_id)).size !== confirmation.length
        || !confirmation.every((selection) => {
          const approved = result.projects!.find(({ manifest }) => manifest.project_id === selection.project_id);
          const inspected = review.projects.find(({ manifest }) => manifest.project_id === selection.project_id)?.manifest;
          return approved && inspected && approved.role === selection.role && approved.manifest.revision === selection.revision
            && approved.manifest.area === inspected.area && approved.manifest.owner_instance_id === inspected.owner_instance_id;
        })) throw new Error("Device approval receipt mismatch or request no longer approved. Inspect the current request before further action.");
      setReceipt(result); setConfirmation(null); setConfirmed(false);
    } catch (nextError) { setError((nextError as Error).message); setConfirmed(false); }
    finally { setBusy(false); }
  }

  return <div className="app-shell"><header className="topbar"><h1>Memory Layer device approval</h1></header>
    <main className="panel-stack device-approval-page">
      <section className="panel"><h2>Approve a requesting device</h2>
        <p>Verify the name and credential fingerprint shown by the device, then choose only the projects and roles you want to grant. This page approves reviewed local access; remote-instance grants, software installation and selective sync require separate steps.</p>
        {checkingIdentity ? <p role="status">Checking human browser identity…</p> : canReview ? <p>Approving as {auth!.principal.display_name}.</p>
          : <><p>Approval requires an OIDC human browser session. Local-owner sessions and service credentials cannot approve devices.</p><a className="button-link" href={authLoginUrl(returnTo)}>Sign in to review this device</a></>}
        {canReview && readOnly ? <p role="status">This service is read-only. Device approvals are disabled.</p> : null}
        <label>User code<input autoComplete="off" maxLength={9} value={code} disabled={busy || loading || Boolean(confirmation)} onChange={(event) => {
          setCode(event.target.value.toUpperCase()); setReview(null); setRoles({}); setReceipt(null); setError(null);
        }} /></label>
        <button type="button" disabled={!canReview || readOnly || !validCode || busy || loading} onClick={() => void loadReview()}>Review device request</button>
      </section>
      {error ? <div className="panel error-banner" role="alert">{error}</div> : null}
      {review && !receipt ? <>
        <section className="panel"><h2>Device identity</h2><dl><dt>Name</dt><dd>{review.name}</dd><dt>Platform</dt><dd>{review.platform}</dd>
          <dt>Device UUID</dt><dd><code>{review.device_id}</code></dd><dt>Request UUID</dt><dd><code>{review.request_id}</code></dd>
          <dt>Credential SHA-256</dt><dd><code className="device-fingerprint">{review.credential_sha256}</code></dd><dt>Expires</dt><dd>{review.expires_at}</dd></dl>
          {expired ? <p role="alert">This device request has expired. Start a new request on the device.</p> : null}
        </section>
        <section className="panel access-table-section"><h2>Currently authorized reviewed projects</h2>
          <p>Select by area, stable project UUID and owner instance. The available roles are returned by the central service from your current permissions.</p>
          <div className="table-scroll"><table><thead><tr><th>Select</th><th>Area / project</th><th>Owner instance</th><th>Reviewed repository</th><th>Device role</th><th>Offline policy</th></tr></thead>
            <tbody>{review.projects.map(({ manifest, allowed_roles, offline_available }) => <tr key={manifest.project_id}>
              <td><input aria-label={`Select ${manifest.area} project ${manifest.project_id}`} type="checkbox" checked={Boolean(roles[manifest.project_id])} disabled={disabled || Boolean(confirmation) || !allowed_roles.length}
                onChange={(event) => setRoles((current) => { const updated = { ...current }; if (event.target.checked) updated[manifest.project_id] = allowed_roles[0]; else delete updated[manifest.project_id]; return updated; })} /></td>
              <td>{manifest.area} / {manifest.project}<br /><code>{manifest.project_id}</code><br />Revision {manifest.revision}</td><td><code>{manifest.owner_instance_id}</code></td>
              <td>{manifest.repository_url}<br />{manifest.default_branch}<br />Tools: {manifest.tools.map((tool) => `${tool.name} ${tool.version}`).join(", ") || "none"}<br />Destinations: {manifest.destinations.map((destination) => `${destination.provider_id}/${destination.destination_id}`).join(", ") || "none"}</td>
              <td><select aria-label={`Role for project ${manifest.project_id}`} value={roles[manifest.project_id] ?? allowed_roles[0] ?? ""} disabled={disabled || Boolean(confirmation) || !roles[manifest.project_id]} onChange={(event) => {
                const role = event.target.value as DeviceProjectRole; if (allowed_roles.includes(role)) setRoles({ ...roles, [manifest.project_id]: role });
              }}>{allowed_roles.map((role) => <option key={role} value={role}>{role}</option>)}</select></td>
              <td>{manifest.area === "trust" ? "Trust — remote-only; no offline copy" : manifest.offline_allowed && offline_available ? "Offline policy allows this project" : "Offline data not available"}</td>
            </tr>)}</tbody></table></div>
          {!review.projects.length ? <p>No authorized reviewed projects are available. Ask your administrator to review project policy and current membership.</p> : null}
          <button type="button" disabled={disabled || !Object.keys(roles).length || Boolean(confirmation)} onClick={prepareApproval}>Review selected access</button>
        </section>
        {confirmation ? <section className="panel" aria-label="Exact device approval"><h2>Confirm exact device access</h2>
          <p>Device {review.name} ({review.device_id}), request {review.request_id}, credential SHA-256 {review.credential_sha256}, expires {review.expires_at}.</p>
          <ul>{confirmation.map((selection) => { const manifest = review.projects.find((project) => project.manifest.project_id === selection.project_id)!.manifest;
            return <li key={selection.project_id}>{manifest.area} · project {selection.project_id} · owner {manifest.owner_instance_id} · revision {selection.revision} · {selection.role}{manifest.area === "trust" ? " · remote-only" : ""}</li>;
          })}</ul>
          <label className="access-checkbox"><input type="checkbox" checked={confirmed} disabled={disabled} onChange={(event) => setConfirmed(event.target.checked)} />I verified this device fingerprint and the exact project identities and roles.</label>
          <div className="actions-row"><button type="button" disabled={disabled || !confirmed} onClick={() => void submitApproval()}>Approve selected device access</button>
            <button type="button" disabled={busy} onClick={() => { setConfirmation(null); setConfirmed(false); }}>Change selection</button></div>
        </section> : null}
      </> : null}
      {receipt ? <section className="panel" role="status"><h2>Device approval recorded</h2><p>Device {receipt.device_id}. Return to the requesting device to continue. This receipt does not establish installation, remote-instance access or selective sync.</p></section> : null}
      <a className="button-link" href="/">Return to Memory Layer</a>
    </main>
  </div>;
}
