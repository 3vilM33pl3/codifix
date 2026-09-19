import { useEffect, useRef, useState } from "react";
import { supervisorPrivate, supervisorReview, type SupervisorProposalDetail, type SupervisorReviewRequest } from "../../api";
import { QuestionsPanel } from "./QuestionsPanel";

export function ProposalsPanel({ project }: { project: string }) {
  const [actions, setActions] = useState<string[]>([]);
  const [after, setAfter] = useState<string | null>(null);
  const [detail, setDetail] = useState<SupervisorProposalDetail | null>(null);
  const [pending, setPending] = useState<SupervisorReviewRequest | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const lifecycle = useRef(new AbortController());
  useEffect(() => { lifecycle.current = new AbortController(); const current = lifecycle.current; return () => current.abort(); }, []);
  async function list(cursor?: string) {
    const controller = lifecycle.current;
    setBusy(true); setDetail(null); setActions([]); setError("");
    try {
      const result = await supervisorPrivate<{ actions: string[]; next_after: string | null }>(project, cursor ? `?after=${encodeURIComponent(cursor)}` : "", controller.signal);
      if (!controller.signal.aborted) { setActions(result.actions); setAfter(result.next_after); }
    } catch (e) { if (!controller.signal.aborted) setError(String(e)); }
    finally { if (!controller.signal.aborted) setBusy(false); }
  }
  async function open(id: string) {
    const controller = lifecycle.current;
    setBusy(true); setDetail(null); setError("");
    try {
      const result = await supervisorPrivate<SupervisorProposalDetail>(project, `/${encodeURIComponent(id)}`, controller.signal);
      if (!controller.signal.aborted) setDetail(result);
    } catch (e) { if (!controller.signal.aborted) setError(String(e)); }
    finally { if (!controller.signal.aborted) setBusy(false); }
  }
  function prepare(decision: "approve" | "reject") {
    if (detail && !pending) { setError(""); setPending({ request_id: crypto.randomUUID(), proposal_digest: detail.proposal_digest, decision }); }
  }
  async function confirm() {
    const controller = lifecycle.current;
    if (!detail || !pending) return;
    setBusy(true); setError("");
    try {
      const review = await supervisorReview(project, detail.action_id, pending);
      if (!controller.signal.aborted) { setDetail({ ...detail, review, can_review: false }); setPending(null); }
    } catch (e) { if (!controller.signal.aborted) setError(String(e)); }
    finally { if (!controller.signal.aborted) setBusy(false); }
  }
  return <section aria-label="Private supervisor proposals">
    <h3>Questions and proposals · {project}</h3>
    <p>Private project content, separate from the operational feed. Reviewing a suggestion does not execute it, change a memory or install software.</p>
    <QuestionsPanel key={project} project={project} onOpenProposal={id => { if (!pending && !busy) void open(id); }} />
    <button disabled={busy || !!pending} onClick={() => void list()}>Load saved proposals</button>
    <button disabled={busy || !!pending || !after} onClick={() => void list(after!)}>Next proposals</button>
    {busy ? <p role="status">Loading private supervisor result…</p> : null}
    {error ? <p role="alert">{error}</p> : null}
    <ul>{actions.map(id => <li key={id}><button disabled={busy || !!pending} onClick={() => void open(id)}>Open proposal {id}</button></li>)}</ul>
    {detail ? <article aria-label="Saved supervisor proposal">
      <h4>Question</h4><p style={{ whiteSpace: "pre-wrap" }}>{detail.question}</p>
      <h4>Supervisor response</h4><p style={{ whiteSpace: "pre-wrap" }}>{detail.summary}</p>
      <p>Suggested next step: {detail.proposal.replaceAll("_", " ")}. This is a model suggestion, not verified truth.</p>
      <p>Saved {detail.created_at}. Digest <code>{detail.proposal_digest}</code>.</p>
      <ul>{detail.citations.map(c => <li key={c.index}>Evidence {c.index + 1}: version {c.version}, record <code>{c.version_id}</code>, SHA-256 <code>{c.evidence_digest}</code></li>)}</ul>
      {detail.review ? <p role="status">Review recorded: {detail.review.decision}. Reviewer {detail.review.reviewer_id}. No action executed.</p> : null}
      {detail.can_review ? <><button disabled={busy || !!pending} onClick={() => prepare("approve")}>Approve proposal</button><button disabled={busy || !!pending} onClick={() => prepare("reject")}>Reject proposal</button></> : null}
    </article> : null}
    {pending ? <div role="dialog" aria-label="Confirm proposal review">
      <p>Record {pending.decision} for proposal {detail?.action_id} with digest {pending.proposal_digest}?</p>
      <p>No installation, source edit, model call or execution permission is granted. This decision is permanent.</p>
      <p>Request {pending.request_id}. An unconfirmed request must be retried with these same terms.</p>
      <button disabled={busy} onClick={() => void confirm()}>{error ? "Retry same review" : "Confirm review"}</button>
      <button disabled={busy} onClick={() => { setPending(null); setDetail(null); setError(""); }}>Close and inspect</button>
    </div> : null}
  </section>;
}
