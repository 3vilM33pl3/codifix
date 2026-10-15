import type { AuthPrincipalResponse, AuthRole } from "../../types";

export interface OnboardingCapabilities {
  schema_version: 1;
  idp_management_enabled: boolean;
  invitation_enabled: boolean;
  idp_provider: "authentik" | null;
}

export interface ManagedUser {
  principal: AuthPrincipalResponse;
  disabled_at: string | null;
  created_at: string;
}

export interface EffectiveUserAccess {
  principal_id: string;
  disabled: boolean;
  global_permissions: string[];
  projects: {
    project: string;
    permissions: string[];
    grants: { source: string; role: AuthRole; group?: string | null }[];
  }[];
  group_rules: { group: string; role: AuthRole; global: boolean; project: string | null; matched: boolean }[];
}

export interface IdpOperation {
  id: string;
  operation: string;
  outcome: "success" | "error" | "pending" | "uncertain";
  provider_user_id?: number | null;
  invitation_id?: string | null;
  provider_correlation?: string | null;
  request_sha256?: string | null;
  provider_origin?: string | null;
}

export type MemoryArea = "general" | "workshop" | "homelab" | "trust";
export interface OnboardingManifestEntry {
  area: MemoryArea;
  owner_instance_id: string;
  repository_url: string;
  default_branch: string;
  offline_allowed: boolean;
  tools: { name: string; version: string; url: string; sha256: string }[];
  destinations: { name: string; provider_id: string; destination_id: string }[];
}
export interface OnboardingManifest extends OnboardingManifestEntry {
  project_id: string;
  project: string;
  revision: number;
  reviewed_by: string;
  reviewed_at: string;
}

export interface ManagedDevice {
  id: string;
  principal_id: string;
  name: string;
  platform: string;
  created_at: string;
  last_seen_at: string | null;
  revoked_at: string | null;
}

export interface CompanyOwner {
  owner_instance_id: string;
  area: MemoryArea;
  issuer: string;
  delegation_policy_revision: number;
  grant_management: boolean;
}
export interface CompanyConfiguration {
  company_id: string | null;
  writes_enabled: boolean;
  owners: CompanyOwner[];
}
export interface CompanyGrantRequest {
  company_id: string;
  operation_id: string;
  owner_instance_id: string;
  area: MemoryArea;
  project_id: string;
  issuer: string;
  subject: string;
  operation: "grant" | "revoke";
  role: "reader" | "writer" | "operator";
  expected_grant_revision: number;
  delegation_policy_revision: number;
}
export interface CompanyOperation {
  request: CompanyGrantRequest;
  request_sha256: string;
  outcome: "pending" | "uncertain" | "confirmed" | "rejected";
  receipt: null | {
    request: CompanyGrantRequest;
    request_sha256: string;
    principal_id: string;
    grant_revision: number;
    direct_grant_active: boolean;
    committed_at: string;
  };
  current_access: null | {
    observed_at: string;
    principal_id: string;
    disabled: boolean;
    project: string;
    direct_role: AuthRole | null;
    grant_company_id: string | null;
    owner_grant_revision: number | null;
    group_roles: AuthRole[];
    permissions: string[];
  };
}

interface CompanyDelegateIdentity {
  company_id: string;
  owner_instance_id: string;
  area: MemoryArea;
  issuer: string;
  principal_id: string;
  token_id: string;
}
export interface CompanyManagementDelegatePlan extends CompanyDelegateIdentity {
  delegation_policy_revision: number;
  projects: { project_id: string; role_ceiling: AuthRole }[];
  status: string;
  expires_at: string | null;
}
export interface CompanyDeliveryDelegatePlan extends CompanyDelegateIdentity {
  delivery_policy_revision: number;
  projects: string[];
  permissions: string[];
  status: string;
  expires_at: string | null;
}
interface DelegateConsent {
  request_id: string;
  name: string;
  expires_in_days: number;
  confirmed: boolean;
}
export type CompanyManagementDelegateRequest = CompanyDelegateIdentity & DelegateConsent
  & Pick<CompanyManagementDelegatePlan, "delegation_policy_revision" | "projects">;
export type CompanyDeliveryDelegateRequest = CompanyDelegateIdentity & DelegateConsent
  & Pick<CompanyDeliveryDelegatePlan, "delivery_policy_revision" | "projects">;
interface DelegateReceipt extends CompanyDelegateIdentity {
  request_id: string;
  expires_at: string;
  replayed: boolean;
  credential_issued: boolean;
  token: string | null;
}
export type CompanyDelegateResponse = DelegateReceipt & (
  { delegation_policy_revision: number; projects: { project_id: string; role_ceiling: AuthRole; permissions: string[] }[] }
  | { delivery_policy_revision: number; projects: string[]; permissions: string[] }
);
