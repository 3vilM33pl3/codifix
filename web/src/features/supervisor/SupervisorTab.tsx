import { useEffect, useState } from "react";
import { supervisorStatus } from "../../api";
import { dollars, EVENT_LABELS, MODEL_WORK_LABELS, type SupervisorResponse } from "./types";
import { SupervisorControls } from "./SupervisorControls";
import { ProposalsPanel } from "./ProposalsPanel";

export function SupervisorTab({ project }: { project?: string }) {
  const [data, setData] = useState<SupervisorResponse | null>(null);
  const [after, setAfter] = useState("0");
  const [revision, setRevision] = useState(0);
  const [error, setError] = useState("");
  const [controlPending, setControlPending] = useState(false);
  useEffect(() => {
    const controller = new AbortController();
    setData(null); setError("");
    supervisorStatus(after, controller.signal).then(result => {
      if (!controller.signal.aborted) setData(result);
    }).catch((failure: unknown) => {
      if (controller.signal.aborted) return;
      const text = String(failure);
      setError(text.startsWith("Error: 401") ? "Signed out. Sign in to inspect the supervisor."
        : text.startsWith("Error: 403") ? "Forbidden. Instance-wide read permission is required; project access alone is not enough."
        : text.startsWith("Error: 409") ? "Identity mismatch. The configured area and database binding need operator review."
        : text.startsWith("Error: 503") || text.includes("Failed to fetch") ? "Offline or storage unavailable. Supervisor state could not be refreshed."
        : "Supervisor inspection failed. No current state is available.");
    });
    return () => controller.abort();
  }, [after, revision]);
  const snapshot = data?.snapshot;
  function refresh(cursor: string) { setData(null); setError(""); setAfter(cursor); setRevision(v => v + 1); }
  return <section aria-label="Area supervisor">
    <h2>Area supervisor</h2>
    <p>This installation only. Operational status is instance-wide; private proposals use the selected project. Other areas are not contacted.</p>
    {project ? <ProposalsPanel key={project} project={project} /> : null}
    <button disabled={controlPending} onClick={() => refresh(after)}>Refresh supervisor</button>
    {error ? <p role="alert">{error}</p> : null}
    {!data && !error ? <p role="status">Loading supervisor…</p> : null}
    {data ? <>
      <p role="status">Full autonomous supervisor execution is not integrated. Bounded proposal work is shown separately; no healthy agent heartbeat is claimed.</p>
      {data.model_work ? <section aria-label="Bounded proposal worker">
        <h3>Single-proposal worker</h3>
        <p>{MODEL_WORK_LABELS[data.model_work.state] ?? "Unknown worker state — operator review required"}.</p>
        <p>Observed {data.model_work.observed_at}. Action {data.model_work.action_id ?? "not loaded"}; recorded state {data.model_work.action_state ?? "unavailable"}.</p>
        <p>One fixed proposal, not autonomous agency. Restart does not reissue finished or uncertain work. This is a process-local observation; durable history below is authoritative.</p>
      </section> : null}
      <p>Deterministic operational checks: {data.deterministic_checks_configured ? "configured; current policy and writable service required" : "not configured"}. These do not enable Pi or model calls.</p>
      {data.read_only ? <p>Read-only service. This panel never starts work or changes policy.</p> : null}
      {!snapshot ? <p>No durable supervisor identity is configured. Nothing was enrolled or enabled.</p>
        : <>
          <h3>{snapshot.area ? `${snapshot.area[0].toUpperCase()}${snapshot.area.slice(1)}` : "Area not bound"}</h3>
          <p>Instance {snapshot.instance_id}. Observed {snapshot.observed_at}.</p>
          {!snapshot.area ? <p>Configuration exists, but the database has no supervisor binding.</p> : null}
          {snapshot.policy ? <>
            <p>Policy: {snapshot.policy.revoked ? "revoked" : snapshot.policy.enabled ? "enabled — worker activation is separate" : "paused"}. Revision {snapshot.policy.revision}.</p>
            <p>Model {snapshot.policy.model}; daily ceiling {dollars(snapshot.policy.daily_usd_micros)} and {snapshot.policy.daily_calls} calls.</p>
          </> : <p>No supervisor policy installed.</p>}
          {snapshot.budget ? <p>UTC accounting day {snapshot.budget.day}: {snapshot.budget.calls} calls; charged {dollars(snapshot.budget.charged_usd_micros)}; reserved or uncertain {dollars(snapshot.budget.reserved_usd_micros)}. Uncertain charges remain reserved.</p> : null}
          <SupervisorControls data={data} onInspect={() => refresh("0")} onPendingChange={setControlPending} />
          <h3>Recorded actions</h3>
          {snapshot.last_health_observation ? <section aria-label="Last operational observation">
            <p>Observed {snapshot.last_health_observation.observed_at}; next due {snapshot.last_health_observation.next_due_at}.</p>
            <p>Policy {snapshot.last_health_observation.policy_revision}; action {snapshot.last_health_observation.action_id}.</p>
            <p>{snapshot.last_health_observation.project_count} projects; {snapshot.last_health_observation.canonical_count} canonical histories; {snapshot.last_health_observation.blocked_jobs} blocked jobs; {snapshot.last_health_observation.failed_jobs} failed jobs; {snapshot.last_health_observation.uncertain_actions} uncertain actions.</p>
            <p>Historical database observation only. It does not prove current availability, backup recovery, remote connectivity or model health.</p>
          </section> : null}
          {snapshot.action_counts.length ? <ul>{snapshot.action_counts.map(c => <li key={c.state}>{c.state}: {c.count}</li>)}</ul> : <p>No recorded actions.</p>}
          <h3>Action history</h3>
          <p>Metadata only: no questions, source contents, private proposals or model prompts.</p>
          {snapshot.journal.length ? <table><caption>Durable supervisor history</caption>
            <thead><tr><th>Sequence</th><th>Time</th><th>Event</th><th>Action</th><th>Attempt</th></tr></thead>
            <tbody>{snapshot.journal.map(item => <tr key={item.sequence}>
              <td>{item.sequence}</td><td>{item.recorded_at}</td><td>{EVENT_LABELS[item.event] ?? "Unrecognized event"}</td>
              <td>{item.action_id ?? "—"}</td><td>{item.attempt_id ?? "—"}</td>
            </tr>)}</tbody></table> : <p>No newer history entries on this page.</p>}
          <button disabled={controlPending || after === "0"} onClick={() => refresh("0")}>First history page</button>
          <button disabled={controlPending || !snapshot.has_more} onClick={() => refresh(snapshot.next_after)}>Next history page</button>
        </>}
    </> : null}
  </section>;
}
