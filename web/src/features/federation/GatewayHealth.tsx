import { useEffect, useState } from "react";
import type { GatewayHealth as Health } from "./types";
function date(seconds: number | null) { const at = seconds === null ? NaN : seconds * 1000; return Number.isFinite(at) && Math.abs(at) <= 8640000000000000 ? new Date(at).toISOString() : "unknown"; }
export function GatewayHealth({ health }: { health?: Health | null }) {
  const [now, setNow] = useState(() => Date.now() / 1000);
  useEffect(() => { const timer = setInterval(() => setNow(Date.now() / 1000), 10000); return () => clearInterval(timer); }, []);
  if (!health) return <p role="status">Gateway credential status unavailable. Local retained data remains usable; publication health is unknown.</p>;
  const c = health.credentials;
  const stale = !Number.isFinite(c.observed_at) || now - c.observed_at > 120 || c.observed_at - now > 30;
  const expired = c.expires_at !== null && c.expires_at <= now;
  const usable = !stale && !expired && c.state === "usable" && c.publication_usable;
  return <section aria-label="Gateway credential health">
    <h3>Gateway publication credentials</h3>
    <p role="status">{stale ? "Observation stale — refresh status" : expired ? "Expired — publication blocked" : usable ? "Credential eligible for publication attempts" : `Publication credential requires attention: ${c.state}`}.</p>
    <p>Access expires {date(c.expires_at)}; generation {c.generation ?? "unknown"}. Observed {date(c.observed_at)}.</p>
    <p>Renewal: {c.maintenance_state}; last success {date(c.last_success)}; next retry {date(c.retry_at)}. {c.reason ?? ""}</p>
    <p>Observed PDS operator TLS expiry {date(c.tls_expires_at)}. {c.tls_expires_at !== null && c.tls_expires_at - now <= 86400 ? "Certificate renewal needs operator attention; no automatic CA changes." : ""}</p>
    {health.pending ? <p>Publication {health.pending.sequence}: {health.pending.blocked ? "blocked" : "pending"}; {health.pending.attempts} attempts; {health.pending.failure ?? "no recorded failure"}.</p> : null}
    <p>Credential eligibility is not proof that the PDS is reachable or a publication has succeeded.</p>
  </section>;
}
