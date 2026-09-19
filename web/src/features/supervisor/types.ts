export type SupervisorArea = "trust" | "homelab" | "workshop" | "general";
export const EVENT_LABELS = {
  policy_installed: "Policy prepared", policy_enabled: "Policy enabled", policy_paused: "Policy paused",
  policy_revoked: "Policy revoked", policy_reduced: "Limits reduced", observed: "Work observed",
  proposed: "Action proposed", authorized: "Action authorized", queued: "Action queued", claimed: "Worker claimed action",
  renewed: "Worker lease renewed", verified: "Outcome verified", failed: "Action failed", denied: "Action denied",
  expired: "Action expired", cancelled: "Action cancelled", uncertain: "Outcome uncertain — reconciliation required",
  reserved: "Model budget reserved", begun: "Provider attempt started", released_before_send: "Reservation released before sending",
  settled: "Usage recorded", settled_over_bound: "Usage exceeded reservation — policy revoked", model_context_bound: "Exact model input bound",
} as const;
export interface SupervisorResponse {
  read_only: boolean;
  execution: "not_integrated";
  can_control?: boolean;
  deterministic_checks_configured?: boolean;
  grant_health?: null | { observed_at: string; expires_at: string | null; state: "valid_window" | "expires_within_day" | "expires_within_hour" | "expired" | "invalid_profile"; automatic_renewal: false };
  model_work?: null | {
    state: keyof typeof MODEL_WORK_LABELS;
    observed_at: string;
    action_id: string | null;
    attempt_id: string | null;
    action_state: string | null;
  };
  snapshot: null | {
    instance_id: string;
    area: SupervisorArea | null;
    observed_at: string;
    policy: null | { revision: string; enabled: boolean; revoked: boolean; model: string; daily_usd_micros: string; daily_calls: number };
    budget: null | { day: string; charged_usd_micros: string; reserved_usd_micros: string; calls: number };
    last_health_observation?: null | { action_id: string; policy_revision: string; observed_at: string; next_due_at: string; project_count: string; canonical_count: string; blocked_jobs: string; failed_jobs: string; uncertain_actions: string };
    action_counts: { state: string; count: string }[];
    journal: { sequence: string; action_id: string | null; attempt_id: string | null; policy_revision: string; event: keyof typeof EVENT_LABELS; recorded_at: string }[];
    next_after: string;
    has_more: boolean;
  };
}

export const MODEL_WORK_LABELS = {
  starting: "Checking the fixed work configuration",
  waiting_for_database: "Waiting for the local database",
  awaiting_authority: "Waiting for current policy, permissions or eligible sources",
  ready: "Preparing the authorized work",
  running: "Work claimed — inspect durable history for progress",
  completed: "Proposal stored and delivery confirmed",
  stored: "Proposal stored — delivery not confirmed by this process",
  stopped: "Worker stopped",
  intervention_required: "Operator review required — no automatic paid retry",
  invalid_configuration: "Work configuration rejected",
  credential_unavailable: "Configured provider credential unavailable — no new work will be claimed",
  artifact_rejected: "Private runtime files or artifact hashes rejected",
  sandbox_unavailable: "Required sandbox unavailable — no automatic paid retry",
  cleanup_unconfirmed: "Sandbox removal unconfirmed — operator cleanup required",
  delivery_unconfirmed: "Proposal delivery unconfirmed — inspect stored work and usage",
  execution_deadline: "Sandbox deadline elapsed — inspect stored work and usage",
} as const;

export function dollars(micros: string): string {
  if (!/^\d+$/.test(micros)) return "unavailable";
  const amount = BigInt(micros);
  return `$${amount / 1_000_000n}.${String(amount % 1_000_000n).padStart(6, "0")}`;
}
