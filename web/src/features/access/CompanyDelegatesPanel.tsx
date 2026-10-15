import { useCallback, useEffect, useRef, useState } from "react";
import { onboardingRequest } from "../../api";
import type {
  CompanyDelegateResponse, CompanyDeliveryDelegatePlan, CompanyDeliveryDelegateRequest,
  CompanyManagementDelegatePlan, CompanyManagementDelegateRequest,
} from "./types";

type Kind = "management" | "delivery";
type Review = { kind: Kind; request: CompanyManagementDelegateRequest | CompanyDeliveryDelegateRequest };
const endpoint = (kind: Kind) => kind === "management" ? "/company/delegates" : "/company/delivery-delegates";
function matches(result: CompanyDelegateResponse, { request, kind }: Review) {
  if (!["request_id", "company_id", "owner_instance_id", "area", "issuer", "principal_id", "token_id"].every(
    (key) => result[key as keyof typeof result] === request[key as keyof typeof request])) return false;
  if (!Number.isFinite(Date.parse(result.expires_at)) || typeof result.replayed !== "boolean"
    || typeof result.credential_issued !== "boolean"
    || result.credential_issued !== (typeof result.token === "string" && result.token.length > 0)
    || result.replayed === result.credential_issued
    || (result.token !== null && (typeof result.token !== "string" || !result.token))) return false;
  if (kind === "management" && "delegation_policy_revision" in result && "delegation_policy_revision" in request) {
    return result.delegation_policy_revision === request.delegation_policy_revision
      && result.projects.every((project) => [...project.permissions].sort().join(",") === "auth_manage,memory_read")
      && JSON.stringify(result.projects.map(({ project_id, role_ceiling }) => ({ project_id, role_ceiling }))) === JSON.stringify(request.projects);
  }
  return kind === "delivery" && "delivery_policy_revision" in result && "delivery_policy_revision" in request
    && result.delivery_policy_revision === request.delivery_policy_revision
    && JSON.stringify(result.projects) === JSON.stringify(request.projects)
    && JSON.stringify(result.permissions) === JSON.stringify(["project_scoped_delivery"]);
}

export function CompanyDelegatesPanel({ readOnly = false }: { readOnly?: boolean }) {
  const [management, setManagement] = useState<CompanyManagementDelegatePlan[]>([]);
  const [delivery, setDelivery] = useState<CompanyDeliveryDelegatePlan[]>([]);
  const [name, setName] = useState("");
  const [days, setDays] = useState("90");
  const [review, setReview] = useState<Review | null>(null);
  const [confirmed, setConfirmed] = useState(false);
  const [attempted, setAttempted] = useState(false);
  const [completed, setCompleted] = useState(false);
  const [busy, setBusy] = useState(false);
  const inFlight = useRef(false);
  // Secrets remain exclusively in component memory, never in rendered markup or browser storage.
  const [receipt, setReceipt] = useState<CompanyDelegateResponse | null>(null);
  const [downloaded, setDownloaded] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const refresh = useCallback(async (signal?: AbortSignal) => {
    setBusy(true); setLoadError(null);
    const outcomes = await Promise.allSettled([
      onboardingRequest<{ delegates: CompanyManagementDelegatePlan[] }>(endpoint("management"), undefined, signal),
      onboardingRequest<{ delegates: CompanyDeliveryDelegatePlan[] }>(endpoint("delivery"), undefined, signal),
    ]);
    if (!signal?.aborted) {
      setManagement(outcomes[0].status === "fulfilled" ? outcomes[0].value.delegates ?? [] : []);
      setDelivery(outcomes[1].status === "fulfilled" ? outcomes[1].value.delegates ?? [] : []);
      if (outcomes.some((outcome) => outcome.status === "rejected")) setLoadError("Some configured delegate plans could not be read. Inspect this local owner's configuration and administration access.");
      setBusy(false);
    }
  }, []);
  useEffect(() => { const abort = new AbortController(); void refresh(abort.signal); return () => abort.abort(); }, [refresh]);
  const valid = !readOnly && name.trim().length > 0 && new TextEncoder().encode(name.trim()).length <= 120
    && !/[\x00-\x1f\x7f]/.test(name) && /^\d+$/.test(days) && Number(days) >= 1 && Number(days) <= 90;
  function prepare(kind: Kind, plan: CompanyManagementDelegatePlan | CompanyDeliveryDelegatePlan) {
    if (!valid || busy || review || plan.status !== "unprovisioned") return;
    const identity = { company_id: plan.company_id, owner_instance_id: plan.owner_instance_id, area: plan.area,
      issuer: plan.issuer, principal_id: plan.principal_id, token_id: plan.token_id,
      request_id: crypto.randomUUID(), name: name.trim(), expires_in_days: Number(days), confirmed: true };
    // Whitelist request fields: status, expiry and delivery permissions are read-only metadata.
    const request = "delegation_policy_revision" in plan
      ? { ...identity, delegation_policy_revision: plan.delegation_policy_revision, projects: plan.projects }
      : { ...identity, delivery_policy_revision: plan.delivery_policy_revision, projects: plan.projects };
    setReview({ kind, request });
    setConfirmed(false); setAttempted(false); setCompleted(false); setMessage(null); setError(null);
  }
  async function submit() {
    if (!review || !confirmed || readOnly || completed || receipt || inFlight.current) return;
    inFlight.current = true; setBusy(true); setAttempted(true); setError(null);
    try {
      const result = await onboardingRequest<CompanyDelegateResponse>(endpoint(review.kind), review.request);
      // Preserve an issued credential even if its receipt needs manual reconciliation.
      if (typeof result.token === "string" && result.token.length > 0) setReceipt(result);
      if (!matches(result, review)) throw new Error("receipt mismatch");
      setCompleted(true);
      if (review.kind === "management") setManagement((plans) => plans.map((plan) => plan.token_id === review.request.token_id
        ? { ...plan, status: "active", expires_at: result.expires_at } : plan));
      else setDelivery((plans) => plans.map((plan) => plan.token_id === review.request.token_id
        ? { ...plan, status: "active", expires_at: result.expires_at } : plan));
      if (result.token) setMessage("Credential issued. Download the private JSON receipt now, then confirm it was saved before continuing.");
      else setMessage("No credential was returned. Reconcile the original request ID and owner credential state before configuring the company service; one-time tokens cannot be recovered by replaying a request.");
    } catch {
      setError(`Provisioning outcome unconfirmed. Retain request ${review.request.request_id}; retry only this exact request or reconcile the owner state. Do not create a new request to recover a lost token.`);
    } finally { inFlight.current = false; setBusy(false); }
  }
  function download() {
    if (!receipt || !review) return;
    let url: string | undefined;
    try {
      url = URL.createObjectURL(new Blob([JSON.stringify(receipt, null, 2)], { type: "application/json" }));
      const link = document.createElement("a");
      link.href = url;
      link.download = `company-delegate-${review.request.owner_instance_id}-${review.kind}-${review.request.request_id}.json`;
      link.click();
      setDownloaded(true);
    } catch { setError("Receipt download could not start. The credential is still held in this view; retry saving before leaving."); }
    finally { if (url) { const savedUrl = url; setTimeout(() => URL.revokeObjectURL(savedUrl), 1000); } }
  }
  const locked = busy || !!review || !!receipt;
  return <section className="panel panel-stack" aria-label="Local owner delegate setup">
    <h3>Local owner delegate setup</h3>
    <p>Inspect the exact delegates configured on this owner. Management delegates manage project grants; delivery delegates have only scoped project delivery. Save each private receipt securely for explicit company service configuration.</p>
    <p>Credentials expire after 1–90 days. A successful credential is returned only once. Leaving this view before saving loses it.</p>
    {readOnly ? <p role="status">This service is read-only. Delegate provisioning is disabled.</p> : null}
    <button type="button" disabled={locked} onClick={() => void refresh()}>Refresh configured delegates</button>
    {loadError ? <p role="status">{loadError}</p> : null}
    {error ? <p role="alert">{error}</p> : null}
    {message ? <p role="status">{message}</p> : null}
    <fieldset disabled={locked || readOnly}>
      <legend>Delegate credential details</legend>
      <label>Delegate credential name<input value={name} onChange={(event) => setName(event.target.value)} /></label>
      <label>Delegate lifetime in days<input type="number" min="1" max="90" value={days} onChange={(event) => setDays(event.target.value)} /></label>
    </fieldset>
    {([{ kind: "management" as const, plans: management }, { kind: "delivery" as const, plans: delivery }]).map(({ kind, plans }) => <section key={kind} aria-label={`${kind} delegate plans`}>
      <h4>{kind === "management" ? "Management delegates" : "Delivery delegates"}</h4>
      {!busy && plans.length === 0 ? <p>No {kind} delegate plans available on this owner.</p> : null}
      {plans.map((plan) => <section key={`${plan.company_id}-${plan.token_id}`}>
        <pre>{JSON.stringify(plan, null, 2)}</pre>
        <button type="button" disabled={locked || !valid || plan.status !== "unprovisioned"} onClick={() => prepare(kind, plan)}>Review {kind} delegate {plan.token_id}</button>
      </section>)}
    </section>)}
    {review ? <section aria-label="Reviewed delegate provisioning">
      <h4>Review {review.kind} delegate provisioning</h4>
      <pre>{JSON.stringify(review.request, null, 2)}</pre>
      <label><input type="checkbox" checked={confirmed} disabled={busy || attempted || readOnly} onChange={(event) => setConfirmed(event.target.checked)} />I reviewed this exact owner, company, identity, policy, projects and credential lifetime.</label>
      <button type="button" disabled={!confirmed || readOnly || busy || completed || !!receipt} onClick={() => void submit()}>{attempted ? "Retry exact delegate request" : "Provision reviewed delegate"}</button>
      <button type="button" disabled={busy || !!receipt || (attempted && !completed)} onClick={() => { setReview(null); setConfirmed(false); setAttempted(false); setCompleted(false); }}>Close delegate review</button>
    </section> : null}
    {receipt ? <section aria-label="Private delegate receipt">
      <p>The private receipt contains a service credential. Store it securely; do not share it in chat or logs.</p>
      <button type="button" onClick={download}>Download private delegate receipt</button>
      <button type="button" disabled={!downloaded} onClick={() => { setReceipt(null); setDownloaded(false); setMessage(completed
        ? "Private receipt cleared from this view. Configure the company service using the saved receipt, then refresh owner status."
        : "Private receipt cleared from this view. Reconcile the unconfirmed receipt against the original request before configuring the company service."); }}>I saved the private receipt</button>
    </section> : null}
  </section>;
}
