import { useCallback, useEffect, useState } from "react";
import { onboardingRequest } from "../../api";
import type { CompanyConfiguration, CompanyGrantRequest, CompanyOperation } from "./types";

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const NIL = "00000000-0000-0000-0000-000000000000";
function sameRequest(left: CompanyGrantRequest, right: CompanyGrantRequest): boolean {
  return (Object.keys(left) as (keyof CompanyGrantRequest)[]).every((key) => left[key] === right[key]);
}
function verified(result: CompanyOperation, request: CompanyGrantRequest): CompanyOperation {
  if (!result?.request || !sameRequest(request, result.request) || !/^[0-9a-f]{64}$/.test(result.request_sha256)
    || !["pending", "uncertain", "confirmed", "rejected"].includes(result.outcome)
    || (result.receipt && (!sameRequest(request, result.receipt.request)
      || result.receipt.request_sha256 !== result.request_sha256))) {
    throw new Error("Company receipt mismatch. Keep the original operation ID and inspect its journal.");
  }
  return result;
}

export function CompanyPanel({ readOnly = false }: { readOnly?: boolean }) {
  const [configuration, setConfiguration] = useState<CompanyConfiguration | null>(null);
  const [operations, setOperations] = useState<CompanyOperation[]>([]);
  const [ownerId, setOwnerId] = useState("");
  const [projectId, setProjectId] = useState("");
  const [subject, setSubject] = useState("");
  const [role, setRole] = useState<CompanyGrantRequest["role"]>("reader");
  const [action, setAction] = useState<CompanyGrantRequest["operation"]>("grant");
  const [revision, setRevision] = useState("0");
  const [review, setReview] = useState<CompanyGrantRequest | null>(null);
  const [confirmed, setConfirmed] = useState(false);
  const [attempted, setAttempted] = useState(false);
  const [selected, setSelected] = useState<CompanyOperation | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const refresh = useCallback(async (signal?: AbortSignal) => {
    try {
      const [config, history] = await Promise.all([
        onboardingRequest<CompanyConfiguration>("/company/configuration", undefined, signal),
        onboardingRequest<{ operations: CompanyOperation[] }>("/company/operations", undefined, signal),
      ]);
      if (!signal?.aborted) {
        setConfiguration(config); setOperations(history.operations);
        setOwnerId((current) => current || config.owners.find((owner) => owner.grant_management)?.owner_instance_id || "");
        // Historical journal reads never refresh a previously observed grant.
        setSelected(null);
      }
    } catch (reason) { if (!signal?.aborted) setError((reason as Error).message); }
  }, []);
  useEffect(() => { const abort = new AbortController(); void refresh(abort.signal); return () => abort.abort(); }, [refresh]);
  const owner = configuration?.owners.find((item) => item.owner_instance_id === ownerId);
  const canWrite = !readOnly && configuration?.writes_enabled === true;
  const valid = canWrite && owner?.grant_management && UUID.test(projectId) && projectId !== NIL
    && subject.trim().length > 0 && subject.trim().length <= 512
    && !/[\x00-\x1f\x7f]/.test(subject) && /^\d+$/.test(revision)
    && Number.isSafeInteger(Number(revision)) && Number(revision) < Number.MAX_SAFE_INTEGER;
  function prepare() {
    if (!valid || !owner || !configuration?.company_id || review) return;
    setError(null); setConfirmed(false); setAttempted(false);
    setReview({ company_id: configuration.company_id, operation_id: crypto.randomUUID(),
      owner_instance_id: owner.owner_instance_id, area: owner.area, project_id: projectId,
      issuer: owner.issuer, subject: subject.trim(), operation: action, role,
      expected_grant_revision: Number(revision), delegation_policy_revision: owner.delegation_policy_revision });
  }
  async function submit() {
    if (!review || !confirmed || !canWrite || busy) return;
    setBusy(true); setError(null); setAttempted(true);
    try {
      const result = verified(await onboardingRequest<CompanyOperation>("/company/operations", review), review);
      setSelected(result);
      setOperations((current) => [result, ...current.filter((item) => item.request.operation_id !== review.operation_id)]);
    } catch (reason) { setError(`${(reason as Error).message} Operation ${review.operation_id}: do not submit changed terms under a new ID until this outcome is resolved.`); }
    finally { setBusy(false); }
  }
  async function reconcile(operation: CompanyOperation) {
    if (!canWrite || busy) return;
    setBusy(true); setError(null);
    try {
      const result = verified(await onboardingRequest<CompanyOperation>(`/company/operations/${operation.request.operation_id}/reconcile`, {}), operation.request);
      setSelected(result);
      setOperations((current) => current.map((item) => item.request.operation_id === result.request.operation_id ? result : item));
    } catch (reason) { setError(`${(reason as Error).message} Original operation ${operation.request.operation_id} retained.`); }
    finally { setBusy(false); }
  }
  return <section className="panel panel-stack">
    <h3>Cross-area project access</h3>
    <p>Use a verified identity-provider subject, not an email address. Owners enforce their exact project and role ceilings. Project access does not grant federation capabilities or model disclosure.</p>
    <button type="button" disabled={busy} onClick={() => void refresh()}>Refresh company operations</button>
    {!configuration?.company_id ? <p role="status">Company owner delegation is not configured on this service.</p> : null}
    {!canWrite ? <p role="status">Company mutations are unavailable or this service is read-only.</p> : null}
    {error ? <p role="alert">{error}</p> : null}
    <fieldset disabled={busy || !!review || !canWrite}>
      <legend>Prepare an owner-scoped grant</legend>
      <label>Owning area<select value={ownerId} onChange={(event) => setOwnerId(event.target.value)}>
        {configuration?.owners.map((item) => <option key={item.owner_instance_id} value={item.owner_instance_id} disabled={!item.grant_management}>
          {item.area} · {item.owner_instance_id}{!item.grant_management ? " · remote expertise only" : ""}
        </option>)}
      </select></label>
      <label>Exact project ID<input value={projectId} onChange={(event) => setProjectId(event.target.value)} /></label>
      <label>Verified user subject<input value={subject} onChange={(event) => setSubject(event.target.value)} /></label>
      <label>Direct grant action<select value={action} onChange={(event) => setAction(event.target.value as CompanyGrantRequest["operation"])}><option value="grant">Grant</option><option value="revoke">Revoke direct grant</option></select></label>
      <label>Owner project role<select value={role} onChange={(event) => setRole(event.target.value as CompanyGrantRequest["role"])}>{["reader", "writer", "operator"].map((value) => <option key={value}>{value}</option>)}</select></label>
      <label>Expected owner grant revision<input value={revision} inputMode="numeric" onChange={(event) => setRevision(event.target.value)} /></label>
      <p>Issuer: {owner?.issuer || "not selected"}. Revision 0 is only for a new company-owned grant; inspect existing ownership before changing it.</p>
      <button type="button" disabled={!valid} onClick={prepare}>Review company grant</button>
    </fieldset>
    {review ? <section aria-label="Reviewed company grant">
      <pre>{JSON.stringify(review, null, 2)}</pre>
      <p>Revoking a direct grant does not remove independent group-derived access. Trust remains remote-only.</p>
      <label><input type="checkbox" checked={confirmed} disabled={busy || attempted} onChange={(event) => setConfirmed(event.target.checked)} />I reviewed this exact owner, project, stable subject, role and revision.</label>
      <button type="button" disabled={!confirmed || busy || !canWrite || selected?.request.operation_id === review.operation_id} onClick={() => void submit()}>{attempted ? "Retry same company request" : "Submit reviewed company grant"}</button>
      <button type="button" disabled={busy || (attempted && selected?.outcome !== "confirmed" && selected?.outcome !== "rejected")} onClick={() => { setReview(null); setConfirmed(false); setAttempted(false); }}>Close local review</button>
    </section> : null}
    <h4>Durable operation history</h4>
    <p>Receipts are historical. Reconcile to observe current owner rights; it retries only the original operation if the owner confirms it is absent.</p>
    {operations.map((item) => <section key={item.request.operation_id}>
      <p>{item.request.area} · project {item.request.project_id} · {item.outcome} · operation {item.request.operation_id}</p>
      <p>Subject {item.request.subject}; direct {item.request.operation} {item.request.role}. Historical grant revision: {item.receipt?.grant_revision ?? "unconfirmed"}.</p>
      <button type="button" disabled={busy || !canWrite} onClick={() => void reconcile(item)}>Reconcile {item.request.operation_id}</button>
    </section>)}
    {selected ? <section role="status">
      <h4>Operation {selected.request.operation_id}: {selected.outcome}</h4>
      {selected.current_access ? <>
        <p>Owner rights observed at {selected.current_access.observed_at}. {selected.current_access.disabled ? "User disabled." : "User enabled."}</p>
        <p>Direct role: {selected.current_access.direct_role ?? "none"}; group roles: {selected.current_access.group_roles.join(", ") || "none"}.</p>
        <p>Effective permissions: {selected.current_access.permissions.join(", ") || "none"}.</p>
      </> : <p>Current effective access is unknown. A historical direct-grant receipt does not prove current access.</p>}
    </section> : null}
  </section>;
}
