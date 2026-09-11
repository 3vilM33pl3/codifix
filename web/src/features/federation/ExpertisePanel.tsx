import { useEffect, useRef, useState } from "react";
import { federationRequest } from "../../api";
import type { Discovery, RecordKey } from "./types";

type Advertisement = { owner: string; collection: string; revision: number; name: string; description: string; topics: string[]; model_destinations?: string[] };
type Catalog = { capabilities: Advertisement[]; next_after: RecordKey | null };
type Review = { digest: unknown; question: { request_id: string; question: string; capability: RecordKey; revision: number; requester: string; principal: string; mode: string; model_destination: string | null; output_destination: string; expires_at: string } };
type Result = { status: string; binding: unknown; payload_until: string; answer: { text: string; citations: unknown[]; insufficient_evidence: boolean; untrusted: boolean } | null };

// Remount on identity changes. No questions or answers go to browser storage.
export function ExpertisePanel({ project, discovery }: { project: string; discovery: Discovery }) {
  return <ExpertiseWorkspace key={`${project}/${discovery.space}/${discovery.principal}/${discovery.read_only}/${discovery.permissions.join(",")}`} project={project} discovery={discovery} />;
}
function ExpertiseWorkspace({ project, discovery }: { project: string; discovery: Discovery }) {
  const [catalog, setCatalog] = useState<Catalog | null>(null);
  const [selected, setSelected] = useState<Advertisement | null>(null);
  const [question, setQuestion] = useState("");
  const [model, setModel] = useState("");
  const [review, setReview] = useState<Review | null>(null);
  const [confirmed, setConfirmed] = useState(false);
  const [requestId, setRequestId] = useState("");
  const [result, setResult] = useState<Result | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [attempted, setAttempted] = useState(false);
  const lifetime = useRef<AbortController | null>(null);
  const running = useRef(false);
  useEffect(() => {
    const controller = new AbortController(); lifetime.current = controller;
    return () => controller.abort();
  }, []);
  const disabled = discovery.read_only || !discovery.permissions.includes("bundle_export");
  async function run(work: () => Promise<void>) {
    if (running.current || disabled) return;
    running.current = true; setBusy(true); setError("");
    try { await work(); } catch (e) { if (!lifetime.current?.signal.aborted) setError(String(e)); }
    finally { running.current = false; if (!lifetime.current?.signal.aborted) setBusy(false); }
  }
  async function call<T>(path: string, body?: unknown) {
    return federationRequest<T>(project, discovery.space, path, body, lifetime.current?.signal);
  }
  async function browse(after?: RecordKey) {
    setCatalog(null); setSelected(null); setResult(null);
    const suffix = after ? `&after_publisher=${encodeURIComponent(after.publisher)}&after_record=${encodeURIComponent(after.id)}` : "";
    const page = await call<Catalog>(`/capabilities?limit=32${suffix}`);
    if (!lifetime.current?.signal.aborted) setCatalog(page);
  }
  return <section aria-label="Private expertise exchange">
    <h3>Ask another instance</h3>
    <p>Experimental internal expertise exchange; the operator must enable browser expertise and grant access. Ordinary queries stay local. Only this question is sent—no conversation history, files or local search results.</p>
    <p>Default: deterministic excerpts. Output destination: local human browser. Optional synthesis discloses reviewed context to the selected, separately authorized provider. Revocation cannot recall delivered bytes.</p>
    {disabled ? <p>Expertise is unavailable in read-only mode or without export permission.</p> : null}
    {error ? <p role="alert">Expertise unavailable or denied: {error}. An uncertain submission may already be queued; preserve its request ID and retry only the exact review.</p> : null}
    <fieldset disabled={disabled || busy || review !== null}><legend>Discover and prepare an explicit question</legend>
      <button onClick={() => void run(() => browse())}>Discover expertise</button>
      {catalog ? <>
        {!catalog.capabilities.length ? <p>No permitted expertise on this page.</p> : <ul>{catalog.capabilities.map(ad => <li key={`${ad.owner}/${ad.collection}`}>
          <strong>{ad.name}</strong> — {ad.description}<br />Answering instance {ad.owner}; collection {ad.collection}; revision {ad.revision}.
          <button onClick={() => { setSelected(ad); setModel(""); setResult(null); }}>Select expertise {ad.name}</button>
        </li>)}</ul>}
        <button disabled={!catalog.next_after} onClick={() => void run(() => browse(catalog.next_after ?? undefined))}>Next expertise page</button>
      </> : null}
      {selected ? <p>Selected destination: {selected.owner}; {selected.name}, revision {selected.revision}.</p> : null}
      <label>Remote question<textarea value={question} onChange={e => setQuestion(e.target.value)} rows={4} /></label>
      <label>Expertise answer mode<select value={model} onChange={e => setModel(e.target.value)}><option value="">Deterministic excerpts (no synthesis)</option>{selected?.model_destinations?.map(id => <option key={id} value={id}>Synthesis via {id}</option>)}</select></label>
      <p>{new TextEncoder().encode(question).length} / 4096 UTF-8 bytes</p>
      <button disabled={!selected || !question.trim() || new TextEncoder().encode(question).length > 4096} onClick={() => void run(async () => {
        if (!selected) return;
        const r = await call<Review>("/questions/prepare", { request_id: crypto.randomUUID(), capability: { publisher: selected.owner, id: selected.collection }, question, ...(model ? { model_destination: model } : {}) });
        if (lifetime.current?.signal.aborted) return;
        setReview(r); setConfirmed(false); setAttempted(false); setResult(null); setRequestId(r.question.request_id);
      })}>Prepare remote question</button>
    </fieldset>
    {review ? <fieldset disabled={disabled || busy}><legend>Review exact remote disclosure</legend>
      <p>Destination {review.question.capability.publisher}; collection {review.question.capability.id}, revision {review.question.revision}. Request {review.question.request_id}.</p>
      <p>Mode {review.question.mode}; output {review.question.output_destination}; model {review.question.model_destination ?? "none"}. Execution deadline {review.question.expires_at}.</p>
      <p className="federation-text">{review.question.question}</p>
      <pre className="federation-json">{JSON.stringify(review, null, 2)}</pre>
      <label><input type="checkbox" checked={confirmed} onChange={e => setConfirmed(e.target.checked)} />I approve sending these exact question bytes to this instance for local-human output{review.question.model_destination ? ` and synthesis via ${review.question.model_destination}` : ""}.</label>
      <button disabled={!confirmed} onClick={() => void run(async () => {
        setAttempted(true); setResult(null);
        const response = await call<Result>("/questions/submit", { review, confirmed_digest: review.digest });
        if (!lifetime.current?.signal.aborted) setResult(response);
      })}>{attempted ? "Retry exact question" : "Send approved question"}</button>
      <button onClick={() => { setReview(null); setConfirmed(false); }}>Close review (does not cancel delivery)</button>
      {attempted ? <p>Submission may already have executed. Starting a new question is a new request and may cause repeat work or billing in model mode.</p> : null}
    </fieldset> : null}
    <fieldset disabled={disabled || busy}><legend>Private question status and answer</legend>
      <label>Expertise request ID<input value={requestId} onChange={e => { setRequestId(e.target.value); setResult(null); }} /></label>
      <button disabled={!requestId.trim()} onClick={() => void run(async () => {
        setResult(null);
        const response = await call<Result>(`/questions/${encodeURIComponent(requestId.trim())}`);
        if (!lifetime.current?.signal.aborted) setResult(response);
      })}>Fetch expertise result</button>
    </fieldset>
    {result ? <section aria-label="Expertise result"><p role="status">Question status: {result.status}. Queued is not answered or replicated. Payload access ends {result.payload_until}.</p>
      {result.answer ? <><h4>Untrusted answer — attribution is not factual verification</h4>
        {result.answer.insufficient_evidence ? <p>Insufficient reviewed evidence.</p> : null}
        <p className="federation-text">{result.answer.text}</p>
        <pre className="federation-json" aria-label="Answer citations and attribution">{JSON.stringify({ binding: result.binding, citations: result.answer.citations }, null, 2)}</pre>
      </> : <p>No answer bytes returned. Fetching a completed answer requires fresh origin approval; an offline or revoked origin cannot authorize a cached copy.</p>}
    </section> : null}
  </section>;
}
