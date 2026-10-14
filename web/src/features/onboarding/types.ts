import type { OnboardingManifest } from "../access/types";

export type DeviceProjectRole = "reader" | "writer";
export interface DeviceApprovalSelection {
  project_id: string;
  role: DeviceProjectRole;
  revision: number;
}
export interface DeviceReview {
  request_id: string;
  device_id: string;
  user_code: string;
  name: string;
  platform: string;
  credential_sha256: string;
  expires_at: string;
  projects: { manifest: OnboardingManifest; allowed_roles: DeviceProjectRole[]; offline_available: boolean }[];
}
export interface DeviceApprovalReceipt {
  status: "pending" | "approved" | "denied" | "expired";
  device_id?: string | null;
  principal_id?: string | null;
  projects?: { manifest: OnboardingManifest; role: DeviceProjectRole }[];
}

export interface CompanyDeviceReview extends Omit<DeviceReview, "projects"> {
  company_id: string;
  observed_at: string;
  projects: { project: DeviceReview["projects"][number]; manifest_sha256: string; curated_contributions_available?: boolean }[];
}
export interface CompanyDeviceSelection {
  area: "workshop" | "homelab" | "general";
  owner_instance_id: string;
  project_id: string;
  manifest_revision: number;
  manifest_sha256: string;
  offline_confirmed: true;
  contribution_confirmed?: boolean;
}
export interface CompanyDeviceApprovalRequest {
  user_code: string;
  confirmed: true;
  projects: CompanyDeviceSelection[];
}
