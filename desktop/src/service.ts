import { invoke, isTauri } from "@tauri-apps/api/core";
import type { AuthMeResponse } from "../../web/src/types";
export type {
  AuthMeResponse,
  ProjectOverviewResponse,
  ProjectMemoriesResponse,
  ProjectMemoryListItem,
  MemoryEntryResponse,
  MemoryHistoryResponse,
  ProjectMemoryGraphResponse,
  AgentSnapshotResponse,
  AgentSessionResponse,
  ActivityListResponse,
  ActivityEvent,
  ReplacementProposalListResponse,
  ReplacementProposalRecord,
  ReplacementProposalResolutionResponse,
  ResumeResponse,
} from "../../web/src/types";
export type {
  JobsPage,
  JobDetail,
  JobSummary,
} from "../../web/src/features/jobs/types";

// Credentials and cookie sessions stay inside Rust. The renderer receives only data.
export const connect = (endpoint: string, token: string | null) =>
  invoke<AuthMeResponse>("connect_service", { endpoint, token });
export const disconnect = () => invoke<void>("disconnect_service");
export function request<T>(path: string, body?: unknown): Promise<T> {
  return invoke<T>("api_request", {
    path,
    method: body === undefined ? "GET" : "POST",
    body: body ?? null,
  });
}
export const projectPath = (project: string, suffix: string) =>
  `/v1/projects/${encodeURIComponent(project)}/${suffix}`;
export const chooseProject = () =>
  invoke<{ path: string; slug: string | null } | null>("choose_project");
export const openService = (section = "home", endpoint: string | null = null) =>
  invoke<void>("open_service", { section, endpoint });
export function errorText(error: unknown): string {
  if (!isTauri() && typeof error === "object" && error instanceof TypeError)
    return "Open the installed Codifix desktop app to connect to a service. Browser preview has no native session bridge.";
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return "The operation could not be confirmed. Refresh the service state before trying again.";
}
export const time = (value?: string | null) =>
  value && !Number.isNaN(Date.parse(value))
    ? new Date(value).toLocaleString(undefined, {
        dateStyle: "medium",
        timeStyle: "short",
      })
    : "Not reported";
export function canWrite(
  auth: AuthMeResponse,
  project: string,
  permission: string,
): boolean {
  if (auth.read_only || !auth.authenticated) return false;
  const { principal } = auth;
  const access = principal.projects.find((p) => p.project === project);
  // Explicit permission arrays are authoritative, even when empty.
  if (
    principal.global_permissions !== undefined ||
    access?.permissions !== undefined
  )
    return (
      !!principal.global_permissions?.includes(permission) ||
      !!access?.permissions?.includes(permission)
    );
  return (
    principal.global_role === "admin" ||
    access?.role === "admin" ||
    access?.role === "operator" ||
    (permission === "memory_curate" && access?.role === "writer")
  );
}

export type BrowserLoginProgress = {
  status: "pending" | "approved";
  request_id: string;
  expires_at: string;
  auth: AuthMeResponse | null;
};
export const startBrowserLogin = (endpoint: string) =>
  invoke<BrowserLoginProgress>("start_browser_login", { endpoint });
export const pollBrowserLogin = (requestId: string) =>
  invoke<BrowserLoginProgress>("poll_browser_login", { requestId });
export const openBrowserLogin = (requestId: string) =>
  invoke<void>("open_browser_login", { requestId });
export const cancelBrowserLogin = (endpoint: string) =>
  invoke<void>("cancel_browser_login", { endpoint });

export const forgetBrowserLogin = (endpoint: string) =>
  invoke<void>("forget_browser_login", { endpoint, confirmed: true });

export const revokeLegacyBrowserLogin = (endpoint: string) =>
  invoke<void>("revoke_legacy_browser_login", { endpoint });

export type AreaBinding = {
  name: string;
  instance_id: string;
  endpoint: string;
};
export type AreaChoice = {
  name: string;
  instance_id: string;
  binding: AreaBinding | null;
  error: string | null;
};
export const listAreas = () => invoke<AreaChoice[]>("list_areas");
export const resolveArea = (
  area: string | null = null,
  repository: string | null = null,
  project: string | null = null,
) => invoke<AreaBinding | null>("resolve_area", { area, repository, project });
export const selectArea = (
  area: string | null = null,
  repository: string | null = null,
  project: string | null = null,
  expectedInstanceId: string | null = null,
) =>
  invoke<AreaBinding | null>("select_area", {
    area,
    repository,
    project,
    expectedInstanceId,
  });
export const selectCustomService = () => invoke<void>("select_custom_service");
