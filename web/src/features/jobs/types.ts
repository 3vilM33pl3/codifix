export const JOB_STATUSES = ["queued", "running", "retry_wait", "blocked", "failed", "succeeded", "skipped"] as const;
export type JobStatus = typeof JOB_STATUSES[number];
export interface JobSummary {
  id: string; project_id: string; operation_id: string; kind: string;
  status: JobStatus; generation: number; attempts: number;
  created_at: string; available_at: string; lease_until: string | null;
  last_failure: string | null; reason?: string;
}
export interface JobDetail {
  job: JobSummary; stages: JobSummary[]; stages_truncated: boolean;
  dependencies: string[]; history_truncated: boolean;
  history: { generation: number; attempt: number; started_at: string;
    finished_at: string | null; outcome: string | null; failure: string | null; uncertain: boolean }[];
}
export interface JobsPage {
  read_only?: boolean;
  jobs: JobSummary[]; next_after: string | null;
  stats: { queued: number; running: number; retry_wait: number; blocked: number; failed: number; oldest_ready_age_seconds: number | null };
  worker: { enabled: boolean; last_poll_at: number | null };
}
