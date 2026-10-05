import { useCallback, useEffect, useState } from "react";
import { onboardingRequest } from "../../api";
import type { IdpOperation } from "./types";

export function ProviderOperationsPanel({ refreshVersion, operationId }: { refreshVersion: number; operationId: string | null }) {
  const [operations, setOperations] = useState<IdpOperation[]>([]);
  const [detail, setDetail] = useState<IdpOperation | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [detailError, setDetailError] = useState<string | null>(null);
  const [detailRevision, setDetailRevision] = useState(0);
  const refresh = useCallback(async (signal?: AbortSignal) => {
    setLoading(true); setError(null);
    try {
      const result = await onboardingRequest<{ operations: IdpOperation[] }>("/idp/operations", undefined, signal);
      if (!signal?.aborted) { setOperations(result.operations.slice(0, 100)); setDetailRevision((value) => value + 1); }
    } catch (nextError) { if (!signal?.aborted) setError((nextError as Error).message); }
    finally { if (!signal?.aborted) setLoading(false); }
  }, []);
  useEffect(() => { const controller = new AbortController(); void refresh(controller.signal); return () => controller.abort(); }, [refresh, refreshVersion]);
  useEffect(() => { if (operationId) setSelectedId(operationId); }, [operationId]);
  useEffect(() => {
    setDetail(null); setDetailError(null);
    if (!selectedId) return;
    const controller = new AbortController();
    void onboardingRequest<IdpOperation>(`/idp/operations/${encodeURIComponent(selectedId)}`, undefined, controller.signal)
      .then((result) => {
        if (controller.signal.aborted) return;
        if (result.id !== selectedId) throw new Error("Operation receipt mismatch. Inspect the original operation in Authentik.");
        setDetail(result);
      })
      .catch((nextError: Error) => { if (!controller.signal.aborted) setDetailError(nextError.message); });
    return () => controller.abort();
  }, [selectedId, detailRevision]);

  return <section className="panel access-table-section"><div className="access-heading"><div><h3>Provider operations</h3>
    <p>The latest 100 metadata records survive page reloads. Pending or uncertain means the provider effect is unconfirmed. Inspect the operation and reconcile it in Authentik before submitting again; another submission may duplicate users or invitations. Refresh only reads metadata and never resubmits an action.</p></div>
    <button type="button" disabled={loading} onClick={() => void refresh()}>Refresh provider operations</button></div>
    {error ? <p role="alert">Provider operation inventory unavailable: {error}</p> : null}
    <div className="table-scroll"><table><thead><tr><th>Operation ID</th><th>Action</th><th>Outcome</th><th>Provider user</th><th>Invitation</th><th /></tr></thead>
      <tbody>{operations.map((operation) => <tr key={operation.id}><td><code>{operation.id}</code></td><td>{operation.operation}</td>
        <td>{operation.outcome}{operation.outcome === "pending" || operation.outcome === "uncertain" ? " — inspect before resubmitting" : ""}</td>
        <td>{operation.provider_user_id ?? "not recorded"}</td><td>{operation.invitation_id ?? "not recorded"}</td>
        <td><button type="button" onClick={() => setSelectedId(operation.id)}>Inspect operation {operation.id}</button></td></tr>)}</tbody>
    </table></div>
    {!loading && !error && !operations.length ? <p>No recorded provider operations.</p> : null}
    {selectedId ? <section aria-label="Provider operation inspection"><h4>Operation {selectedId}</h4>
      {detailError ? <p role="alert">Operation inspection failed: {detailError}. Keep this operation ID and inspect the provider before resubmitting.</p> : null}
      {!detail && !detailError ? <p>Loading operation metadata…</p> : null}
      {detail ? <><p>Action: {detail.operation}. Outcome: {detail.outcome}. Provider user: {detail.provider_user_id ?? "not recorded"}. Invitation: {detail.invitation_id ?? "not recorded"}.</p>
        <p>Provider origin: {detail.provider_origin ?? "not recorded"}. Correlation: {detail.provider_correlation ?? "not recorded"}. Request digest: {detail.request_sha256 ?? "not recorded"}.</p></> : null}
    </section> : null}
  </section>;
}
