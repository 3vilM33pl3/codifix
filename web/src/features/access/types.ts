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
