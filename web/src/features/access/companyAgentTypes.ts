import type { CompanyDeviceSelection } from "../onboarding/types";
import type { OnboardingManifest } from "./types";

export interface CompanyAgentRequest {
  replication_mode?: "shared_publications_v1" | "curated_project_v1";
  request_id: string; agent_id: string; principal_id: string; token_id: string;
  caller: "codex" | "claude"; provider_id: "openai" | "anthropic";
  credential_sha256: string; expires_at: string; projects: CompanyDeviceSelection[]; confirmed: boolean;
}
export interface CompanyAgentEnvelope {
  device_id: string; approval_endpoint: string; request: CompanyAgentRequest;
  agent_id: string; contains_secret: false; automatic_approval?: false;
}
export interface CompanyAgentReceipt extends Omit<CompanyAgentRequest, "confirmed" | "projects"> {
  schema_version: 1; device_id: string; human_id: string; issuer: string; subject: string; company_id: string;
  projects: { selection: CompanyDeviceSelection; destination: OnboardingManifest["destinations"][number] }[];
  approved_at: string; revoked_at: string | null;
}
export interface CompanyAgentList { schema_version: 1; agents: CompanyAgentReceipt[] }
export const AGENT_UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
const SHA = /^[0-9a-f]{64}$/;
const uuid = (v: unknown): v is string => typeof v === "string" && AGENT_UUID.test(v) && v !== "00000000-0000-0000-0000-000000000000";
const date = (v: unknown): v is string => typeof v === "string" && /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{1,9})?(?:Z|[+-]\d{2}:\d{2})$/.test(v) && Number.isFinite(Date.parse(v));
const text = (v: unknown): v is string => typeof v === "string" && v.length <= 2048 && !/[\u0000-\u001f\u007f]/.test(v);
const exact = (v: unknown, keys: string[]): v is Record<string, unknown> => Boolean(v && typeof v === "object" && !Array.isArray(v) && Object.keys(v).sort().join("|") === [...keys].sort().join("|"));
export const agentScopeKey = (p: CompanyDeviceSelection) => `${p.area}/${p.owner_instance_id}/${p.project_id}`;
const selectionKeys = ["area", "owner_instance_id", "project_id", "manifest_revision", "manifest_sha256", "offline_confirmed"];
function scopes(v: unknown): v is CompanyDeviceSelection[] {
  return Array.isArray(v) && v.length > 0 && v.length <= 32 && v.every(p => exact(p, selectionKeys)
    && ["workshop", "homelab", "general"].includes(String(p.area)) && uuid(p.owner_instance_id) && uuid(p.project_id)
    && Number.isSafeInteger(p.manifest_revision) && Number(p.manifest_revision) > 0 && typeof p.manifest_sha256 === "string" && SHA.test(p.manifest_sha256) && p.offline_confirmed === true)
    && new Set(v.map(agentScopeKey)).size === v.length;
}
const requestKeys = ["request_id", "agent_id", "principal_id", "token_id", "caller", "provider_id", "credential_sha256", "expires_at", "projects", "confirmed"];
const modeKeys = (v: unknown, keys: string[]) => v && typeof v === "object" && "replication_mode" in v ? [...keys, "replication_mode"] : keys;
function identity(v: Record<string, unknown>): boolean {
  const ids = [v.request_id, v.agent_id, v.principal_id, v.token_id];
  return ids.every(uuid) && new Set(ids).size === 4 && ((v.caller === "codex" && v.provider_id === "openai") || (v.caller === "claude" && v.provider_id === "anthropic"))
    && (v.replication_mode === undefined || v.replication_mode === "shared_publications_v1" || v.replication_mode === "curated_project_v1")
    && typeof v.credential_sha256 === "string" && SHA.test(v.credential_sha256) && date(v.expires_at);
}
export function parseCompanyAgentEnvelope(input: string): CompanyAgentEnvelope {
  try {
    if (new TextEncoder().encode(input).length > 64 * 1024) throw new Error();
    const v: unknown = JSON.parse(input);
    const keys = ["device_id", "approval_endpoint", "request", "agent_id", "contains_secret"];
    if (v && typeof v === "object" && "automatic_approval" in v) keys.push("automatic_approval");
    if (!exact(v, keys) || !uuid(v.device_id) || v.contains_secret !== false || ("automatic_approval" in v && v.automatic_approval !== false)
      || v.approval_endpoint !== `/v1/onboarding/devices/${v.device_id}/agents` || !exact(v.request, modeKeys(v.request, requestKeys)) || !identity(v.request)
      || v.request.confirmed !== false || v.agent_id !== v.request.agent_id || !scopes(v.request.projects)
      || [v.request.request_id, v.request.agent_id, v.request.principal_id, v.request.token_id].includes(v.device_id)) throw new Error();
    return v as unknown as CompanyAgentEnvelope;
  } catch { throw new Error("Invalid public agent envelope. Use the bounded CLI metadata only; no secrets, remote endpoints, Trust, unknown fields or automatic approval are accepted."); }
}
export function validAgentReceipt(v: unknown, device: string, human: string): v is CompanyAgentReceipt {
  const keys = [...requestKeys.filter(k => !["confirmed", "projects"].includes(k)), "schema_version", "device_id", "human_id", "issuer", "subject", "company_id", "projects", "approved_at", "revoked_at"];
  if (!exact(v, modeKeys(v, keys)) || !identity(v) || v.schema_version !== 1 || v.device_id !== device || v.human_id !== human || !uuid(v.company_id)
    || !text(v.issuer) || !text(v.subject) || !date(v.approved_at) || (v.revoked_at !== null && !date(v.revoked_at)) || !Array.isArray(v.projects)) return false;
  return scopes(v.projects.map(p => p?.selection)) && v.projects.every(p => exact(p, ["selection", "destination"])
    && exact(p.destination, ["name", "provider_id", "destination_id"]) && p.destination.name === v.caller && p.destination.provider_id === v.provider_id && p.destination.destination_id === v.provider_id);
}
export function matchesAgentRequest(receipt: CompanyAgentReceipt, request: CompanyAgentRequest): boolean {
  const same = ["request_id", "agent_id", "principal_id", "token_id", "caller", "provider_id", "credential_sha256"] as const;
  const scope = (p: CompanyDeviceSelection) => `${agentScopeKey(p)}/${p.manifest_revision}/${p.manifest_sha256}/${p.offline_confirmed}`;
  return same.every(k => receipt[k] === request[k]) && receipt.expires_at === request.expires_at
    && (receipt.replication_mode ?? "shared_publications_v1") === (request.replication_mode ?? "shared_publications_v1")
    && receipt.projects.map(p => scope(p.selection)).sort().join("|") === request.projects.map(scope).sort().join("|");
}
