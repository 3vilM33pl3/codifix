import { useEffect, useState } from "react";
import { supervisorPolicy, type SupervisorPolicyRequest } from "../../api";
import type { SupervisorResponse } from "./types";

export function SupervisorControls({ data, onInspect, onPendingChange }: { data: SupervisorResponse; onInspect: () => void; onPendingChange: (value: boolean) => void }) {
  const [request, setRequest] = useState<SupervisorPolicyRequest | null>(null);
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState("");
  useEffect(() => {
    onPendingChange(request !== null);
    return () => onPendingChange(false);
  }, [request, onPendingChange]);
  const snapshot = data.snapshot;
  if (!data.can_control || data.read_only || !snapshot?.policy || snapshot.policy.revoked) return null;
  function prepare() {
    if (!snapshot?.policy) return;
    setFailure("");
    setRequest({ request_id: crypto.randomUUID(), instance_id: snapshot.instance_id,
      policy_revision: snapshot.policy.revision, expected_enabled: snapshot.policy.enabled, enabled: !snapshot.policy.enabled });
  }
  async function submit() {
    if (!request || busy) return;
    setBusy(true); setFailure("");
    try { await supervisorPolicy(request); onInspect(); }
    catch (error) { setFailure(error instanceof Error ? error.message : "Outcome unconfirmed; retry the same request ID."); }
    finally { setBusy(false); }
  }
  return <section aria-label="Supervisor controls">
    {!request ? <button onClick={prepare}>{snapshot.policy.enabled ? "Pause supervisor" : "Resume supervisor"}</button> :
      <div role="dialog" aria-label="Confirm supervisor policy change" aria-busy={busy}>
        <p>{request.enabled ? "Resume" : "Pause"} {snapshot.area} on instance {request.instance_id}?</p>
        <p>Exact policy {request.policy_revision}; request {request.request_id}.</p>
        <p>Existing policy only; no new sharing/model consent or installation. Resuming permits an already configured worker to run its eligible approved work, including a paid proposal. Pausing blocks future admissions and results; already transmitted bytes cannot be recalled.</p>
        {failure ? <p role="alert">{failure}</p> : null}
        <button disabled={busy} onClick={() => void submit()}>{busy ? "Submitting…" : failure ? "Retry same request" : "Confirm policy change"}</button>
        <button disabled={busy} onClick={() => { setRequest(null); onInspect(); }}>{failure ? "Close and inspect history" : "Cancel"}</button>
      </div>}
  </section>;
}
