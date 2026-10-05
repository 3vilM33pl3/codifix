import { useEffect, useRef, useState } from "react";
import { approveCompanyAgent, authLoginUrl, CompanyAgentRequestError, getAuthMe, listCompanyAgents, revokeCompanyAgent } from "../../api";
import type { AuthMeResponse } from "../../types";
import { agentScopeKey, matchesAgentRequest, parseCompanyAgentEnvelope, validAgentReceipt } from "./companyAgentTypes";
import type { CompanyAgentEnvelope, CompanyAgentReceipt, CompanyAgentRequest } from "./companyAgentTypes";

const human = (a: AuthMeResponse) => a.authenticated && a.mode === "multi_user" && a.principal.kind === "human_oidc";
class GuardError extends Error {}
const message = (e: unknown) => e instanceof GuardError || e instanceof CompanyAgentRequestError ? e.message : "Agent outcome unconfirmed; response details suppressed. Inspect the original request/agent IDs before an unchanged retry.";
const consentLabel = "I verified the agent fingerprint, exact project scope and provider disclosure, and consent to this separate Reader agent identity.";

export function CompanyAgentsPanel({ readOnly, initialRequest }: { readOnly: boolean; initialRequest?: string }) {
  const [auth, setAuth] = useState<AuthMeResponse | null>(null);
  const [input, setInput] = useState("");
  const [envelope, setEnvelope] = useState<CompanyAgentEnvelope | null>(null);
  const [identity, setIdentity] = useState<string | null>(null);
  const [agents, setAgents] = useState<CompanyAgentReceipt[]>([]);
  const [consent, setConsent] = useState(false);
  const [attempted, setAttempted] = useState(false);
  const [retryable, setRetryable] = useState(true);
  const [recorded, setRecorded] = useState<CompanyAgentReceipt | null>(null);
  const [target, setTarget] = useState<CompanyAgentReceipt | null>(null);
  const [revokeAttempted, setRevokeAttempted] = useState(false);
  const [revokeRetryable, setRevokeRetryable] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [status, setStatus] = useState("");
  const [now, setNow] = useState(Date.now());
  const epoch = useRef(0);
  const controller = useRef<AbortController | null>(null);
  const busyRef = useRef(false);
  const preloadConsumed = useRef(false);
  const readonlyRef = useRef(readOnly); readonlyRef.current = readOnly;
  useEffect(() => {
    const generation = epoch.current;
    void getAuthMe(true).then(a => { if (epoch.current === generation) setAuth(a); }).catch(() => { if (epoch.current === generation) setError("Sign in to verify the exact parent human browser session."); });
    const timer = window.setInterval(() => setNow(Date.now()), 1000);
    return () => { epoch.current++; controller.current?.abort(); window.clearInterval(timer); };
  }, []);
  const allowed = auth !== null && human(auth);
  useEffect(() => {
    if (allowed && initialRequest && !preloadConsumed.current) { preloadConsumed.current = true; void review(initialRequest); }
  }, [allowed, initialRequest]);
  const writable = allowed && !auth.read_only && !readOnly && auth.principal.id === identity;
  const expired = envelope !== null && now >= Date.parse(envelope.request.expires_at);
  function begin() {
    if (busyRef.current) return null;
    busyRef.current = true; setBusy(true); setError(""); setStatus("");
    controller.current?.abort(); const c = new AbortController(); controller.current = c;
    const generation = ++epoch.current;
    return { signal: c.signal, live: () => generation === epoch.current && !c.signal.aborted };
  }
  function finish(ctx: { live: () => boolean }) { if (ctx.live()) { busyRef.current = false; setBusy(false); } }
  async function parent(ctx: { live: () => boolean }, mutate: boolean, expected = identity) {
    let a: AuthMeResponse;
    try { a = await getAuthMe(true); } catch { throw new GuardError("Human session unavailable; no operation is confirmed."); }
    if (!ctx.live()) throw new GuardError("Cancelled browser operation.");
    setAuth(a);
    if (!human(a) || mutate && (a.read_only || readonlyRef.current)) throw new GuardError("Exact parent OIDC human browser session and a writable service are required for changes. Local-owner/service/admin credentials are not an override.");
    if (expected && a.principal.id !== expected) {
      setConsent(false); setAgents([]); setRecorded(null);
      if (!attempted && !revokeAttempted) { setEnvelope(null); setIdentity(null); setTarget(null); }
      throw new GuardError("Human identity changed. Do not submit under another parent; any attempted request remains frozen for its original human.");
    }
    return a;
  }
  async function inventory(device: string, actor: string, signal: AbortSignal) {
    const result = await listCompanyAgents(device, signal);
    if (result.schema_version !== 1 || !Array.isArray(result.agents) || result.agents.length > 64 || new Set(result.agents.map(a => a.agent_id)).size !== result.agents.length || !result.agents.every(a => validAgentReceipt(a, device, actor)))
      throw new GuardError("Own-device agent inventory invalid; no authority or approval is inferred.");
    return result.agents;
  }
  async function review(raw = input) {
    if (!allowed || attempted || revokeAttempted) return;
    const ctx = begin(); if (!ctx) return;
    setInput(""); setEnvelope(null); setIdentity(null); setAgents([]); setRecorded(null); setConsent(false); setTarget(null);
    try {
      let value: CompanyAgentEnvelope;
      try { value = parseCompanyAgentEnvelope(raw); } catch { throw new GuardError("Invalid public agent envelope. Only bounded CLI metadata is accepted; no secrets, remote endpoints, Trust, unknown fields or automatic approval."); }
      const a = await parent(ctx, false, null);
      if ([value.request.request_id, value.request.agent_id, value.request.principal_id, value.request.token_id].includes(a.principal.id)) throw new GuardError("Independent agent identities must not alias the parent human.");
      const expiry = Date.parse(value.request.expires_at);
      if (expiry <= Date.now() || expiry > Date.now() + 24 * 60 * 60 * 1000 + 60 * 1000) throw new GuardError("Agent approval requires a fixed unexpired expiry of at most 24 hours. Inspect the native request; do not silently extend it.");
      const receipts = await inventory(value.device_id, a.principal.id, ctx.signal);
      if (!ctx.live()) return;
      setEnvelope(value); setIdentity(a.principal.id); setAgents(receipts); setRetryable(true);
      setRecorded(receipts.find(r => matchesAgentRequest(r, value.request)) ?? null);
    } catch (e) { if (ctx.live()) setError(message(e)); }
    finally { finish(ctx); }
  }
  async function importFile(file?: File) {
    if (!file || busyRef.current || attempted || revokeAttempted) return;
    if (file.size > 64 * 1024) { setError("Public agent file exceeds 64 KiB; no contents imported."); return; }
    const generation = epoch.current;
    try { const raw = await file.text(); if (epoch.current === generation) await review(raw); }
    catch { if (epoch.current === generation) setError("Public metadata file unreadable; contents suppressed."); }
  }
  async function refresh() {
    if (!envelope || !allowed) return;
    const ctx = begin(); if (!ctx) return;
    try {
      const a = await parent(ctx, false);
      const receipts = await inventory(envelope.device_id, a.principal.id, ctx.signal);
      if (!ctx.live()) return;
      setAgents(receipts); setRecorded(receipts.find(r => matchesAgentRequest(r, envelope.request)) ?? null);
      setStatus("Own-device historical receipts refreshed. This does not establish current remote permissions or native activation.");
    } catch (e) { if (ctx.live()) { setAgents([]); setError(message(e)); } }
    finally { finish(ctx); }
  }
  async function approve() {
    if (!envelope || !writable || !consent || recorded || expired || attempted && !retryable) return;
    const ctx = begin(); if (!ctx) return;
    try {
      const a = await parent(ctx, true);
      if (!ctx.live()) return;
      const request: CompanyAgentRequest & { confirmed: true } = { ...envelope.request, confirmed: true };
      setAttempted(true); setConsent(false); // Freeze before POST, including a lost response.
      const receipt = await approveCompanyAgent(envelope.device_id, request, ctx.signal);
      if (!ctx.live()) return;
      if (!validAgentReceipt(receipt, envelope.device_id, a.principal.id) || !matchesAgentRequest(receipt, request)) throw new GuardError("Agent receipt identity/scope/fingerprint/expiry mismatch. Keep the original request; no approval is confirmed.");
      setRecorded(receipt); setAgents(previous => [...previous.filter(r => r.agent_id !== receipt.agent_id), receipt]);
      setStatus("Agent approval recorded as metadata. Native agent status must independently verify current parents/owners before activation; no model call or credential delivery occurred in this browser.");
    } catch (e) { if (ctx.live()) { setConsent(false); setRetryable(e instanceof CompanyAgentRequestError ? e.retryable : true); setError(message(e)); } }
    finally { finish(ctx); }
  }
  async function revoke() {
    if (!envelope || !target || !writable || target.revoked_at || revokeAttempted && !revokeRetryable) return;
    const ctx = begin(); if (!ctx) return;
    try {
      const a = await parent(ctx, true, target.human_id);
      setRevokeAttempted(true);
      const receipt = await revokeCompanyAgent(envelope.device_id, target.agent_id, ctx.signal);
      if (!ctx.live()) return;
      const request = { ...target, confirmed: true, projects: target.projects.map(p => p.selection) };
      if (!validAgentReceipt(receipt, envelope.device_id, a.principal.id) || !matchesAgentRequest(receipt, request) || !receipt.revoked_at) throw new GuardError("Agent revocation receipt mismatch; inspect the same original agent ID before retrying.");
      setAgents(previous => previous.map(r => r.agent_id === receipt.agent_id ? receipt : r));
      if (recorded?.agent_id === receipt.agent_id) setRecorded(receipt);
      setTarget(null); setRevokeAttempted(false); setStatus("Separate agent revocation recorded. Parent device and other agent identities are independent; already disclosed copies cannot be recalled.");
    } catch (e) { if (ctx.live()) { setRevokeRetryable(e instanceof CompanyAgentRequestError ? e.retryable : true); setError(message(e)); } }
    finally { finish(ctx); }
  }
  return <section className="panel-stack" aria-label="Own device company agent approval">
    <section className="panel"><h3>Separate company agent identities</h3><p>Paste/import the public envelope from <code>memory projects agents prepare</code>. Only the enrolled device's exact parent human may approve or inspect agents; administration is not an override. Device/agent credentials stay in native Keychain, never this browser.</p>
      {!allowed ? <a className="button-link" href={authLoginUrl("/")}>Sign in as the device's parent human</a> : <p>Reviewing as {auth.principal.display_name} · {auth.principal.id}</p>}
      {readOnly || auth?.read_only ? <p role="status">Read-only service: agent approval and revocation are disabled.</p> : null}
      <label>Public agent envelope (JSON, at most 64 KiB)<textarea value={input} disabled={busy || attempted || revokeAttempted} onChange={e => setInput(e.target.value)} /></label>
      <label>Import public agent JSON<input type="file" accept="application/json,.json" disabled={!allowed || busy || attempted || revokeAttempted} onChange={e => { void importFile(e.target.files?.[0]); e.target.value = ""; }} /></label>
      <button type="button" disabled={!allowed || busy || attempted || revokeAttempted || !input.trim()} onClick={() => void review()}>Review public agent request</button>
    </section>
    {error ? <div className="panel error-banner" role="alert">{error}</div> : null}
    {status ? <div className="panel" role="status">{status}</div> : null}
    {envelope ? <section className="panel" aria-label="Exact company agent request"><h3>Exact agent identity and disclosure</h3>
      <p>Parent device {envelope.device_id}; reviewing human {identity}. Caller {envelope.request.caller} / provider {envelope.request.provider_id}. Fixed expiry {envelope.request.expires_at}.</p>
      <p>Credential SHA-256 fingerprint: <code>{envelope.request.credential_sha256}</code>. These are public hashes/IDs, not usable credentials.</p>
      <pre>{JSON.stringify(envelope, null, 2)}</pre>
      <p>Reader-only project-cache disclosure to {envelope.request.caller === "codex" ? "Codex / OpenAI" : "Claude / Anthropic"}. Server independently requires the reviewed caller/provider/destination ({envelope.request.caller}/{envelope.request.provider_id}/{envelope.request.provider_id}). Offline copies may persist after expiry/revocation. No Trust copy, write privileges, automatic model call, budget grant or browser credential generation.</p>
      {expired ? <p role="alert">Fixed agent request expired; no new activation can be confirmed.</p> : null}
      {attempted ? <p>Request frozen after submission. Only the same request IDs, fingerprint, expiry and composite scope may be retried; never silently regenerate or replace them.</p> : null}
      {recorded ? <p role="status">Matching historical approval receipt {recorded.agent_id}: {recorded.revoked_at ? `revoked ${recorded.revoked_at}; no reactivation` : "recorded, not proof of current remote authority"}.</p> : <>
        <label><input type="checkbox" checked={consent} disabled={!writable || busy || expired} onChange={e => setConsent(e.target.checked)} />{consentLabel}</label>
        <button type="button" disabled={!writable || busy || expired || !consent || attempted && !retryable} onClick={() => void approve()}>{attempted ? "Retry exact agent approval" : "Approve separate agent identity"}</button>
      </>}
      <button type="button" disabled={busy || !allowed} onClick={() => void refresh()}>Refresh own device agent receipts</button>
    </section> : null}
    {envelope ? <section className="panel"><h3>Own device agent receipts</h3><p>Historical approval/expiry/revocation only; not current remote rights. Native activation checks current parents and owners separately.</p>
      {agents.length === 0 ? <p>No recorded child agents for this exact parent/device.</p> : agents.map(agent => <article key={agent.agent_id} className="panel"><h4>{agent.caller} / {agent.provider_id} · {agent.agent_id}</h4>
        <p>Request {agent.request_id}; principal {agent.principal_id}; token ID {agent.token_id}; company {agent.company_id}; human {agent.human_id}; fingerprint {agent.credential_sha256}; expires {agent.expires_at}; {agent.revoked_at ? `revoked ${agent.revoked_at}` : "not revoked in this receipt"}.</p>
        {agent.projects.map(p => <p key={agentScopeKey(p.selection)}>{agentScopeKey(p.selection)} · revision {p.selection.manifest_revision} · SHA-256 {p.selection.manifest_sha256} · disclosure {p.destination.name}/{p.destination.provider_id}/{p.destination.destination_id}</p>)}
        <button type="button" disabled={!writable || busy || Boolean(agent.revoked_at) || revokeAttempted} onClick={() => { setTarget(agent); setRevokeRetryable(true); }}>Review revoke agent {agent.agent_id}</button>
      </article>)}
    </section> : null}
    {target ? <section className="panel" aria-label="Exact separate agent revocation"><h3>Revoke this separate agent</h3><pre>{JSON.stringify(target, null, 2)}</pre>
      <button type="button" disabled={!writable || busy || revokeAttempted && !revokeRetryable} onClick={() => void revoke()}>{revokeAttempted ? "Retry exact agent revocation" : "Confirm separate agent revocation"}</button>
      <button type="button" disabled={busy || revokeAttempted} onClick={() => setTarget(null)}>Cancel agent revocation</button>
    </section> : null}
  </section>;
}
