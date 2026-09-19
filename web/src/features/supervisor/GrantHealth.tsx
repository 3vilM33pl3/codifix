import { useEffect, useState } from "react";
import type { SupervisorResponse } from "./types";

const LABELS = { valid_window: "Within the reviewed time window", expires_within_day: "Expires within one day", expires_within_hour: "Expires within one hour", expired: "Expired — paid questions are blocked", invalid_profile: "Profile unavailable or invalid — operator review required" };
export function GrantHealth({ grant }: { grant: SupervisorResponse["grant_health"] }) {
  const [now, setNow] = useState(Date.now);
  useEffect(() => { const timer = setInterval(() => setNow(Date.now()), 10000); return () => clearInterval(timer); }, []);
  if (!grant) return <p>Disclosure grant expiry unavailable; no healthy grant is claimed.</p>;
  const observed = Date.parse(grant.observed_at), expires = grant.expires_at ? Date.parse(grant.expires_at) : NaN;
  const stale = !Number.isFinite(observed) || now - observed > 120000 || observed - now > 30000;
  const label = stale ? "Observation stale — refresh status" : !Number.isFinite(expires) ? LABELS.invalid_profile : expires <= now ? LABELS.expired : LABELS[grant.state] ?? "Unknown grant state — refresh status";
  return <section aria-label="Disclosure grant window">
    <h3>Disclosure grant</h3><p role="status">{label}.</p>
    <p>Expires {grant.expires_at ?? "unknown"}. Observed {grant.observed_at}. No automatic renewal.</p>
    <p>This is the disclosure time window, not proof of current permissions or an active worker. New questions still require explicit confirmation.</p>
  </section>;
}
