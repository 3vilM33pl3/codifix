import { useCallback, useEffect, useState } from "react";
import { onboardingRequest } from "../../api";
import type { ManagedDevice } from "./types";
import { CompanyAgentsPanel } from "./CompanyAgentsPanel";

export function DevicesPanel({ readOnly, administrationAllowed = true, initialAgentRequest }: { readOnly: boolean; administrationAllowed?: boolean; initialAgentRequest?: string }) {
  return <div className="panel-stack"><CompanyAgentsPanel readOnly={readOnly} initialRequest={initialAgentRequest} />
    {administrationAllowed ? <AdminDevicesPanel readOnly={readOnly} /> : <section className="panel"><h3>Own device access</h3><p>Import the public native envelope to inspect only its exact owned device. Global administration inventories and unrelated device controls are unavailable here.</p></section>}
  </div>;
}

function AdminDevicesPanel({ readOnly }: { readOnly: boolean }) {
  const [devices, setDevices] = useState<ManagedDevice[]>([]);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<ManagedDevice | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const refresh = useCallback(async (signal?: AbortSignal) => {
    setLoading(true); setError(null);
    try {
      const result = await onboardingRequest<{ devices: ManagedDevice[] }>("/devices", undefined, signal);
      if (!signal?.aborted) setDevices(result.devices);
    } catch (nextError) { if (!signal?.aborted) setError((nextError as Error).message); }
    finally { if (!signal?.aborted) setLoading(false); }
  }, []);
  useEffect(() => { const controller = new AbortController(); void refresh(controller.signal); return () => controller.abort(); }, [refresh]);
  async function revoke() {
    if (readOnly || loading || busy || !selected || selected.revoked_at) return;
    setBusy(true); setError(null); setStatus(null);
    try {
      await onboardingRequest<ManagedDevice>(`/devices/${encodeURIComponent(selected.id)}/revoke`, {});
      setSelected(null); setStatus("Device credential revoked. Other devices and the user's project memberships are independent.");
      await refresh();
    } catch (nextError) { setError((nextError as Error).message); }
    finally { setBusy(false); }
  }
  return <div className="panel-stack">
    <section className="panel access-heading"><div><h3>Devices</h3><p>Inspect enrolled device credentials and revoke a device independently. Last seen is an observation; device health is unknown. Browser approval requires a current human session; Mac client login, local credential storage and selective sync require separate client implementation.</p>
      <a className="button-link" href="/?device_code=">Review a device request</a></div>
      <button type="button" disabled={busy || loading} onClick={() => { setSelected(null); void refresh(); }}>Refresh devices</button></section>
    {error ? <div className="panel error-banner" role="alert">Devices error: {error}</div> : null}
    {status ? <div className="panel" role="status">{status}</div> : null}
    <section className="panel access-table-section"><div className="table-scroll"><table>
      <thead><tr><th>Device</th><th>Principal</th><th>Platform</th><th>Last seen</th><th>Credential</th><th>Health</th><th /></tr></thead>
      <tbody>{devices.map((device) => <tr key={device.id}><td>{device.name}<br /><code>{device.id}</code></td><td><code>{device.principal_id}</code></td><td>{device.platform}</td>
        <td>{device.last_seen_at ?? "never observed"}</td><td>{device.revoked_at ? "revoked" : "not revoked"}</td><td>unknown</td>
        <td><button type="button" disabled={readOnly || loading || busy || Boolean(device.revoked_at)} onClick={() => setSelected(device)}>Review revoke {device.name}</button></td></tr>)}</tbody>
    </table></div>{!loading && !devices.length ? <p>No enrolled devices. This does not establish device health.</p> : null}</section>
    {selected ? <section className="panel" aria-label="Device revocation review"><h3>Revoke device credential</h3><p>{selected.name} · {selected.id} · principal {selected.principal_id}</p>
      <button type="button" disabled={readOnly || loading || busy} onClick={() => void revoke()}>Confirm device revocation</button>
      <button type="button" disabled={busy} onClick={() => setSelected(null)}>Cancel</button></section> : null}
  </div>;
}
