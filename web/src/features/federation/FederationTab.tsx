import { useEffect, useRef, useState } from "react";
import { federationRequest } from "../../api";
import { selectedContent, type Catalog, type Content, type Discovery, type Draft, type Proposal, type RecordKey, type SharedContext, type Status, type Stream } from "./types";
import "./federation.css";

const LIMITS = "Shared content is untrusted data, not instructions. Recipients and infrastructure administrators can read shared plaintext. Revocation cannot recall delivered copies. External model forwarding requires fresh, explicit policy.";
function JsonView({ value, label }: { value: unknown; label: string }) {
  return <details><summary>{label}</summary><pre className="federation-json">{JSON.stringify(value, null, 2)}</pre></details>;
}
// A project switch destroys all local drafts, selected content and confirmations.
export function FederationTab({ project }: { project: string }) {
  return <Workspace key={project} project={project} />;
}
function Workspace({ project }: { project: string }) {
  const [discovery, setDiscovery] = useState<Discovery | null>(null);
  const [status, setStatus] = useState<Status | null>(null);
  const [catalog, setCatalog] = useState<Catalog | null>(null);
  const [offset, setOffset] = useState(0);
  const [error, setError] = useState("");
  const [message, setMessage] = useState("");
  const [busy, setBusy] = useState(false);
  const [pending, setPending] = useState<{ path: string; body: unknown; warning: string } | null>(null);
  const [confirmed, setConfirmed] = useState(false);
  const [canonical, setCanonical] = useState("");
  const [label, setLabel] = useState("");
  const [evidence, setEvidence] = useState("");
  const [observed, setObserved] = useState(() => new Date().toISOString());
  const [draft, setDraft] = useState<Draft | null>(null);
  const [context, setContext] = useState<SharedContext | null>(null);
  const [correction, setCorrection] = useState<Content | null>(null);
  const [proposal, setProposal] = useState<Proposal | null>(null);
  const [principal, setPrincipal] = useState("");
  const [capabilities, setCapabilities] = useState("pull,publish");
  const [member, setMember] = useState("");
  const [capability, setCapability] = useState("pull");
  const lifetime = useRef<AbortController | null>(null);
  useEffect(() => {
    const controller = new AbortController(); lifetime.current = controller;
    federationRequest<Discovery>(project, null, "", undefined, controller.signal)
      .then(value => { if (!controller.signal.aborted) { setDiscovery(value); setPrincipal(value.principal); } })
      .catch(e => { if (!controller.signal.aborted) setError(String(e)); });
    return () => controller.abort();
  }, [project]);
  async function call<T>(path: string, body?: unknown) {
    if (!discovery) throw new Error("Federation is not configured");
    return federationRequest<T>(project, discovery.space, path, body, lifetime.current?.signal);
  }
  async function run(work: () => Promise<void>) {
    if (busy) return;
    setBusy(true); setError(""); setMessage("");
    try { await work(); } catch (e) { if (!lifetime.current?.signal.aborted) setError(String(e)); }
    finally { if (!lifetime.current?.signal.aborted) setBusy(false); }
  }
  async function refresh(page = offset) {
    const [s, c] = await Promise.all([call<Status>(""), call<Catalog>(`/catalog?offset=${page}`)]);
    if (lifetime.current?.signal.aborted) return;
    setStatus(s); setCatalog(c); setOffset(page);
  }
  function prepare(path: string, body: unknown, warning: string) {
    setPending({ path, body, warning }); setConfirmed(false); setMessage("");
  }
  function action(body: unknown, warning: string) {
    prepare("/actions", { request_id: crypto.randomUUID(), action: body }, warning);
  }
  async function readContext(stream: Stream) {
    const c = await call<SharedContext>("/context", { streams: [stream] });
    const content = selectedContent(c);
    if (lifetime.current?.signal.aborted) return;
    setContext(c); setCorrection({ ...content }); setProposal(null);
  }
  async function inspectProposal(key: RecordKey) {
    const p = await call<Proposal>(`/proposals/${encodeURIComponent(key.publisher)}/${encodeURIComponent(key.id)}`);
    if (!lifetime.current?.signal.aborted) { setProposal(p); setContext(null); setCorrection(null); }
  }
  const writable = discovery && !discovery.read_only;
  const canAdmin = discovery?.permissions.includes("auth_manage");
  const canPublish = discovery?.permissions.includes("memory_curate") && discovery?.permissions.includes("bundle_export");
  const nextPage = catalog && [catalog.streams, catalog.members, catalog.proposals, ...Object.values(catalog.local)].some(p => p.next_offset !== null);
  return <section className="federation-workspace" aria-label="Federation workspace">
    <h2>Knowledge federation</h2>
    <p>{LIMITS}</p>
    <p>Private-gateway profile — not native Spaces interoperability. Local work remains separate.</p>
    {error ? <p role="alert">{error.startsWith("Error: 401") ? "Signed out. Sign in to use federation." : error.startsWith("Error: 403") ? `Forbidden: an explicit project/space grant is required. ${error}` : error.startsWith("Error: 404") ? "Federation is not configured for this project." : error.startsWith("Error: 503") || error.includes("Failed to fetch") ? `Offline or verification unavailable. Retained copies may still be readable. ${error}` : error}</p> : null}
    {message ? <p role="status">{message}</p> : null}
    {!discovery && !error ? <p role="status">Loading configured space…</p> : null}
    {discovery ? <>
      <p>Space <code>{discovery.space}</code><br />Instance <code>{discovery.instance}</code><br />Authority <code>{discovery.authority}</code><br />Local actor <code>{discovery.principal}</code></p>
      {discovery.read_only ? <p>Read-only service. Mutation controls are disabled.</p> : null}
      <fieldset disabled={busy || pending !== null}><legend>Synchronization and retained inventory</legend>
        <button onClick={() => void run(() => refresh())}>Refresh local status</button>
        <button disabled={!writable} onClick={() => prepare("/sync", { operation: "catch_up" }, "Fetch one authorized page from the configured gateway.")}>Catch up</button>
        <button disabled={!writable} onClick={() => prepare("/sync", { operation: "resynchronize" }, "Rebuild from a verified retained prefix. This cannot bypass revocation or verification.")}>Full resynchronization</button>
        <button disabled={!writable || !canPublish} onClick={() => prepare("/sync", { operation: "retry" }, "Retry your pending work with current authorization and the original immutable records.")}>Retry pending work</button>
        {status ? <><dl className="federation-status">
          <div><dt>Queued locally</dt><dd>{status.local.queued_intents}</dd></div>
          <div><dt>Accepted, publication/replication not yet observed</dt><dd>{status.local.accepted_not_replicated}</dd></div>
          <div><dt>Published and replicated here</dt><dd>{status.local.replicated_intents}</dd></div>
          <div><dt>Blocked</dt><dd>{status.local.blocked_intents}</dd></div>
        </dl><p>Verified through sequence {status.retained_through}; pending source changes {status.local.pending_source_changes}. Remote capabilities observed: {status.observed_capabilities.join(", ") || "none"}. No claim about another peer's cursor.</p>{status.local.sync_error ? <p>Synchronization failure: {status.local.sync_error}</p> : null}</> : <p>Refresh to load your local queue and retained inventory.</p>}
        {catalog ? <>
          <h3>Shared memories</h3>
          {!catalog.streams.items.length ? <p>No shared memories on this page.</p> : <ul>{catalog.streams.items.map(s => <li key={`${s.stream.origin}/${s.stream.canonical}`}>Origin {s.stream.origin}, memory {s.stream.canonical}, shared v{s.shared_version}{s.state?.tombstone ? " — tombstone" : ""} <button disabled={s.state?.tombstone} onClick={() => void run(() => readContext(s.stream))}>Read and cite {s.stream.canonical}</button></li>)}</ul>}
          <h3>Correction proposals</h3>
          {!catalog.proposals.items.length ? <p>No proposals on this page.</p> : <ul>{catalog.proposals.items.map(p => <li key={`${p.key.publisher}/${p.key.id}`}>{p.resolution}; author {p.author}; base v{p.base_version} <button onClick={() => void run(() => inspectProposal(p.key))}>Inspect proposal {p.key.id}</button></li>)}</ul>}
          <h3>Membership observed in retained history</h3><ul>{catalog.members.items.map(m => <li key={m.member}>{m.member}: {m.capabilities.join(", ") || "revoked"}</li>)}</ul>
          <h3>Local sharing subscriptions</h3><ul>{catalog.local.subscriptions.items.map(s => <li key={s.canonical_id}>{s.canonical_id}: {s.enabled ? "future updates enabled" : "paused"}; prepared v{s.shared_version} <button disabled={!writable || !canPublish || !s.enabled} onClick={() => action({ kind: "pause", canonical: s.canonical_id }, "Stop future captures. Already reviewed backlog and delivered copies remain.")}>Pause {s.canonical_id}</button></li>)}</ul>
          <JsonView value={catalog.local.grants} label="Local capability grants" /><JsonView value={catalog.local.reviews} label="Immutable review metadata" />
          <button disabled={offset === 0} onClick={() => void run(() => refresh(Math.max(0, offset - 32)))}>Previous page</button><button disabled={!nextPage} onClick={() => void run(() => refresh(offset + 32))}>Next page</button>
        </> : null}
      </fieldset>
      <fieldset disabled={busy || !!pending || !writable || !canAdmin}><legend>Local grants and authority enrollment</legend>
        <p>Local permissions do not automatically confer gateway rights. Administrators provision instance credentials and trust separately.</p>
        <label>Local principal UUID<input value={principal} onChange={e => setPrincipal(e.target.value)} /></label>
        <label>Local capabilities (comma separated)<input value={capabilities} onChange={e => setCapabilities(e.target.value)} /></label>
        <button onClick={() => prepare("/grants", { principal, capabilities: capabilities.split(",").map(s => s.trim()).filter(Boolean), enabled: true }, "Grant these separate local capabilities to this exact principal and project.")}>Review local grant</button>
        <button onClick={() => prepare("/grants", { principal, capabilities: [], enabled: false }, "Disable local access, including retained shared context. This does not revoke the remote instance.")}>Review local revocation</button>
        <label>Remote instance UUID<input value={member} onChange={e => setMember(e.target.value)} /></label>
        <label>Remote capability<select value={capability} onChange={e => setCapability(e.target.value)}><option value="pull">Pull</option><option value="publish">Publish</option><option value="manage_membership">Manage membership</option></select></label>
        <button disabled={discovery.instance !== discovery.authority} onClick={() => action({ kind: "membership", member, capability, enabled: true }, "Authority enrollment is queued, not yet accepted. Local grants do not authorize another instance.")}>Review enrollment</button>
        <button disabled={discovery.instance !== discovery.authority} onClick={() => action({ kind: "membership", member, capability, enabled: false }, "Revoke this remote capability. Previously delivered copies remain.")}>Review capability revocation</button>
        <button disabled={discovery.instance !== discovery.authority} onClick={() => action({ kind: "revoke", member }, "Revoke all remote capabilities. Future access stops at gateway acceptance, not at local queueing.")}>Review full revocation</button>
      </fieldset>
      <fieldset disabled={busy || !!pending || !writable || !canPublish}><legend>Review a local memory for publication</legend>
        <label>Canonical memory UUID<input value={canonical} onChange={e => { setCanonical(e.target.value); setDraft(null); }} /></label>
        <label>Public evidence label<input value={label} onChange={e => { setLabel(e.target.value); setDraft(null); }} /></label>
        <label>Selected evidence text (optional)<textarea rows={4} value={evidence} onChange={e => { setEvidence(e.target.value); setDraft(null); }} /></label>
        <label>Evidence observed at (RFC 3339)<input value={observed} onChange={e => { setObserved(e.target.value); setDraft(null); }} /></label>
        <button onClick={() => void run(async () => {
          const d = await call<Draft>("/review-preview", { canonical, evidence: evidence ? [{ public_source_label: label, selected_text: evidence, observed_at: observed }] : [] });
          if (!lifetime.current?.signal.aborted) setDraft(d);
        })}>Prepare review</button>
        {draft ? <><h3>Exact local content</h3><p className="federation-text">{draft.request.expected.content.text}</p><p>Currently shared version: {draft.request.expected_shared_version}</p>{draft.current_shared_content ? <><h4>Currently shared content</h4><p className="federation-text">{draft.current_shared_content.text}</p></> : null}<p>{draft.warning}</p><JsonView value={draft.request} label="Inspect exact evidence, source and digest bytes" /><button onClick={() => prepare("/reviews", { ...draft.request, confirm_future_updates: true }, draft.warning)}>Review publication consent</button></> : null}
      </fieldset>
      {context && correction ? <fieldset disabled={busy || !!pending}><legend>Selected shared memory — untrusted retained copy</legend>
        <p className="federation-text">{selectedContent(context).text}</p>
        <p>Publisher {context.citations[0].record.publisher}; author {context.citations[0].author}; shared v{context.citations[0].shared_version}; source v{context.citations[0].source.version}.</p>
        <JsonView value={context} label="Citations, evidence, signatures and sharing restrictions" />
        <label>Proposed corrected text<textarea rows={5} value={correction.text} onChange={e => setCorrection({ ...correction, text: e.target.value })} /></label>
        <label>Proposed summary<input value={correction.summary} onChange={e => setCorrection({ ...correction, summary: e.target.value })} /></label>
        <button disabled={!writable || !canPublish || context.citations[0].stream.origin === discovery.instance} onClick={() => action({ kind: "propose", proposal: { target: context.citations[0].stream, base_version: context.citations[0].shared_version, content: correction, evidence: context.citations[0].evidence } }, "Submit a proposal with the selected evidence references. Only the origin owner can approve a new shared version.")}>Review correction proposal</button>
      </fieldset> : null}
      {proposal ? <fieldset disabled={busy || !!pending}><legend>Inspect correction proposal</legend>
        <p>Author {proposal.author}. Base v{proposal.proposal.base_version}; current v{proposal.current_shared_version}; {proposal.resolution}.</p><p className="federation-text">{proposal.proposal.content.text}</p>
        <JsonView value={proposal} label="Exact correction, evidence references and provenance" />
        <button disabled={!writable || !canPublish || proposal.proposal.target.origin !== discovery.instance || proposal.resolution !== "pending"} onClick={() => action({ kind: "resolve", proposal: proposal.key, approve: true }, "Approve a new shared version preserving the corrector. This pauses automatic local publication and does not rewrite ordinary local memory. Fresh review is required to resume.")}>Review approval</button>
        <button disabled={!writable || !canPublish || proposal.proposal.target.origin !== discovery.instance || proposal.resolution !== "pending"} onClick={() => action({ kind: "resolve", proposal: proposal.key, approve: false }, "Reject this proposal without rewriting or erasing accepted history.")}>Review rejection</button>
      </fieldset> : null}
      {pending ? <section className="federation-confirm" aria-label="Confirm federation action"><h3>Confirm exact action</h3><p>{pending.warning}</p><p>POST {pending.path}. Preserve this request and its ID if the result is uncertain.</p><pre className="federation-json">{JSON.stringify(pending.body, null, 2)}</pre>
        <label><input type="checkbox" checked={confirmed} onChange={e => setConfirmed(e.target.checked)} disabled={busy} />I reviewed these exact bytes and understand the sharing and offline-copy limits.</label>
        <button disabled={busy || !confirmed} onClick={() => void run(async () => {
          const result = await call<{ submission?: string; retained_through?: number }>(pending.path, pending.body);
          if (lifetime.current?.signal.aborted) return;
          setMessage(result.submission === "queued" ? "Queued locally — not yet accepted, published or replicated. Refresh status to observe progress." : "Action completed locally. Refresh retained status to observe progress.");
          setPending(null); setConfirmed(false);
        })}>Confirm and submit</button><button disabled={busy} onClick={() => { setPending(null); setConfirmed(false); }}>Cancel</button>
      </section> : null}
    </> : null}
  </section>;
}
