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

export function dollars(micros: string): string {
  if (!/^\d+$/.test(micros)) return "unavailable";
  const amount = BigInt(micros);
  return `$${amount / 1_000_000n}.${String(amount % 1_000_000n).padStart(6, "0")}`;
}
