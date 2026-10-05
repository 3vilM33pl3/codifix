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
