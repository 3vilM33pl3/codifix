import { useEffect, useRef, useState } from "react";
import { jobsRequest } from "../../api";
import { JOB_STATUSES, type JobDetail, type JobsPage, type JobStatus } from "./types";

export function JobsTab({ project, readOnly }: { project: string; readOnly: boolean }) {
  return <Workspace key={project} project={project} readOnly={readOnly} />;
}
function Workspace({ project, readOnly }: { project: string; readOnly: boolean }) {
  const [page, setPage] = useState<JobsPage | null>(null);
  const [detail, setDetail] = useState<JobDetail | null>(null);
  const [status, setStatus] = useState<JobStatus | "">("");
  const [after, setAfter] = useState<string | null>(null);
  const [revision, setRevision] = useState(0);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [message, setMessage] = useState("");
  const [pending, setPending] = useState<{ id: string; request_id: string } | null>(null);
  const [confirmed, setConfirmed] = useState(false);
  const lifetime = useRef<AbortController | null>(null);
  useEffect(() => {
    const controller = new AbortController(); lifetime.current = controller;
    return () => controller.abort();
  }, []);
  useEffect(() => {
    const controller = new AbortController();
    const query = new URLSearchParams({ limit: "50" });
    if (status) query.set("status", status);
    if (after) query.set("after", after);
    setError(""); setPage(null);
    jobsRequest<JobsPage>(project, `?${query}`, undefined, controller.signal)
      .then(value => { if (!controller.signal.aborted) setPage(value); })
      .catch(e => { if (!controller.signal.aborted) setError(String(e)); });
    return () => controller.abort();
  }, [project, status, after, revision]);
  async function run(work: () => Promise<void>) {
    if (busy) return;
    setBusy(true); setError(""); setMessage("");
    try { await work(); }
    catch (e) { if (!lifetime.current?.signal.aborted) setError(String(e)); }
    finally { if (!lifetime.current?.signal.aborted) setBusy(false); }
  }
  async function inspect(id: string) {
    const value = await jobsRequest<JobDetail>(project, `/${id}`, undefined, lifetime.current?.signal);
    if (!lifetime.current?.signal.aborted) { setDetail(value); setPending(null); }
  }
  const failure = error.startsWith("Error: 401") ? "Signed out. Sign in to inspect jobs."
    : error.startsWith("Error: 403") ? "Forbidden. Project access and current execution permissions are required."
    : error.startsWith("Error: 503") || error.includes("Failed to fetch") ? "Offline or database unavailable. Job state could not be refreshed."
    : error;
  const locked = readOnly || page?.read_only === true;
  return <section aria-label="Project jobs">
    <h2>Background jobs — {project}</h2>
    <p>Curation success means its data and background intents committed. Enrichment may still be queued, blocked or failed.</p>
    {locked ? <p>Read-only service. Retry is disabled.</p> : null}
    {failure ? <p role="alert">{failure}</p> : null}
    {message ? <p role="status">{message}</p> : null}
    <fieldset disabled={busy || pending !== null}><legend>Inspect project work</legend>
      <label>Status <select value={status} onChange={e => { setStatus(e.target.value as JobStatus | ""); setAfter(null); setDetail(null); }}>
        <option value="">All statuses</option>{JOB_STATUSES.map(s => <option key={s} value={s}>{s}</option>)}
      </select></label>
      <button onClick={() => { setRevision(v => v + 1); setDetail(null); }}>Refresh jobs</button>
      {!page && !error ? <p role="status">Loading jobs…</p> : null}
      {page ? <>
        <p>Queued {page.stats.queued}; running {page.stats.running}; retry wait {page.stats.retry_wait}; blocked {page.stats.blocked}; failed {page.stats.failed}. Oldest ready: {page.stats.oldest_ready_age_seconds === null ? "none" : `${Math.round(page.stats.oldest_ready_age_seconds)} seconds`}.</p>
        <p>Worker {page.worker.enabled ? "enabled" : "disabled"}; last poll: {page.worker.last_poll_at === null ? "not observed" : new Date(page.worker.last_poll_at * 1000).toISOString()}. Refresh to observe changes.</p>
        {page.jobs.length ? <table><caption>Durable stages</caption><thead><tr><th>Stage</th><th>Status</th><th>Attempts / generation</th><th>Next eligible</th><th>Reason</th><th>Details</th></tr></thead>
          <tbody>{page.jobs.map(job => <tr key={job.id}><td>{job.kind}</td><td>{job.status}</td><td>{job.attempts} / {job.generation}</td><td>{job.available_at}</td><td>{job.last_failure ?? job.reason ?? "—"}</td><td><button onClick={() => void run(() => inspect(job.id))}>Inspect {job.id}</button></td></tr>)}</tbody></table> : <p>No jobs match this page and filter.</p>}
        <button disabled={after === null} onClick={() => setAfter(null)}>First page</button>
        <button disabled={!page.next_after} onClick={() => setAfter(page.next_after)}>Next page</button>
      </> : null}
    </fieldset>
    {detail ? <section aria-label="Job details"><h3>{detail.job.kind} — {detail.job.id}</h3>
      <p>{detail.job.status}; failure {detail.job.last_failure ?? "none"}; reason {detail.job.reason ?? "none"}. Operation {detail.job.operation_id}.</p>
      <p>Dependencies: {detail.dependencies.join(", ") || "none"}</p>
      <h4>Stages</h4><ul>{detail.stages.map(stage => <li key={stage.id}>{stage.kind}: {stage.status} ({stage.id})</li>)}</ul>
      {detail.stages_truncated ? <p>Additional stages omitted from this bounded view.</p> : null}
      <h4>Attempts</h4><ul>{detail.history.map(a => <li key={`${a.generation}/${a.attempt}`}>Generation {a.generation}, attempt {a.attempt}: {a.outcome ?? "running"}; {a.failure ?? "no recorded failure"}; {a.started_at} — {a.finished_at ?? "unfinished"}{a.uncertain ? "; provider outcome uncertain: repeat billing is possible" : ""}</li>)}</ul>
      {detail.history_truncated ? <p>Older attempts omitted from this bounded view.</p> : null}
      <button disabled={locked || busy || pending !== null || !["failed", "blocked"].includes(detail.job.status)} onClick={() => { setPending({ id: detail.job.id, request_id: crypto.randomUUID() }); setConfirmed(false); }}>Review retry</button>
    </section> : null}
    {pending ? <section aria-label="Confirm job retry"><h3>Confirm retry</h3>
      <p>Project {project}; job {pending.id}; request {pending.request_id}. Original inputs, completed stages and history are retained. Current authorization and policy still apply. Provider calls may incur repeat billing.</p>
      <label><input type="checkbox" checked={confirmed} disabled={busy || locked} onChange={e => setConfirmed(e.target.checked)} />I confirm this retry.</label>
      <button disabled={!confirmed || busy || locked} onClick={() => void run(async () => {
        await jobsRequest(project, `/${pending.id}/retry`, { request_id: pending.request_id }, lifetime.current?.signal);
        if (lifetime.current?.signal.aborted) return;
        setPending(null); setDetail(null); setMessage("Retry accepted. Execution remains subject to current policy."); setRevision(v => v + 1);
      })}>Confirm retry</button>
      <button disabled={busy} onClick={() => setPending(null)}>Cancel retry</button>
      <p>If the response is uncertain, keep this request ID and retry the same confirmation.</p>
    </section> : null}
  </section>;
}
