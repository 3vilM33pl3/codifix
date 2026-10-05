import { useCallback, useEffect, useState } from "react";
import { onboardingRequest } from "../../api";
import type { EffectiveUserAccess, IdpOperation, ManagedUser, OnboardingCapabilities } from "./types";

export function UsersPanel({ readOnly, onChanged, accessVersion = 0 }: { readOnly: boolean; onChanged: () => void; accessVersion?: number }) {
  const [users, setUsers] = useState<ManagedUser[]>([]);
  const [capabilities, setCapabilities] = useState<OnboardingCapabilities | null>(null);
  const [selected, setSelected] = useState("");
  const [access, setAccess] = useState<EffectiveUserAccess | null>(null);
  const [accessError, setAccessError] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [issuer, setIssuer] = useState("");
  const [subject, setSubject] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [email, setEmail] = useState("");
  const [username, setUsername] = useState("");
  const [providerName, setProviderName] = useState("");
  const [providerEmail, setProviderEmail] = useState("");
  const [providerId, setProviderId] = useState("");
  const [confirmDisable, setConfirmDisable] = useState(false);
  const [confirmProviderDisable, setConfirmProviderDisable] = useState(false);
  const [accessRevision, setAccessRevision] = useState(0);

  const refresh = useCallback(async (signal?: AbortSignal) => {
    setLoading(true);
    setError(null);
    try {
      const [inventory, configured] = await Promise.all([
        onboardingRequest<{ users: ManagedUser[] }>("/users", undefined, signal),
        onboardingRequest<OnboardingCapabilities>("/capabilities", undefined, signal),
      ]);
      if (signal?.aborted) return;
      setUsers(inventory.users);
      setCapabilities(configured);
      setSelected((current) => inventory.users.some((user) => user.principal.id === current)
        ? current : inventory.users[0]?.principal.id ?? "");
      setAccessRevision((current) => current + 1);
    } catch (nextError) {
      if (!signal?.aborted) { setError((nextError as Error).message); setCapabilities(null); }
    } finally { if (!signal?.aborted) setLoading(false); }
  }, []);

  useEffect(() => {
    const controller = new AbortController();
    void refresh(controller.signal);
    return () => controller.abort();
  }, [refresh]);

  useEffect(() => {
    setAccess(null);
    setAccessError(null);
    setConfirmDisable(false);
    if (!selected) return;
    const controller = new AbortController();
    void onboardingRequest<EffectiveUserAccess>(`/users/${encodeURIComponent(selected)}/access`, undefined, controller.signal)
      .then((value) => { if (!controller.signal.aborted) setAccess(value); })
      .catch((nextError: Error) => { if (!controller.signal.aborted) setAccessError(nextError.message); });
    return () => controller.abort();
  }, [selected, accessRevision, accessVersion]);

  async function mutate(suffix: string, body: unknown, message: string, provider = false) {
    if (readOnly || busy || loading) return;
    setBusy(true); setError(null); setStatus(null);
    try {
      const result = await onboardingRequest<IdpOperation | ManagedUser>(suffix, body);
      if (provider && "outcome" in result) {
        setStatus(`${message}: ${result.outcome}. Operation ${result.id}${result.provider_user_id != null
          ? `; provider user ${result.provider_user_id}` : ""}${result.invitation_id ? `; invitation ${result.invitation_id}` : ""}.${result.outcome === "pending" ? " Inspect this operation in provider administration before retrying." : ""}`);
      } else { setStatus(message); }
      await refresh();
      onChanged();
    } catch (nextError) { setError((nextError as Error).message); }
    finally { setBusy(false); setConfirmDisable(false); setConfirmProviderDisable(false); }
  }

  const selectedUser = users.find((user) => user.principal.id === selected);
  const disabled = readOnly || loading || busy;
  const providerDisabled = disabled || !capabilities?.idp_management_enabled;
  const providerPayload = { username: username.trim(), display_name: providerName.trim(), email: providerEmail.trim() };
  const providerValid = Boolean(providerPayload.username && providerPayload.display_name && providerPayload.email);

  return <div className="panel-stack">
    <section className="panel access-heading"><div><h3>Users</h3>
      <p>Register a verified issuer and subject before first login, then grant access using the membership controls below. Email is a contact field.</p></div>
      <button type="button" disabled={loading || busy} onClick={() => void refresh()}>Refresh users</button>
    </section>
    {error ? <div className="panel error-banner" role="alert">Users error: {error}</div> : null}
    {status ? <div className="panel" role="status">{status}</div> : null}
    <section className="panel access-admin-form">
      <h3>Register stable login identity</h3>
      <label>Verified OIDC issuer<input value={issuer} onChange={(event) => setIssuer(event.target.value)} /></label>
      <label>Verified OIDC subject<input value={subject} onChange={(event) => setSubject(event.target.value)} /></label>
      <label>User display name<input value={displayName} onChange={(event) => setDisplayName(event.target.value)} /></label>
      <label>User contact email<input type="email" value={email} onChange={(event) => setEmail(event.target.value)} /></label>
      <button type="button" disabled={disabled || !issuer.trim() || !subject.trim() || !displayName.trim()}
        onClick={() => void mutate("/users", { issuer: issuer.trim(), subject: subject.trim(), display_name: displayName.trim(), ...(email.trim() ? { email: email.trim() } : {}) }, "Stable identity registered; review its project memberships.")}>Register identity</button>
    </section>
    <section className="panel access-table-section"><h3>Managed users</h3><div className="table-scroll"><table>
      <thead><tr><th>Name</th><th>Contact</th><th>Stable identity</th><th>Status</th><th /></tr></thead>
      <tbody>{users.map((user) => <tr key={user.principal.id}>
        <td>{user.principal.display_name}</td><td>{user.principal.email ?? "-"}</td><td><code>{user.principal.id}</code><br />{user.principal.issuer ?? "issuer not reported"}<br />{user.principal.subject ?? "subject not reported"}</td>
        <td>{user.disabled_at ? "disabled" : "enabled"}</td><td><button type="button" onClick={() => setSelected(user.principal.id)}>Inspect {user.principal.display_name}</button></td>
      </tr>)}</tbody>
    </table></div>{!loading && !users.length ? <p>No registered users.</p> : null}</section>
    {selectedUser ? <section className="panel access-table-section"><h3>Effective access: {selectedUser.principal.display_name}</h3>
      <p>Direct grants and configured group grants are resolved by the central service. Global permissions: {access?.global_permissions.join(", ") || "none reported"}.</p>
      {accessError ? <p role="alert">Access inspection failed: {accessError}</p> : null}
      {!access && !accessError ? <p>Loading effective access…</p> : null}
      {access ? <><p>Access state: {access.disabled ? "disabled" : "enabled"}</p>
        <div className="table-scroll"><table><thead><tr><th>Project</th><th>Permissions</th><th>Grant sources</th></tr></thead>
          <tbody>{access.projects.map((entry) => <tr key={entry.project}><td>{entry.project}</td><td>{entry.permissions.join(", ") || "none"}</td>
            <td>{entry.grants.map((grant) => `${grant.source}: ${grant.role}${grant.group ? ` (${grant.group})` : ""}`).join(", ") || "none"}</td></tr>)}</tbody>
        </table></div>
        <h4>Configured group rules</h4><ul>{access.group_rules.map((rule, index) => <li key={index}>
          {rule.group}: {rule.role} · {rule.global ? "global" : rule.project ?? "no project"} · {rule.matched ? "matched" : "not matched"}
        </li>)}</ul></> : null}
      <label className="access-checkbox"><input type="checkbox" checked={confirmDisable} disabled={disabled || Boolean(selectedUser.disabled_at)} onChange={(event) => setConfirmDisable(event.target.checked)} />Confirm disabling this Memory identity, its sessions and linked devices</label>
      <button type="button" disabled={disabled || !confirmDisable || Boolean(selectedUser.disabled_at)} onClick={() => void mutate(`/users/${encodeURIComponent(selected)}/disable`, {}, "Memory identity disabled.")}>Disable Memory user</button>
    </section> : null}
    <section className="panel access-admin-form"><h3>Identity provider administration</h3>
      <p>{capabilities?.idp_management_enabled ? "Authentik management is enabled." : "Identity provider management is not enabled on this service."} Provider operations do not assign Memory access. Verify the provider's configured OIDC subject before registering an identity.</p>
      <label>Provider username<input value={username} onChange={(event) => setUsername(event.target.value)} /></label>
      <label>Provider display name<input value={providerName} onChange={(event) => setProviderName(event.target.value)} /></label>
      <label>Provider email<input type="email" value={providerEmail} onChange={(event) => setProviderEmail(event.target.value)} /></label>
      <div className="actions-row"><button type="button" disabled={providerDisabled || !providerValid} onClick={() => void mutate("/idp/users", providerPayload, "Provider user creation", true)}>Create provider user</button>
        <button type="button" disabled={providerDisabled || !capabilities?.invitation_enabled || !providerValid} onClick={() => void mutate("/users/invite", providerPayload, "Provider invitation creation", true)}>Create provider invitation</button></div>
      <p>Invitations require a configured provider flow. This action creates an invitation; delivery is handled separately in Authentik.</p>
      <label>Verified provider user ID<input inputMode="numeric" value={providerId} onChange={(event) => { setProviderId(event.target.value); setConfirmProviderDisable(false); }} /></label>
      <label className="access-checkbox"><input type="checkbox" checked={confirmProviderDisable} disabled={providerDisabled} onChange={(event) => setConfirmProviderDisable(event.target.checked)} />Confirm disabling this provider user</label>
      <button type="button" disabled={providerDisabled || !/^\d+$/.test(providerId) || !confirmProviderDisable} onClick={() => void mutate(`/idp/users/${encodeURIComponent(providerId)}/disable`, {}, "Provider user disable", true)}>Disable provider user</button>
    </section>
  </div>;
}
