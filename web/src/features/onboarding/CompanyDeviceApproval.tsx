import { useEffect, useRef, useState } from "react";
import { approveCompanyDevice, authLoginUrl, companyDeviceReview, getAuthMe } from "../../api";
import type { AuthMeResponse } from "../../types";
import type { OnboardingManifest } from "../access/types";
import type { CompanyDeviceApprovalRequest, CompanyDeviceReview } from "./types";

const CODE = /^[A-Z0-9]{4}-[A-Z0-9]{4}$/;
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const SHA = /^[0-9a-f]{64}$/i;
const key = (m: OnboardingManifest) => `${m.area}/${m.owner_instance_id}/${m.project_id}`;
const human = (a: AuthMeResponse) => a.authenticated && a.mode === "multi_user" && a.principal.kind === "human_oidc";
function stable(value: unknown): string {
  if (Array.isArray(value)) return `[${value.map(stable).join(",")}]`;
  if (value && typeof value === "object") return `{${Object.entries(value).sort(([a], [b]) => a.localeCompare(b)).map(([k, v]) => `${JSON.stringify(k)}:${stable(v)}`).join(",")}}`;
  return JSON.stringify(value);
}
function validReview(r: CompanyDeviceReview, code: string): boolean {
  try { return r.user_code === code && UUID.test(r.company_id) && UUID.test(r.request_id) && UUID.test(r.device_id) && SHA.test(r.credential_sha256)
    && typeof r.name === "string" && typeof r.platform === "string"
    && Number.isFinite(Date.parse(r.expires_at)) && Number.isFinite(Date.parse(r.observed_at)) && Array.isArray(r.projects)
    && new Set(r.projects.map(p => key(p.project.manifest))).size === r.projects.length
    && r.projects.every(p => SHA.test(p.manifest_sha256) && UUID.test(p.project.manifest.owner_instance_id) && UUID.test(p.project.manifest.project_id)
      && ["workshop", "homelab", "general", "trust"].includes(p.project.manifest.area)
      && Number.isSafeInteger(p.project.manifest.revision) && p.project.manifest.revision > 0
      && typeof p.project.offline_available === "boolean" && typeof p.project.manifest.offline_allowed === "boolean"
      && [p.project.manifest.project, p.project.manifest.repository_url, p.project.manifest.default_branch, p.project.manifest.reviewed_by, p.project.manifest.reviewed_at].every(s => typeof s === "string")
      && Array.isArray(p.project.manifest.tools) && p.project.manifest.tools.every(t => [t.name, t.version, t.url].every(s => typeof s === "string") && SHA.test(t.sha256))
      && Array.isArray(p.project.manifest.destinations) && p.project.manifest.destinations.every(d => [d.name, d.provider_id, d.destination_id].every(s => typeof s === "string"))
      && p.project.allowed_roles.length === 1 && p.project.allowed_roles[0] === "reader"); }
  catch { return false; }
}

export function CompanyDeviceApproval({ initialCode, onLock }: { initialCode: string; onLock: (locked: boolean) => void }) {
  const [code, setCode] = useState(CODE.test(initialCode.toUpperCase()) ? initialCode.toUpperCase() : "");
  const [auth, setAuth] = useState<AuthMeResponse | null>(null);
  const [review, setReview] = useState<CompanyDeviceReview | null>(null);
  const [identity, setIdentity] = useState<string | null>(null);
  const [selection, setSelection] = useState<Record<string, boolean>>({});
  const [contributions, setContributions] = useState<Record<string, boolean>>({});
  const [confirmation, setConfirmation] = useState<CompanyDeviceApprovalRequest | null>(null);
  const [confirmed, setConfirmed] = useState(false);
  const [attempted, setAttempted] = useState(false);
  const [busy, setBusy] = useState(false);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [message, setMessage] = useState("");
  const [approved, setApproved] = useState(false);
  const [now, setNow] = useState(Date.now());
  const request = useRef<AbortController | null>(null);
  const generation = useRef(0);
  useEffect(() => {
    let cancelled = false;
    void getAuthMe(true).then(a => { if (!cancelled) setAuth(a); }).catch(() => { if (!cancelled) setError("Sign in to verify a current human browser session."); });
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => { cancelled = true; generation.current++; request.current?.abort(); window.clearInterval(timer); };
  }, []);
  useEffect(() => { onLock(Boolean(confirmation) || busy); return () => onLock(false); }, [confirmation, busy, onLock]);
  const allowed = auth !== null && human(auth) && !auth.read_only;
  const expired = review !== null && now >= Date.parse(review.expires_at);
  const disabled = !allowed || busy || loading || expired;
  function cancelReview() { generation.current++; request.current?.abort(); setLoading(false); setReview(null); setIdentity(null); setSelection({}); setContributions({}); setError(""); }
  async function currentHuman() {
    let a: AuthMeResponse;
    try { a = await getAuthMe(true); } catch { throw new Error("Human authentication unavailable. No approval is confirmed."); }
    setAuth(a);
    if (!human(a) || a.read_only) throw new Error("A writable OIDC human browser session is required; local-owner/service credentials cannot approve company devices.");
    return a;
  }
  async function load() {
    if (!allowed || !CODE.test(code) || confirmation || busy) return;
    cancelReview(); setLoading(true); setApproved(false); setMessage(""); setAttempted(false); setConfirmed(false);
    const controller = new AbortController(); request.current = controller; const version = generation.current;
    try {
      const a = await currentHuman(); if (controller.signal.aborted || version !== generation.current) return;
      const value = await companyDeviceReview(code, controller.signal);
      if (controller.signal.aborted || version !== generation.current) return;
      if (!validReview(value, code)) throw new Error("Company review identity, composite scope, reader role or manifest digest invalid. No access selected.");
      setReview(value); setIdentity(a.principal.id); setNow(Date.now());
    } catch (e) { if (!controller.signal.aborted && version === generation.current) setError(e instanceof Error ? e.message : "Company review unavailable."); }
    finally { if (!controller.signal.aborted && version === generation.current) setLoading(false); }
  }
  function prepare() {
    if (disabled || !review || confirmation) return;
    const projects: CompanyDeviceApprovalRequest["projects"] = [];
    for (const p of review.projects) {
      const m = p.project.manifest;
      if (!selection[key(m)]) continue;
      if (m.area === "trust" || !m.offline_allowed || !p.project.offline_available) return;
      projects.push({ area: m.area, owner_instance_id: m.owner_instance_id, project_id: m.project_id, manifest_revision: m.revision, manifest_sha256: p.manifest_sha256, offline_confirmed: true, ...(p.curated_contributions_available && contributions[key(m)] ? { contribution_confirmed: true } : {}) });
    }
    if (projects.length > 32) { setError("Select at most 32 exact owner projects for one device request."); return; }
    if (projects.length) { setConfirmation({ user_code: code, confirmed: true, projects }); setConfirmed(false); setError(""); }
  }
  async function approve() {
    if (disabled || !confirmed || !confirmation || !review) return;
    const controller = new AbortController(); request.current = controller; const version = generation.current;
    setBusy(true); setError(""); setMessage("");
    try {
      const a = await currentHuman(); if (controller.signal.aborted || version !== generation.current) return;
      if (a.principal.id !== identity) {
        if (!attempted) { setConfirmation(null); setReview(null); setIdentity(null); setSelection({}); }
        throw new Error("Human identity changed after review. No approval submitted as the new human; keep this request ID and return as the reviewing human or start a separate request deliberately.");
      }
      setAttempted(true);
      const receipt = await approveCompanyDevice(confirmation, controller.signal);
      if (controller.signal.aborted || version !== generation.current) return;
      if (receipt.status === "pending") { setMessage("Approval pending/unconfirmed. Keep this exact selection and request ID; retry only these same bytes. Do not start a replacement approval automatically."); return; }
      if (receipt.status === "denied" || receipt.status === "expired") throw new Error(`Company device request ${receipt.status}. No active approval is confirmed.`);
      if (receipt.status !== "approved" || receipt.device_id !== review.device_id || !receipt.principal_id || !UUID.test(receipt.principal_id) || !Array.isArray(receipt.projects)
        || receipt.projects.length !== confirmation.projects.length || new Set(receipt.projects.map(p => key(p.manifest))).size !== confirmation.projects.length
        || !confirmation.projects.every(s => {
          const p = receipt.projects!.find(p => p.manifest.area === s.area && p.manifest.owner_instance_id === s.owner_instance_id && p.manifest.project_id === s.project_id);
          const inspected = review.projects.find(p => key(p.project.manifest) === `${s.area}/${s.owner_instance_id}/${s.project_id}`);
          return p && inspected && p.role === "reader" && p.manifest.revision === s.manifest_revision && stable(p.manifest) === stable(inspected.project.manifest);
        })) throw new Error("Company approval receipt scope/revision/manifest mismatch. Keep the original confirmed selection; no success is established.");
      setApproved(true); setConfirmation(null);
    } catch (e) { if (!controller.signal.aborted && version === generation.current) setError(e instanceof Error ? e.message : "Company approval unconfirmed; keep the exact request."); }
    finally { if (!controller.signal.aborted && version === generation.current) { setBusy(false); setConfirmed(false); } }
  }
  return <div className="app-shell"><header className="topbar"><h1>Memory Layer company device approval</h1></header><main className="panel-stack">
    <section className="panel"><h2>Company project access</h2>
      <p>Select projects for offline knowledge and optionally permit this device to upload locally curated changes. Corrections remain subject to the owning project's review.</p>
      {auth && human(auth) ? <p>Approving as {auth.principal.display_name}.</p> : <><p>Company approval requires an OIDC human browser session, never local-owner or service credentials.</p><a href={authLoginUrl(`/?device_code=${CODE.test(code) ? encodeURIComponent(code) : ""}&company_device=1`)}>Sign in to review this device</a></>}
      {auth?.read_only ? <p>This service is read-only. Company approval is disabled.</p> : null}
      <label>User code<input autoComplete="off" maxLength={9} value={code} disabled={busy || Boolean(confirmation)} onChange={e => { cancelReview(); setCode(e.target.value.toUpperCase()); }} /></label>
      <button disabled={!allowed || !CODE.test(code) || busy || loading || Boolean(confirmation)} onClick={() => void load()}>Review company device request</button>
      {loading ? <button onClick={cancelReview}>Cancel review</button> : null}
    </section>
    {error ? <p className="panel" role="alert">{error}</p> : null}{message ? <p role="status">{message}</p> : null}
    {review && !approved ? <>
      <section className="panel"><h2>Device identity</h2><dl>
        <dt>Company UUID</dt><dd>{review.company_id}</dd><dt>Name</dt><dd>{review.name}</dd><dt>Platform</dt><dd>{review.platform}</dd>
        <dt>Device UUID</dt><dd>{review.device_id}</dd><dt>Request UUID</dt><dd>{review.request_id}</dd><dt>Credential SHA-256</dt><dd>{review.credential_sha256}</dd>
        <dt>Expires</dt><dd>{review.expires_at}</dd><dt>Catalog observed</dt><dd>{review.observed_at}</dd></dl>
        <p>Observed catalog metadata is not a promise of ongoing owner authorization. The server rechecks each selected owner before activation.</p>
        {expired ? <p role="alert">Request expired; no new activation permitted.</p> : null}
      </section>
      <section className="panel"><h2>Currently authorized company projects</h2>
        <p>Offline copies may contain sensitive knowledge, survive later revocation, and become stale. Select only the exact projects you permit this device to retain. Trust is remote-only and is never selectable for an offline copy.</p>
        {!review.projects.length ? <p>No authorized owner projects are available; absent projects cannot be granted here.</p> : null}
        {review.projects.map(p => { const m = p.project.manifest; const id = key(m); const selectable = m.area !== "trust" && m.offline_allowed && p.project.offline_available;
          return <article key={id}><label><input type="checkbox" aria-label={`Select ${m.area} project ${m.project_id} owner ${m.owner_instance_id}`} checked={Boolean(selection[id])} disabled={disabled || Boolean(confirmation) || !selectable} onChange={e => setSelection(current => ({ ...current, [id]: e.target.checked }))} />{m.project}</label>
            <p>{m.area} · project {m.project_id} · owner {m.owner_instance_id} · revision {m.revision} · reader only · manifest SHA-256 {p.manifest_sha256}</p>
            <p>Repository (text only): {m.repository_url}; branch {m.default_branch}; reviewed by {m.reviewed_by} at {m.reviewed_at}.</p>
            <p>{m.area === "trust" ? "Trust — remote-only; no offline copy" : selectable ? "Offline policy permits a retained read-only copy." : "Offline copy unavailable; cannot select."}</p>
            {selectable && p.curated_contributions_available ? <label><input type="checkbox" aria-label={`Allow curated uploads for ${m.project_id}`} checked={Boolean(contributions[id])} disabled={disabled || Boolean(confirmation) || !selection[id]} onChange={e => setContributions(current => ({ ...current, [id]: e.target.checked }))} />Allow this device to synchronize locally curated changes. Raw captures and sessions stay local.</label> : null}
            <h3>Reviewed pinned tools</h3>{m.tools.length ? <ul>{m.tools.map((t, i) => <li key={i}>{t.name} · {t.version} · {t.url} · SHA-256 {t.sha256}</li>)}</ul> : <p>No reviewed tools.</p>}
            <h3>Reviewed model destinations</h3>{m.destinations.length ? <ul>{m.destinations.map((d, i) => <li key={i}>{d.name} · provider {d.provider_id} · destination {d.destination_id}</li>)}</ul> : <p>No reviewed destinations.</p>}
          </article>;
        })}
        <button disabled={disabled || Boolean(confirmation) || !Object.values(selection).some(Boolean)} onClick={prepare}>Review selected company access</button>
      </section>
      {confirmation ? <section className="panel" aria-label="Exact company device approval"><h2>Confirm exact retained copy</h2>
        <p>Company {review.company_id}; device {review.device_id}; request {review.request_id}; credential SHA-256 {review.credential_sha256}; expires {review.expires_at}.</p>
        <pre>{JSON.stringify(confirmation, null, 2)}</pre>
        <p>Selected projects retain an offline copy. Projects marked contribution_confirmed also permit curated uploads under current project permissions. Revocation stops future synchronization.</p>
        <label><input type="checkbox" checked={confirmed} disabled={disabled} onChange={e => setConfirmed(e.target.checked)} />I verified the device fingerprint, exact owner/project revisions and digests, and permit retained offline copies.</label>
        <button disabled={disabled || !confirmed} onClick={() => void approve()}>{attempted ? "Retry exact company approval" : "Approve selected company access"}</button>
        <button disabled={busy || attempted} onClick={() => { setConfirmation(null); setConfirmed(false); }}>Change company selection</button>
        {attempted ? <p>Outcome may already be committed. The request and selected bytes remain frozen; no automatic refresh or replacement grant is attempted.</p> : null}
      </section> : null}
    </> : null}
    {approved ? <section role="status" className="panel"><h2>Company device approval recorded</h2><p>Return to the requesting device to continue. This receipt is not installation, current owner authority or completed synchronization.</p></section> : null}
    <a href="/">Return to Memory Layer</a>
  </main></div>;
}
