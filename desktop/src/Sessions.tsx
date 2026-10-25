import { Button } from "./ui";
import { useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import {
  request,
  projectPath,
  time,
  errorText,
  type AgentSnapshotResponse,
  type ProjectOverviewResponse,
  type ResumeResponse,
} from "./service";
import { useProjectData } from "./data";
import { Badge, Empty, Intro, QueryState, Technical } from "./ui";
export function Sessions({
  scope,
  project,
  repoRoot,
  connected,
}: {
  scope: string;
  project: string;
  repoRoot: string | null;
  connected: boolean;
}) {
  const agents = useQuery({
    queryKey: [scope, project, "agents"],
    queryFn: () => request<AgentSnapshotResponse>("/v1/agents"),
    retry: false,
    refetchInterval: 30_000,
  });
  const overview = useProjectData<ProjectOverviewResponse>(
    scope,
    project,
    "overview",
  );
  const [selected, setSelected] = useState("");
  const [recent, setRecent] = useState(false);
  const sessions = (agents.data?.sessions ?? []).filter(
    (s) =>
      (repoRoot ? s.cwd === repoRoot : s.project_name === project) &&
      (recent || s.status !== "done"),
  );
  const session =
    sessions.find((s) => s.session_id === selected) ?? sessions[0];
  const briefing = useMutation({
    mutationFn: () =>
      request<ResumeResponse>(projectPath(project, "resume"), {
        project,
        repo_root: repoRoot,
        include_llm_summary: false,
        limit: 20,
      }),
  });
  return (
    <>
      <Intro
        title="Sessions"
        subtitle="Follow agent work and pick up your project’s context."
      />
      <div className="row">
        <div className="actions">
          <Button
            aria-pressed={!recent}
            className={!recent ? "selected" : ""}
            onClick={() => setRecent(false)}
          >
            Active
          </Button>
          <Button
            aria-pressed={recent}
            className={recent ? "selected" : "ghost"}
            onClick={() => setRecent(true)}
          >
            Recent
          </Button>
        </div>
        <span className="caption">
          This project ·{" "}
          {repoRoot ? "Exact folder match" : "Project name match"}
        </span>
      </div>
      <div className="split">
        <section className="card result-list">
          <QueryState query={agents}>
            {sessions.map((s) => (
              <Button
                className={`result ${s.session_id === session?.session_id ? "selected" : ""}`}
                key={s.session_id}
                onClick={() => setSelected(s.session_id)}
              >
                <strong className="clamp">
                  {s.current_tasks[0] || s.initial_prompt || s.project_name}
                </strong>
                <span className="muted">
                  {s.agent_cli} · {s.model}
                </span>
                <Badge tone={s.status === "working" ? "positive" : ""}>
                  {s.status}
                </Badge>
                <span className="caption">
                  Capture status unavailable for this session
                </span>
              </Button>
            ))}
            {!sessions.length && (
              <Empty title="No sessions detected">
                Open your coding agent in this project. Completed processes may
                no longer appear in the monitor.
              </Empty>
            )}
          </QueryState>
        </section>
        <section className="card detail">
          {session ? (
            <>
              <div className="row">
                <span className="caption">{session.agent_cli}</span>
                <Badge tone={session.status === "working" ? "positive" : ""}>
                  {session.status}
                </Badge>
              </div>
              <h2>
                {session.current_tasks[0] ||
                  session.initial_prompt ||
                  session.project_name}
              </h2>
              <p className="prose muted">
                {session.first_assistant_text ||
                  "No session summary has been reported."}
              </p>
              <Technical
                label="Session reference & technical details"
                value={{
                  session_id: session.session_id,
                  cwd: session.cwd,
                  agent: session.agent_cli,
                  started_at: new Date(session.started_at * 1000).toISOString(),
                  status: session.status,
                }}
              />
              <p className="caption">
                Use this session reference in your agent. Direct agent handoff
                is not supported by the service.
              </p>
              <hr />
            </>
          ) : (
            <h2>Resume your project</h2>
          )}
          <strong>Latest project capture</strong>
          <QueryState query={overview}>
            <p className="muted">
              {time(overview.data?.last_capture_at)} · Project-wide; not a
              per-session capture indicator.
            </p>
          </QueryState>
          <Button
            className="primary"
            disabled={!connected || briefing.isPending}
            onClick={() => briefing.mutate()}
          >
            {briefing.isPending
              ? "Preparing briefing…"
              : "Load resume briefing"}
          </Button>
          {briefing.error && (
            <p role="alert" className="notice danger">
              {errorText(briefing.error)}
            </p>
          )}
          {briefing.data && briefing.data.project === project && (
            <article className="briefing">
              <h2>Latest checkpoint & next steps</h2>
              <p className="prose">{briefing.data.briefing}</p>
              {briefing.data.primary_next_step && (
                <Technical
                  label="Suggested next action"
                  value={briefing.data.primary_next_step}
                />
              )}
              <h3>Captured knowledge</h3>
              {briefing.data.context_items.map((m) => (
                <p key={m.id}>
                  <strong>{m.summary}</strong>
                  <br />
                  <span className="muted">{m.preview}</span>
                </p>
              ))}
              {briefing.data.warnings.map((warning, i) => (
                <p key={i} className="notice warning">
                  {warning}
                </p>
              ))}
            </article>
          )}
        </section>
      </div>
    </>
  );
}
