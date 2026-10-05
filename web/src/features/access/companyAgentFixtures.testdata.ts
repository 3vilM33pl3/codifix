import type { AuthMeResponse } from "../../types";
import type { CompanyAgentEnvelope, CompanyAgentReceipt } from "./companyAgentTypes";
export const aid = (n: number) => `00000000-0000-4000-8000-${String(n).padStart(12, "0")}`;
export const agentHuman = (): AuthMeResponse => ({ authenticated: true, mode: "multi_user", read_only: false, principal: { id: aid(1), kind: "human_oidc", display_name: "Parent Reader", email: null, issuer: "https://id.test", subject: "parent", groups: [], projects: [], global_permissions: [] } });
export const agentEnvelope = (): CompanyAgentEnvelope => ({ device_id: aid(2), approval_endpoint: `/v1/onboarding/devices/${aid(2)}/agents`, agent_id: aid(4), contains_secret: false, automatic_approval: false, request: {
  request_id: aid(3), agent_id: aid(4), principal_id: aid(5), token_id: aid(6), caller: "codex", provider_id: "openai", credential_sha256: "a".repeat(64), expires_at: new Date(Date.now() + 3600000).toISOString(), confirmed: false,
  projects: [{ area: "workshop", owner_instance_id: aid(7), project_id: aid(8), manifest_revision: 3, manifest_sha256: "b".repeat(64), offline_confirmed: true }, { area: "workshop", owner_instance_id: aid(9), project_id: aid(8), manifest_revision: 7, manifest_sha256: "c".repeat(64), offline_confirmed: true }],
} });
export const agentReceipt = (value = agentEnvelope()): CompanyAgentReceipt => {
  const { confirmed: _confirmed, projects, ...request } = value.request;
  return { ...request, schema_version: 1, device_id: value.device_id, human_id: aid(1), issuer: "https://id.test", subject: "parent", company_id: aid(10), approved_at: new Date().toISOString(), revoked_at: null,
    projects: projects.map(selection => ({ selection: { ...selection }, destination: { name: request.caller, provider_id: request.provider_id, destination_id: request.provider_id } })) };
};
