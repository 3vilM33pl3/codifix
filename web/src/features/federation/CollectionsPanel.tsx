import { useEffect, useRef, useState } from "react";
import { federationRequest } from "../../api";
import type { Discovery } from "./types";

type Advertisement = { collection: string; name: string; revision: number; active: boolean };
type Page = { collections: Advertisement[]; next_after: string | null };
type Detail = { advertisement: Advertisement; snapshot: unknown; reviewer: string; request_id: string };
type Draft = Detail & { review_digest: unknown };
type Pending = { path: string; body: unknown; label: string };
const split = (s: string) => s.split(/[\s,]+/).filter(Boolean);

export function CollectionsPanel({ project, discovery }: { project: string; discovery: Discovery }) {
  return <CollectionsWorkspace key={`${project}/${discovery.space}/${discovery.principal}/${discovery.read_only}/${discovery.permissions.join(",")}`} project={project} discovery={discovery} />;
}
function CollectionsWorkspace({ project, discovery }: { project: string; discovery: Discovery }) {
  const [page, setPage] = useState<Page | null>(null);
  const [detail, setDetail] = useState<Detail | null>(null);
  const [collection, setCollection] = useState("");
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [topics, setTopics] = useState("");
  const [requesters, setRequesters] = useState("");
  const [outputs, setOutputs] = useState("local_human");
  const [models, setModels] = useState("");
  const [memories, setMemories] = useState("");
  const [evidence, setEvidence] = useState("{}");
  const [servicePrincipal, setServicePrincipal] = useState("");
  const [grantState, setGrantState] = useState<{ principal: string; enabled: boolean | null } | null>(null);
  const [pending, setPending] = useState<Pending | null>(null);
  const [confirmed, setConfirmed] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [message, setMessage] = useState("");
  const lifetime = useRef<AbortController | null>(null);
  const running = useRef(false);
  useEffect(() => { const controller = new AbortController(); lifetime.current = controller; return () => controller.abort(); }, []);
  const disabled = discovery.read_only || !["memory_curate", "bundle_export"].every(p => discovery.permissions.includes(p));
  async function call<T>(path: string, body?: unknown) { return federationRequest<T>(project, discovery.space, path, body, lifetime.current?.signal); }
  async function run(work: () => Promise<void>) {
    if (running.current || disabled) return;
    running.current = true; setBusy(true); setError(""); setMessage("");
    try { await work(); } catch (e) { if (!lifetime.current?.signal.aborted) {
      setError(String(e));
      // Grant changes are current policy, not replayable publication intents.
      if (pending?.path.endsWith("/grants")) { setPending(null); setConfirmed(false); }
    } }
    finally { running.current = false; if (!lifetime.current?.signal.aborted) setBusy(false); }
  }
  async function browse(after?: string) {
    setPage(null); setDetail(null);
    const response = await call<Page>(`/collections?limit=32${after ? `&after=${encodeURIComponent(after)}` : ""}`);
    if (!lifetime.current?.signal.aborted) setPage(response);
  }
  return <section aria-label="Expertise collection management"><h3>Approved expertise collections</h3>
    <p>Experimental internal expertise exchange. Collections stay local; only reviewed advertisements are published. Activation grants standing permission to answer from the frozen collection, subject to current permissions. Model synthesis requires separate operator configuration and explicit consent.</p>
    <p>Stored active consent does not prove current source eligibility or gateway publication. Withdrawal cannot recall delivered or already authorized in-flight bytes.</p>
    {disabled ? <p>Collection controls require a writable service and curate/export permissions.</p> : null}
    {error ? <p role="alert">Collection operation failed or was denied: {error}. Publication intents may be retried exactly; uncertain grant changes require fresh policy inspection and review. Do not assume rollback.</p> : null}
    {message ? <p role="status">{message}</p> : null}
    <fieldset disabled={disabled || busy || pending !== null}><legend>Stored local reviews</legend>
      <button onClick={() => void run(() => browse())}>Browse collections</button>
      {page ? <>{!page.collections.length ? <p>No stored collections on this page.</p> : <ul>{page.collections.map(ad => <li key={ad.collection}>{ad.name}: {ad.active ? "stored active consent" : "withdrawn"}, revision {ad.revision} ({ad.collection}) <button onClick={() => void run(async () => {
        setDetail(null); setGrantState(null); const d = await call<Detail>(`/collections/${encodeURIComponent(ad.collection)}`);
        if (!lifetime.current?.signal.aborted) setDetail(d);
      })}>Inspect collection {ad.name}</button></li>)}</ul>}
      <button disabled={!page.next_after} onClick={() => void run(() => browse(page.next_after ?? undefined))}>Next collection page</button></> : null}
      {detail ? <><h4>Stored review — not current answer authorization</h4><pre className="federation-json">{JSON.stringify(detail, null, 2)}</pre>
        <button disabled={!detail.advertisement.active} onClick={() => { setConfirmed(false); setPending({ path: "/collections/withdraw", label: "Withdraw standing collection consent", body: { request_id: crypto.randomUUID(), collection: detail.advertisement.collection, expected_revision: detail.advertisement.revision, confirmed: true } }); }}>Review collection withdrawal</button>
        <fieldset disabled={!discovery.permissions.includes("auth_manage")}><legend>Answering service collection grant</legend>
          <p>This changes only local collection access. The target must be an operator-configured internal identity with current project permissions; worker configuration and local/remote AnswerExpertise grants remain separate. No credentials are created.</p>
          <label>Answering service principal UUID<input value={servicePrincipal} onChange={e => { setServicePrincipal(e.target.value); setGrantState(null); }} /></label>
          <button disabled={!servicePrincipal.trim()} onClick={() => void run(async () => {
            setGrantState(null);
            const value = await call<{ principal: string; enabled: boolean | null }>(`/collections/${encodeURIComponent(detail.advertisement.collection)}/grants?principal=${encodeURIComponent(servicePrincipal.trim())}`);
            if (!lifetime.current?.signal.aborted) setGrantState(value);
          })}>Inspect service grant</button>
          {grantState ? <p>Stored grant for {grantState.principal}: {grantState.enabled === null ? "not configured" : grantState.enabled ? "enabled" : "disabled"}. This is not current execution authorization.</p> : null}
          {[true, false].map(enabled => <button key={String(enabled)} disabled={!servicePrincipal.trim()} onClick={() => {
            setGrantState(null); setConfirmed(false); setPending({ path: `/collections/${encodeURIComponent(detail.advertisement.collection)}/grants`, label: enabled ? "Allow service collection access" : "Revoke service collection access", body: { principal: servicePrincipal.trim(), enabled, confirmed: true } });
          }}>{enabled ? "Review service grant" : "Review service grant revocation"}</button>)}
          <p>After an uncertain grant change, inspect current policy and prepare a new deliberate change; do not replay an old grant over newer policy.</p>
        </fieldset>
      </> : null}
    </fieldset>
    <fieldset disabled={disabled || busy || pending !== null}><legend>Prepare a new collection revision</legend>
      <label>Collection UUID (blank creates a new collection)<input value={collection} onChange={e => setCollection(e.target.value)} /></label>
      <label>Collection name<input value={name} onChange={e => setName(e.target.value)} /></label>
      <label>Reviewed collection description<textarea value={description} onChange={e => setDescription(e.target.value)} /></label>
      <label>Collection topics (comma separated)<input value={topics} onChange={e => setTopics(e.target.value)} /></label>
      <label>Allowed requesting instance UUIDs<textarea value={requesters} onChange={e => setRequesters(e.target.value)} /></label>
      <label>Reviewed output destination IDs (comma separated)<input value={outputs} onChange={e => setOutputs(e.target.value)} /></label>
      <label>Reviewed synthesis provider IDs (blank disables models)<input value={models} onChange={e => setModels(e.target.value)} /></label>
      <p>Non-local output permits disclosure to an assistant or model destination. The operator must separately allow each ID; typing a name does not authorize it.</p>
      <label>Selected canonical memory UUIDs<textarea value={memories} onChange={e => setMemories(e.target.value)} /></label>
      <details><summary>Advanced explicit evidence selection</summary><p>JSON object mapping a selected canonical UUID to its EvidenceSelection array (snapshot and snapshot_digest). Empty means no evidence is attached. No raw source, capture or path is selected automatically; server validation and the complete preview remain mandatory.</p>
        <label>Evidence selections JSON<textarea rows={5} value={evidence} onChange={e => setEvidence(e.target.value)} /></label></details>
      <p>Deterministic excerpts remain available. Adding a synthesis provider permits reviewed context disclosure to that operator-configured provider. Revisions replace the selected collection contents—omitted entries or evidence are not carried forward.</p>
      <button disabled={!name.trim() || !description.trim() || !split(requesters).length || !split(memories).length} onClick={() => void run(async () => {
        const ids = split(memories);
        const selections: unknown = JSON.parse(evidence);
        if (!selections || typeof selections !== "object" || Array.isArray(selections)) throw new Error("Evidence selections must be an object");
        if (Object.keys(selections).some(id => !ids.includes(id)) || Object.values(selections).some(value => !Array.isArray(value))) throw new Error("Evidence must be arrays for selected memory UUIDs only");
        const entries = ids.map(canonical => ({ canonical, evidence: (selections as Record<string, unknown[]>)[canonical] ?? [] }));
        const draft = await call<Draft>("/collections/draft", { collection: collection.trim() || crypto.randomUUID(), name, description, topics: topics.split(",").map(t => t.trim()).filter(Boolean), requesters: split(requesters), entries, modes: split(models).length ? ["deterministic", "model"] : ["deterministic"], model_destinations: split(models), output_destinations: outputs.split(",").map(t => t.trim()).filter(Boolean) });
        if (lifetime.current?.signal.aborted) return;
        setConfirmed(false); setPending({ path: "/collections/activate", label: "Activate reviewed collection", body: { draft, confirmed_digest: draft.review_digest } });
      })}>Prepare collection review</button>
    </fieldset>
    {pending ? <fieldset disabled={disabled || busy}><legend>Confirm exact collection action</legend><h4>{pending.label}</h4>
      <p>Project {project}; space {discovery.space}; local instance {discovery.instance}; reviewer {discovery.principal}. POST {pending.path}.</p>
      <pre className="federation-json" aria-label="Exact collection action bytes">{JSON.stringify(pending.body, null, 2)}</pre>
      <label><input type="checkbox" checked={confirmed} onChange={e => setConfirmed(e.target.checked)} />I reviewed every collection field, source version, evidence byte and disclosure permission.</label>
      <button disabled={!confirmed} onClick={() => void run(async () => {
        await call(pending.path, pending.body);
        if (lifetime.current?.signal.aborted) return;
        setPending(null); setConfirmed(false); setPage(null); setDetail(null);
        setMessage(pending.path.endsWith("/grants") ? "Service collection grant updated locally. Worker configuration, current permissions and space capabilities are still required." : "Collection action queued locally—not yet gateway accepted, published or replicated. Browse again to inspect local consent.");
      })}>Confirm collection action</button>
      <button onClick={() => { setPending(null); setConfirmed(false); }}>Close collection review</button>
      <p>{pending.path.endsWith("/grants") ? "This updates current policy, not a replayable publication. After uncertainty, recheck policy before a new change." : "After an uncertain response, retry this same confirmation."} Closing does not undo a committed action.</p>
    </fieldset> : null}
  </section>;
}
