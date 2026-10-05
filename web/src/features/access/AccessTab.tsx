import { useCallback, useEffect, useMemo, useState } from "react";

import {
  createAuthToken,
  grantAuthMembership,
  listAuthMemberships,
  listAuthPrincipals,
  listAuthTokens,
  revokeAuthMembership,
  revokeAuthToken,
} from "../../api";
import type {
  AuthMembershipResponse,
  AuthPrincipalResponse,
  AuthRole,
  AuthServiceTokenResponse,
} from "../../types";
import { DevicesPanel } from "./DevicesPanel";
import { ProjectsPanel } from "./ProjectsPanel";
import { UsersPanel } from "./UsersPanel";
import { CompanyPanel } from "./CompanyPanel";

const ROLES: AuthRole[] = ["reader", "writer", "operator", "admin"];
const SERVICE_ROLES: AuthRole[] = ["reader", "writer", "operator"];

interface AccessTabProps {
  project: string;
  readOnly?: boolean;
}

export function AccessTab({ project, readOnly = false }: AccessTabProps) {
  const [view, setView] = useState<"users" | "projects" | "devices" | "company">("users");
  const [refreshVersion, setRefreshVersion] = useState(0);
  const [accessVersion, setAccessVersion] = useState(0);
  const refreshLegacy = useCallback(() => setRefreshVersion((value) => value + 1), []);
  const refreshUserAccess = useCallback(() => setAccessVersion((value) => value + 1), []);
  return <main className="panel-stack access-tab">
    <section className="panel"><h2>Company administration</h2>
      <p>Users, onboarding projects and device credentials. Company grants use explicitly enrolled owner delegates; device approval is still owner-local.</p>
      {readOnly ? <p role="status">This service is read-only. Administration changes are disabled.</p> : null}
      <nav className="actions-row" aria-label="Company administration views">
        {(["users", "projects", "devices", "company"] as const).map((name) => <button type="button" key={name} aria-pressed={view === name} onClick={() => setView(name)}>
          {name[0].toUpperCase() + name.slice(1)}
        </button>)}
      </nav>
    </section>
    {view === "users" ? <UsersPanel readOnly={readOnly} onChanged={refreshLegacy} accessVersion={accessVersion} /> : null}
    <div hidden={view !== "users"}>
      <LegacyAccessControls project={project} readOnly={readOnly} refreshVersion={refreshVersion} onMembershipChanged={refreshUserAccess} />
    </div>
    {view === "projects" ? <ProjectsPanel readOnly={readOnly} /> : null}
    {view === "devices" ? <DevicesPanel readOnly={readOnly} /> : null}
    {view === "company" ? <CompanyPanel readOnly={readOnly} /> : null}
  </main>;
}

function LegacyAccessControls({ project, readOnly, refreshVersion, onMembershipChanged }: AccessTabProps & { refreshVersion: number; onMembershipChanged: () => void }) {
  const [principals, setPrincipals] = useState<AuthPrincipalResponse[]>([]);
  const [tokens, setTokens] = useState<AuthServiceTokenResponse[]>([]);
  const [memberships, setMemberships] = useState<AuthMembershipResponse[]>([]);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [issuedToken, setIssuedToken] = useState<string | null>(null);
  const [tokenName, setTokenName] = useState("");
  const [tokenProject, setTokenProject] = useState(project);
  const [tokenRole, setTokenRole] = useState<AuthRole>("reader");
  const [membershipPrincipal, setMembershipPrincipal] = useState("");
  const [membershipProject, setMembershipProject] = useState(project);
  const [membershipRole, setMembershipRole] = useState<AuthRole>("reader");

  const principalNames = useMemo(
    () => new Map(principals.map((principal) => [principal.id, principal.display_name])),
    [principals],
  );
  const selectedMembershipPrincipal = principals.find(
    (principal) => principal.id === membershipPrincipal,
  );

  const refresh = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [nextPrincipals, nextTokens, nextMemberships] = await Promise.all([
        listAuthPrincipals(),
        listAuthTokens(),
        listAuthMemberships(),
      ]);
      setPrincipals(nextPrincipals);
      setTokens(nextTokens);
      setMemberships(nextMemberships);
      setMembershipPrincipal((current) => current || nextPrincipals[0]?.id || "");
    } catch (nextError) {
      setError((nextError as Error).message);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh, refreshVersion]);

  useEffect(() => {
    if (selectedMembershipPrincipal?.kind === "service_token" && membershipRole === "admin") {
      setMembershipRole("operator");
    }
  }, [membershipRole, selectedMembershipPrincipal?.kind]);

  async function createToken() {
    if (readOnly || !tokenName.trim() || !tokenProject.trim()) return;
    setBusy(true);
    setError(null);
    try {
      const created = await createAuthToken({
        name: tokenName.trim(),
        project: tokenProject.trim(),
        role: tokenRole,
      });
      setIssuedToken(created.token ?? null);
      setTokenName("");
      await refresh();
    } catch (nextError) {
      setError((nextError as Error).message);
    } finally {
      setBusy(false);
    }
  }

  async function grantMembership() {
    if (readOnly || !membershipPrincipal || !membershipProject.trim()) return;
    setBusy(true);
    setError(null);
    try {
      await grantAuthMembership({
        principal_id: membershipPrincipal,
        project: membershipProject.trim(),
        role: membershipRole,
      });
      await refresh();
      onMembershipChanged();
    } catch (nextError) {
      setError((nextError as Error).message);
    } finally {
      setBusy(false);
    }
  }

  async function revokeToken(selector: string) {
    if (readOnly) return;
    setBusy(true);
    try {
      await revokeAuthToken(selector);
      await refresh();
    } catch (nextError) {
      setError((nextError as Error).message);
    } finally {
      setBusy(false);
    }
  }

  async function revokeMembership(id: string) {
    if (readOnly) return;
    setBusy(true);
    try {
      await revokeAuthMembership(id);
      await refresh();
      onMembershipChanged();
    } catch (nextError) {
      setError((nextError as Error).message);
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="panel-stack">
      <section className="panel panel-toolbar access-heading">
        <div>
          <h3>Memberships and service tokens</h3>
          <p>Authentik users, service credentials, and project-scoped roles.</p>
        </div>
        <button type="button" onClick={() => void refresh()} disabled={loading || busy}>
          Refresh
        </button>
      </section>

      {error ? <div className="panel error-banner">Access error: {error}</div> : null}
      {issuedToken ? (
        <section className="panel one-time-secret" role="status">
          <div>
            <strong>Service token, shown once</strong>
            <p>Store this value in OpenBao or your client secret store before dismissing it.</p>
          </div>
          <code>{issuedToken}</code>
          <button type="button" onClick={() => setIssuedToken(null)}>Dismiss</button>
        </section>
      ) : null}

      <section className="panel access-form-row">
        <div>
          <h3>Create service token</h3>
          <p>The raw token is never retained by Memory Layer.</p>
        </div>
        <label>Name<input value={tokenName} onChange={(event) => setTokenName(event.target.value)} /></label>
        <label>Project<input value={tokenProject} onChange={(event) => setTokenProject(event.target.value)} /></label>
        <RoleSelect value={tokenRole} roles={SERVICE_ROLES} onChange={setTokenRole} />
        <button type="button" disabled={readOnly || busy || !tokenName.trim() || !tokenProject.trim()} onClick={() => void createToken()}>
          Create
        </button>
      </section>

      <section className="panel access-form-row">
        <div>
          <h3>Grant project access</h3>
          <p>Explicit grants combine with configured Authentik group mappings.</p>
        </div>
        <label>
          Principal
          <select value={membershipPrincipal} onChange={(event) => setMembershipPrincipal(event.target.value)}>
            {principals.map((principal) => <option key={principal.id} value={principal.id}>{principal.display_name}</option>)}
          </select>
        </label>
        <label>Project<input value={membershipProject} onChange={(event) => setMembershipProject(event.target.value)} /></label>
        <RoleSelect
          value={membershipRole}
          roles={selectedMembershipPrincipal?.kind === "service_token" ? SERVICE_ROLES : ROLES}
          onChange={setMembershipRole}
        />
        <button type="button" disabled={readOnly || busy || !membershipPrincipal || !membershipProject.trim()} onClick={() => void grantMembership()}>
          Grant
        </button>
      </section>

      <section className="panel access-table-section">
        <h3>Principals</h3>
        <div className="table-scroll">
          <table>
            <thead><tr><th>Name</th><th>Kind</th><th>Global role</th><th>Projects</th><th>Email</th></tr></thead>
            <tbody>
              {principals.map((principal) => (
                <tr key={principal.id}>
                  <td>{principal.display_name}</td><td>{principal.kind}</td><td>{principal.global_role ?? "-"}</td>
                  <td>{principal.projects.map((entry) => `${entry.project}:${entry.role}`).join(", ") || "-"}</td>
                  <td>{principal.email ?? "-"}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </section>

      <section className="panel access-table-section">
        <h3>Service tokens</h3>
        <div className="table-scroll">
          <table>
            <thead><tr><th>Name</th><th>Prefix</th><th>Projects</th><th>Last used</th><th>Status</th><th /></tr></thead>
            <tbody>
              {tokens.map((token) => (
                <tr key={token.id}>
                  <td>{token.name}</td><td><code>{token.token_prefix}</code></td>
                  <td>{token.projects.map((entry) => `${entry.project}:${entry.role}`).join(", ")}</td>
                  <td>{formatDate(token.last_used_at)}</td><td>{token.revoked_at ? "revoked" : "active"}</td>
                  <td><button type="button" disabled={readOnly || busy || Boolean(token.revoked_at)} onClick={() => void revokeToken(token.id)}>Revoke</button></td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </section>

      <section className="panel access-table-section">
        <h3>Explicit memberships</h3>
        <div className="table-scroll">
          <table>
            <thead><tr><th>Principal</th><th>Project</th><th>Role</th><th>Source</th><th /></tr></thead>
            <tbody>
              {memberships.map((membership) => (
                <tr key={membership.id}>
                  <td>{principalNames.get(membership.principal_id) ?? membership.principal_id}</td>
                  <td>{membership.project}</td><td>{membership.role}</td><td>{membership.source}</td>
                  <td><button type="button" disabled={readOnly || busy} onClick={() => void revokeMembership(membership.id)}>Revoke</button></td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </section>
    </div>
  );
}

function RoleSelect({ value, roles, onChange }: { value: AuthRole; roles: AuthRole[]; onChange: (role: AuthRole) => void }) {
  return (
    <label>
      Role
      <select value={value} onChange={(event) => onChange(event.target.value as AuthRole)}>
        {roles.map((role) => <option key={role} value={role}>{role}</option>)}
      </select>
    </label>
  );
}

function formatDate(value?: string | null): string {
  return value ? new Date(value).toLocaleString() : "never";
}
