import { useEffect, useRef, useState } from "react";
import { supervisorQuestions, supervisorQuestionSubmit, type SupervisorQuestionList, type SupervisorQuestionPreview, type SupervisorQuestionRequest, type SupervisorQuestionStatus } from "../../api";
import { dollars } from "./types";

export function QuestionsPanel({ project, onOpenProposal }: { project: string; onOpenProposal: (action: string) => void }) {
  const [question, setQuestion] = useState("");
  const [preview, setPreview] = useState<SupervisorQuestionPreview | null>(null);
  const [pending, setPending] = useState<SupervisorQuestionRequest | null>(null);
  const [attempted, setAttempted] = useState(false);
  const [page, setPage] = useState<SupervisorQuestionList | null>(null);
  const [status, setStatus] = useState<SupervisorQuestionStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const lifecycle = useRef(new AbortController());
  useEffect(() => { lifecycle.current = new AbortController(); const current = lifecycle.current; return () => current.abort(); }, []);
  const bytes = new TextEncoder().encode(question).length;
  async function prepare() {
    const controller = lifecycle.current;
    setBusy(true); setError(""); setStatus(null);
    try {
      const result = await supervisorQuestions<SupervisorQuestionPreview>(project, "/prepare", controller.signal, { question });
      if (result.question !== question) throw new Error("Question preview mismatch. Nothing submitted.");
      if (!controller.signal.aborted) {
        setPreview(result); setAttempted(false);
        setPending({ request_id: crypto.randomUUID(), question, profile_digest: result.profile_digest, request_digest: result.request_digest });
      }
    } catch (e) { if (!controller.signal.aborted) setError(String(e)); }
    finally { if (!controller.signal.aborted) setBusy(false); }
  }
  async function submit() {
    if (!pending) return;
    const controller = lifecycle.current;
    setBusy(true); setAttempted(true); setError("");
    try {
      const result = await supervisorQuestionSubmit(project, pending);
      if (!controller.signal.aborted) { setStatus(result); setPending(null); setPreview(null); }
    } catch (e) { if (!controller.signal.aborted) setError(String(e)); }
    finally { if (!controller.signal.aborted) setBusy(false); }
  }
  async function refresh(id: string) {
    const controller = lifecycle.current;
    setBusy(true); setError(""); setStatus(null);
    try {
      const result = await supervisorQuestions<SupervisorQuestionStatus>(project, `/${encodeURIComponent(id)}`, controller.signal);
      if (result.request_id !== id) throw new Error("Question status mismatch.");
      if (!controller.signal.aborted) { setStatus(result); if (pending?.request_id === id) { setPending(null); setPreview(null); } }
    } catch (e) { if (!controller.signal.aborted) setError(String(e)); }
    finally { if (!controller.signal.aborted) setBusy(false); }
  }
  async function list(after?: string) {
    const controller = lifecycle.current;
    setBusy(true); setError(""); setPage(null); setStatus(null);
    try {
      const result = await supervisorQuestions<SupervisorQuestionList>(project, after ? `?after=${encodeURIComponent(after)}` : "", controller.signal);
      if (!controller.signal.aborted) setPage(result);
    } catch (e) { if (!controller.signal.aborted) setError(String(e)); }
    finally { if (!controller.signal.aborted) setBusy(false); }
  }
  return <section aria-label="Ask Workshop supervisor">
    <h4>Ask a private question</h4>
    <p>One question and the approved collection only. No conversation history, files, ordinary searches or other areas are attached. Preparing is free; confirming may make a paid model call.</p>
    <label htmlFor="supervisor-question">Exact question</label>
    <textarea id="supervisor-question" rows={4} value={question} disabled={busy || !!pending} onChange={e => setQuestion(e.target.value)} />
    <p>{bytes} / 4096 UTF-8 bytes</p>
    <button disabled={busy || !!pending || !question.trim() || bytes > 4096} onClick={() => void prepare()}>Prepare question</button>
    <button disabled={busy || !!pending} onClick={() => void list()}>My questions</button>
    {page?.next_after ? <button disabled={busy || !!pending} onClick={() => void list(page.next_after!)}>Next questions</button> : null}
    {busy ? <p role="status">Checking private question…</p> : null}
    {error ? <p role="alert">{error}</p> : null}
    {preview && pending ? <div role="dialog" aria-label="Confirm paid supervisor question">
      <p style={{ whiteSpace: "pre-wrap" }}>{pending.question}</p>
      <p>Destination: {preview.destination}. Model: {preview.model}. Approved sources: {preview.source_count}.</p>
      <p>Daily limits: {preview.daily_call_limit} calls and {dollars(preview.daily_usd_micros)} USD.</p>
      <p>Profile {pending.profile_digest}; request digest {pending.request_digest}; request ID {pending.request_id}.</p>
      <p>Permit one bounded answer attempt, not tools or automatic actions. Uncertain provider work is not automatically retried. Starting another question can repeat billing.</p>
      <button disabled={busy} onClick={() => void submit()}>{attempted ? "Retry exact submission" : "Confirm paid question"}</button>
      {attempted ? <button disabled={busy} onClick={() => void refresh(pending.request_id)}>Inspect this request</button>
        : <button disabled={busy} onClick={() => { setPreview(null); setPending(null); }}>Cancel question</button>}
    </div> : null}
    {status ? <article aria-label="Question status">
      <p>Request {status.request_id}: {status.state}. Deadline {status.expires_at}. Queued is not answered.</p>
      <button disabled={busy || !!pending} onClick={() => void refresh(status.request_id)}>Refresh question status</button>
      {status.proposal_available ? <button disabled={busy || !!pending} onClick={() => onOpenProposal(status.action_id)}>Read cited answer</button> : null}
      {status.state === "uncertain" ? <p>Provider execution is uncertain; no automatic retry. A new request can cause repeat billing.</p> : null}
    </article> : null}
    {page ? <ul>{page.questions.length ? page.questions.map(item => <li key={item.request_id}>
      <button disabled={busy || !!pending} onClick={() => void refresh(item.request_id)}>Inspect question {item.request_id} · {item.state}</button>
    </li>) : <li>No retained questions for this requester.</li>}</ul> : null}
  </section>;
}
