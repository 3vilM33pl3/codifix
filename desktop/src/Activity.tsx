import { Button } from "./ui";
import { useRef, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useProjectData } from "./data";
import {
  canWrite,
  errorText,
  projectPath,
  request,
  time,
  type ActivityEvent,
  type ActivityListResponse,
  type AuthMeResponse,
  type JobDetail,
  type JobsPage,
} from "./service";
import { Badge, Empty, Intro, Modal, QueryState, Technical } from "./ui";
export function Activity({
  scope,
  project,
  auth,
  connected,
  onReview,
}: {
  scope: string;
  project: string;
  auth: AuthMeResponse;
  connected: boolean;
  onReview: () => void;
}) {
  const [tab, setTab] = useState("All");
  const [event, setEvent] = useState<ActivityEvent | null>(null);
  const [jobId, setJobId] = useState("");
  const [after, setAfter] = useState("");
  const [confirmed, setConfirmed] = useState(false);
  const requests = useRef(new Map<string, string>());
  const [receipt, setReceipt] = useState("");
  const client = useQueryClient();
  const activity = useProjectData<ActivityListResponse>(
    scope,
    project,
    "activities?limit=100&include_details=true",
  );
  const jobs = useProjectData<JobsPage>(
    scope,
    project,
    `jobs?limit=50${after ? `&after=${encodeURIComponent(after)}` : ""}`,
    tab === "Jobs",
  );
  const detail = useQuery({
    queryKey: [scope, project, "job", jobId],
    queryFn: () => request<JobDetail>(projectPath(project, `jobs/${jobId}`)),
    enabled: !!jobId,
    retry: false,
    refetchInterval: 15_000,
  });
  const retry = useMutation({
    mutationFn: async () => {
      let requestId = requests.current.get(jobId);
      if (!requestId) {
        requestId = crypto.randomUUID();
        requests.current.set(jobId, requestId);
      }
      const result = await request<{
        request_id: string;
        job_id: string;
        generation: number;
      }>(projectPath(project, `jobs/${jobId}/retry`), {
        request_id: requestId,
      });
      if (
        result.request_id !== requestId ||
        result.job_id !== jobId ||
        !Number.isInteger(result.generation) ||
        result.generation < 1
      )
        throw new Error(
          "Retry receipt did not match. Keep the same request reference and refresh the job.",
        );
      requests.current.delete(jobId);
    },
    onSuccess: async () => {
      setReceipt(
        "Retry accepted. Execution remains subject to current policy.",
      );
      setConfirmed(false);
      await client.invalidateQueries({ queryKey: [scope, project] });
    },
  });
  const events = (activity.data?.items ?? []).filter(
    (e) =>
      tab !== "Errors" || /error|fail|blocked/i.test(`${e.kind} ${e.summary}`),
  );
  const locked =
    !connected ||
    !canWrite(auth, project, "memory_curate") ||
    !canWrite(auth, project, "loops_run") ||
    jobs.data?.read_only ||
    !!detail.error ||
    retry.isPending ||
    !["failed", "blocked"].includes(detail.data?.job.status ?? "");
  return (
    <>
      <Intro
        title="Activity"
        subtitle="A readable history of captures, reviews and background work."
      />
      <div className="row">
        <div className="actions" role="group" aria-label="Activity filter">
          {["All", "Errors", "Jobs"].map((t) => (
            <Button
              key={t}
              aria-pressed={t === tab}
              className={t === tab ? "selected" : "ghost"}
              onClick={() => setTab(t)}
            >
              {t}
            </Button>
          ))}
        </div>
        <Button
          className="ghost"
          onClick={() =>
            void client.invalidateQueries({ queryKey: [scope, project] })
          }
        >
          Refresh
        </Button>
      </div>
      {tab === "Jobs" ? (
        <QueryState query={jobs}>
          <section className="card timeline">
            <h2>Background jobs</h2>
            <p className="caption">
              Worker {jobs.data?.worker.enabled ? "enabled" : "disabled"} ·{" "}
              {jobs.data?.stats.failed ?? 0} failed
            </p>
            {jobs.data?.jobs.map((job) => (
              <div className="timeline-item" key={job.id}>
                <div>
                  <strong>{job.kind.replaceAll("_", " ")}</strong>
                  <p className="muted">
                    {job.last_failure ||
                      job.reason ||
                      `Available ${time(job.available_at)}`}
                  </p>
                </div>
                <Badge tone={job.status === "failed" ? "danger" : ""}>
                  {job.status}
                </Badge>
                <Button
                  onClick={() => {
                    setJobId(job.id);
                    setConfirmed(false);
                    setReceipt("");
                    retry.reset();
                  }}
                >
                  Details
                </Button>
              </div>
            ))}
            {!jobs.data?.jobs.length && <Empty title="No jobs on this page" />}
            <div className="row">
              <Button disabled={!after} onClick={() => setAfter("")}>
                First page
              </Button>
              <Button
                disabled={!jobs.data?.next_after}
                onClick={() => setAfter(jobs.data!.next_after!)}
              >
                Next page
              </Button>
            </div>
          </section>
        </QueryState>
      ) : (
        <QueryState query={activity}>
          <section className="card timeline">
            <p className="eyebrow">LATEST ACTIVITY</p>
            {events.map((item) => (
              <div className="timeline-item" key={item.id}>
                <time className="caption" dateTime={item.recorded_at}>
                  {time(item.recorded_at)}
                </time>
                <div>
                  <strong>{item.summary}</strong>
                  <p className="muted">
                    {item.kind.replaceAll("_", " ")}
                    {item.actor_name ? ` · ${item.actor_name}` : ""}
                  </p>
                </div>
                <Button onClick={() => setEvent(item)}>Details</Button>
              </div>
            ))}
            {!events.length && (
              <Empty
                title={
                  tab === "Errors"
                    ? "No errors in the latest activity"
                    : "No activity yet"
                }
              />
            )}
            <p className="caption">
              Latest {activity.data?.total_returned ?? 0} events.
              {activity.data?.cursor?.has_more
                ? " Older events are available in the service’s web interface."
                : ""}
            </p>
          </section>
        </QueryState>
      )}
      <Modal
        open={!!event}
        title="Activity details"
        description={event?.summary ?? ""}
        onClose={() => setEvent(null)}
      >
        {event && (
          <>
            <p className="muted">{time(event.recorded_at)}</p>
            <Technical label="Recorded event" value={event} />
            {/review|proposal/.test(event.kind) && (
              <Button
                onClick={() => {
                  setEvent(null);
                  onReview();
                }}
              >
                Review updates
              </Button>
            )}
            {/error|fail/.test(`${event.kind} ${event.summary}`) && (
              <>
                <p>
                  Inspect the recorded failure and related job before retrying.
                </p>
                <Button
                  onClick={() => {
                    setEvent(null);
                    setTab("Jobs");
                  }}
                >
                  View background jobs
                </Button>
              </>
            )}
          </>
        )}
      </Modal>
      <Modal
        open={!!jobId}
        title="Background job"
        description="Inspect the failure and its history before retrying."
        onClose={() => !retry.isPending && setJobId("")}
      >
        <QueryState query={detail}>
          {detail.data && (
            <>
              <Badge tone={detail.data.job.status === "failed" ? "danger" : ""}>
                {detail.data.job.status}
              </Badge>
              <h2>{detail.data.job.kind.replaceAll("_", " ")}</h2>
              <p className="prose">
                {detail.data.job.last_failure ||
                  detail.data.job.reason ||
                  "No failure recorded."}
              </p>
              <Technical
                label="Attempts, stages & dependencies"
                value={detail.data}
              />
              <p className="caption">
                Retrying retains original inputs and history. Provider calls may
                incur repeat billing.
              </p>
              <label className="check">
                <input
                  type="checkbox"
                  checked={confirmed}
                  disabled={locked}
                  onChange={(e) => setConfirmed(e.target.checked)}
                />
                I confirm this job retry.
              </label>
              <Button
                className="primary"
                disabled={locked || !confirmed}
                onClick={() => retry.mutate()}
              >
                {retry.isPending ? "Requesting retry…" : "Confirm retry"}
              </Button>
              {requests.current.get(jobId) && (
                <p className="caption">
                  Request reference: {requests.current.get(jobId)}
                </p>
              )}
              {retry.error && (
                <p role="alert" className="notice danger">
                  Outcome unconfirmed. {errorText(retry.error)} Retrying this
                  job uses the same request reference.
                </p>
              )}
              {receipt && (
                <p role="status" className="notice positive">
                  {receipt}
                </p>
              )}
            </>
          )}
        </QueryState>
      </Modal>
    </>
  );
}
