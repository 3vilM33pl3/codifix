import type {
  AgentSnapshotResponse,
  ArchiveResponse,
  CodeGraphResponse,
  CodeGraphStatusResponse,
  CodeGraphViewFilters,
  CurateResponse,
  DeleteMemoryResponse,
  EmbeddingBackendsResponse,
  MemoryEntryResponse,
  MemoryHistoryResponse,
  ProjectMemoriesResponse,
  ProjectMemoryBundlePreview,
  ProjectMemoryExportOptions,
  ProjectMemoryImportPreview,
  ProjectMemoryImportResponse,
  ProjectMemoryGraphResponse,
  ProjectOverviewResponse,
  QueryRequest,
  QueryResponse,
  ReembedResponse,
  ReindexResponse,
  ReplacementPolicyRequest,
  ReplacementPolicyResponse,
  ReplacementProposalListResponse,
  ReplacementProposalResolutionResponse,
  ResumeResponse,
  ActivityListResponse,
  LlmAuditStatusResponse,
  LoopDefinitionResponse,
  LoopDefinitionsResponse,
  LoopApprovalDecisionResponse,
  LoopApprovalsResponse,
  LoopGlobalStateResponse,
  LoopGlobalStateUpdateRequest,
  LoopMemoryProposalDecisionResponse,
  LoopRunRequest,
  LoopRunResponse,
  LoopRunsResponse,
  LoopSettingsUpdateRequest,
  LoopSettingResponse,
  RuntimeStatusResponse,
  SkillContentResponse,
  SkillInventoryFilter,
  SkillInventoryReport,
  SkillUpgradeReport,
  UpToSpeedRequest,
  UpToSpeedResponse,
  AuthMeResponse,
  AuthMembershipResponse,
  AuthPrincipalResponse,
  AuthRole,
  AuthServiceTokenResponse,
} from "./types";

let sessionBootstrapPromise: Promise<void> | null = null;
let authMePromise: Promise<AuthMeResponse> | null = null;
let serviceReadOnly = false;

export type SupervisorProposalDetail = {
  project_id: string; action_id: string; proposal_digest: string; question: string;
  summary: string; proposal: string; created_at: string; can_review: boolean;
  citations: { index: number; canonical_id: string; version_id: string; version: number; evidence_digest: string }[];
  review: SupervisorReviewReceipt | null;
};
export type SupervisorReviewRequest = { request_id: string; proposal_digest: string; decision: "approve" | "reject" };
export type SupervisorReviewReceipt = SupervisorReviewRequest & { action_id: string; reviewer_id: string; recorded_at: string };
export type SupervisorQuestionPreview = { question: string; profile_digest: string; request_digest: string; destination: string; model: string; source_count: number; daily_call_limit: number; daily_usd_micros: string };
export type SupervisorQuestionRequest = { request_id: string; question: string; profile_digest: string; request_digest: string };
export type SupervisorQuestionStatus = { request_id: string; action_id: string; state: string; expires_at: string; proposal_available: boolean };
export type SupervisorQuestionList = { questions: SupervisorQuestionStatus[]; next_after: string | null };

export async function supervisorQuestions<T>(project: string, suffix: string, signal?: AbortSignal, request?: { question: string } | SupervisorQuestionRequest): Promise<T> {
  let response: Response;
  try {
    // Requester-private reads also need a revocable session, not anonymous local read access.
    const me = await getAuthMe();
    if (me.mode === "single_user") await ensureSession();
    response = await apiFetch(`/v1/projects/${encodeURIComponent(project)}/supervisor/questions${suffix}`, {
      signal, cache: "no-store", ...(request ? { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(request) } : {}),
    });
  } catch { throw new Error("Question outcome unconfirmed. Inspect its request ID; retry only the same payload."); }
  if (!response.ok) throw new Error(response.status === 401 || response.status === 403 ? "Question access denied. Current project permission and authentication are required."
    : response.status === 404 ? "Question or interaction profile unavailable."
    : response.status === 409 ? "Question conflicts with current sources, policy or prior request. Do not change terms under this request ID."
    : response.status === 429 ? "Question capacity or budget exhausted. Inspect status before retrying."
    : "Question outcome unconfirmed. Inspect this request ID; response details suppressed.");
  try { return await response.json() as T; } catch { throw new Error("Question receipt unreadable. Inspect this request ID."); }
}
export async function supervisorQuestionSubmit(project: string, request: SupervisorQuestionRequest): Promise<SupervisorQuestionStatus> {
  const result = await supervisorQuestions<SupervisorQuestionStatus>(project, "", undefined, request);
  if (result.request_id !== request.request_id) throw new Error("Question receipt mismatch. Inspect the original request ID.");
  return result;
}
export async function supervisorPrivate<T>(project: string, suffix: string, signal?: AbortSignal, request?: SupervisorReviewRequest): Promise<T> {
  let response: Response;
  try { response = await apiFetch(`/v1/projects/${encodeURIComponent(project)}/supervisor/proposals${suffix}`, {
    signal, cache: "no-store", ...(request ? { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(request) } : {}),
  }); } catch { throw new Error("Connection unavailable. A review may have committed; retry the same request ID."); }
  if (!response.ok) throw new Error(response.status === 401 || response.status === 403 ? "Access denied. Current project permissions and authentication are required."
    : response.status === 404 ? "Private interaction or this result is unavailable."
    : response.status === 409 ? "The profile, sources or review changed. Refresh before continuing."
    : "Private operation unconfirmed. Retry the same request ID; response details suppressed.");
  try { return await response.json() as T; } catch { throw new Error("Unreadable receipt. Retry the same request ID."); }
}
export async function supervisorReview(project: string, id: string, request: SupervisorReviewRequest): Promise<SupervisorReviewReceipt> {
  const result = await supervisorPrivate<SupervisorReviewReceipt>(project, `/${encodeURIComponent(id)}/review`, undefined, request);
  if (result.action_id !== id || result.request_id !== request.request_id || result.proposal_digest !== request.proposal_digest || result.decision !== request.decision)
    throw new Error("Review receipt mismatch. Inspect before further changes.");
  return result;
}

export async function supervisorStatus(after: string, signal?: AbortSignal): Promise<import("./features/supervisor/types").SupervisorResponse> {
  return parseJson(await apiFetch(`/v1/supervisor?${new URLSearchParams({ after, limit: "50" })}`, {
    signal, cache: "no-store",
  }));
}

export type SupervisorPolicyRequest = { request_id: string; instance_id: string; policy_revision: string; expected_enabled: boolean; enabled: boolean };
export async function supervisorPolicy(request: SupervisorPolicyRequest): Promise<void> {
  let response: Response;
  try { response = await apiFetch("/v1/supervisor/policy", { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(request) }); }
  catch { throw new Error("Outcome unconfirmed. Inspect history or retry this same request ID."); }
  if (response.ok) {
    let receipt: unknown;
    try { receipt = await response.json(); } catch { throw new Error("Receipt unreadable. Retry this same request ID."); }
    if (!receipt || typeof receipt !== "object" || !("request_id" in receipt) || receipt.request_id !== request.request_id
      || !("instance_id" in receipt) || receipt.instance_id !== request.instance_id
      || !("policy_revision" in receipt) || receipt.policy_revision !== request.policy_revision
      || !("enabled" in receipt) || receipt.enabled !== request.enabled
      || !("replayed" in receipt) || typeof receipt.replayed !== "boolean") throw new Error("Receipt mismatch. Inspect history before further changes.");
    return;
  }
  throw new Error(response.status === 401 || response.status === 403 ? "Denied. Current global administration and a writable service are required."
    : response.status === 409 ? "Conflict. Inspect the current identity and policy; do not change terms under this request ID."
    : "Outcome unconfirmed. Inspect history or retry this same request ID.");
}

/** True when the service reported read-only (student) mode on token bootstrap. */
export function isServiceReadOnly(): boolean {
  return serviceReadOnly;
}

/** Resolves the read-only flag after the auth-token bootstrap completes. */
export async function fetchServiceReadOnly(): Promise<boolean> {
  await getAuthMe();
  return serviceReadOnly;
}

async function parseJson<T>(response: Response): Promise<T> {
  if (!response.ok) {
    throw new Error(`${response.status} ${await response.text()}`);
  }
  return (await response.json()) as T;
}

/** Single-user installs mint a local-owner browser session over loopback. */
async function ensureSession(): Promise<void> {
  if (readCookie("memory_csrf")) return;
  if (!sessionBootstrapPromise) {
    sessionBootstrapPromise = fetch("/v1/auth/session/bootstrap", {
      method: "POST",
      credentials: "same-origin",
    })
      .then((response) => {
        if (!response.ok) throw new Error(`session bootstrap failed: ${response.status}`);
      })
      .catch((error) => {
        sessionBootstrapPromise = null;
        throw error;
      });
  }
  return sessionBootstrapPromise;
}

export async function getAuthMe(refresh = false): Promise<AuthMeResponse> {
  if (refresh) authMePromise = null;
  if (!authMePromise) {
    authMePromise = parseJson<AuthMeResponse>(
      await fetch("/v1/auth/me", { credentials: "same-origin" }),
    )
      .then((payload) => {
        serviceReadOnly = payload.read_only;
        return payload;
      })
      .catch((error) => {
        authMePromise = null;
        throw error;
      });
  }
  return authMePromise;
}

/** Explicit local-owner login; never bootstrap an OIDC or machine identity. */
export async function openLocalAdministration(): Promise<AuthMeResponse> {
  const before = await getAuthMe();
  if (before.mode !== "single_user" || before.principal.kind !== "internal" || before.read_only) {
    throw new Error("Local administration is unavailable for this identity or service.");
  }
  const response = await fetch("/v1/auth/session/bootstrap", {
    method: "POST", credentials: "same-origin", cache: "no-store",
  });
  if (!response.ok) throw new Error("Local administration login failed. The service must be reachable over loopback.");
  const authenticated = await getAuthMe(true);
  const admin = authenticated.principal.global_permissions !== undefined
    ? authenticated.principal.global_permissions.includes("system_admin")
    : authenticated.principal.global_role === "admin";
  if (!authenticated.authenticated || authenticated.mode !== "single_user"
    || authenticated.principal.kind !== "internal" || authenticated.read_only || !admin) {
    throw new Error("The server did not authorize local administration.");
  }
  return authenticated;
}

async function apiFetch(input: RequestInfo | URL, init: RequestInit = {}): Promise<Response> {
  const auth = await getAuthMe();
  const headers = new Headers(init.headers);
  const mutating =
    init.method !== undefined && !["GET", "HEAD", "OPTIONS"].includes(init.method.toUpperCase());
  if (mutating) {
    if (auth.mode === "single_user") await ensureSession();
    const csrf = readCookie("memory_csrf");
    if (csrf) headers.set("x-csrf-token", csrf);
  }
  return fetch(input, { ...init, credentials: "same-origin", headers });
}

function readCookie(name: string): string | null {
  const prefix = `${encodeURIComponent(name)}=`;
  for (const part of document.cookie.split(";")) {
    const value = part.trim();
    if (value.startsWith(prefix)) return decodeURIComponent(value.slice(prefix.length));
  }
  return null;
}

/** The configured LOCAL service is the only target; remote endpoints and trust
 * material are never accepted from this workflow or stored in the browser. */
export async function federationRequest<T>(project: string, space: string | null, suffix = "", body?: unknown, signal?: AbortSignal): Promise<T> {
  // Federation reads require a real, revocable browser session too. Anonymous
  // single-user reads of ordinary memories do not establish this credential.
  // Do not clear stale cookies or retry authorization failures with a new session.
  const auth = await getAuthMe();
  if (auth.mode === "single_user") await ensureSession();
  const base = `/v1/projects/${encodeURIComponent(project)}/federation${space ? `/${encodeURIComponent(space)}` : ""}`;
  return parseJson(await apiFetch(base + suffix, {
    method: body === undefined ? "GET" : "POST",
    headers: body === undefined ? undefined : { "Content-Type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
    signal,
    cache: "no-store",
  }));
}

export async function jobsRequest<T>(project: string, suffix = "", body?: unknown, signal?: AbortSignal): Promise<T> {
  return parseJson(await apiFetch(`/v1/projects/${encodeURIComponent(project)}/jobs${suffix}`, {
    method: body === undefined ? "GET" : "POST",
    headers: body === undefined ? undefined : { "Content-Type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
    signal, cache: "no-store",
  }));
}

/** Local cache inspection only: no remote destination, model, or browser storage. */
async function cacheRequest(path: string, signal?: AbortSignal, body?: { question: string; limit: number }): Promise<unknown> {
  let response: Response;
  try {
    const auth = await getAuthMe();
    if (auth.mode === "single_user") await ensureSession();
    response = await apiFetch(path, { signal, cache: "no-store", redirect: "error",
      ...(body ? { method: "POST", headers: { "Content-Type": "application/json" }, body: JSON.stringify(body) } : {}),
    });
  } catch { throw new Error("Synchronized cache unavailable. No remote search was attempted."); }
  if (!response.ok) throw new Error(response.status === 401 ? "Signed out. Local owner authentication is required."
    : response.status === 403 ? "Forbidden. Synchronized cache requires the authenticated local loopback owner."
    : response.status === 404 ? "Synchronized cache is not enabled or this subscription is unavailable in this build."
    : response.status === 400 ? "Invalid local cache search. Check question and result bounds."
    : "Synchronized cache unavailable. No remote search was attempted; response details suppressed.");
  try {
    const text = await response.text();
    if (new TextEncoder().encode(text).length > 256 * 1024) throw new Error("oversized");
    return JSON.parse(text);
  } catch { throw new Error("Unreadable or oversized synchronized cache response."); }
}
const cacheUuid = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
function validCacheProject(value: unknown): value is import("./features/offline/types").LaptopCacheProject {
  if (!value || typeof value !== "object") return false;
  const p = value as Record<string, unknown>;
  return ["subscription_id", "device_id", "company_id", "owner_instance_id", "project_id", "space_id"]
    .every(k => typeof p[k] === "string" && cacheUuid.test(p[k]))
    && ["workshop", "homelab", "general"].includes(String(p.area)) && p.read_only === true
    && typeof p.initialized === "boolean" && typeof p.cursor === "number" && p.cursor >= 0 && Number.isInteger(p.cursor)
    && (p.observed_at === null || (typeof p.observed_at === "string" && Number.isFinite(Date.parse(p.observed_at))));
}
export async function laptopCacheList(signal?: AbortSignal): Promise<import("./features/offline/types").LaptopCacheList> {
  const value = await cacheRequest("/v1/onboarding/cache", signal) as import("./features/offline/types").LaptopCacheList;
  if (value?.schema_version !== 1 || !cacheUuid.test(value.instance_id) || typeof value.area !== "string"
    || !Array.isArray(value.projects) || !value.projects.every(validCacheProject)
    || new Set(value.projects.map(p => p.subscription_id)).size !== value.projects.length)
    throw new Error("Invalid synchronized cache inventory; no results displayed.");
  return value;
}
export async function laptopCacheSearch(subscription: string, question: string, limit = 8, signal?: AbortSignal): Promise<import("./features/offline/types").LaptopCacheSearchResponse> {
  if (!cacheUuid.test(subscription) || !question.trim() || /[\u0000-\u0008\u000e-\u001f\u007f]/.test(question) || new TextEncoder().encode(question).length > 4096
    || !Number.isInteger(limit) || limit < 1 || limit > 32) throw new Error("Question must contain 1–4096 UTF-8 bytes; result limit must be 1–32.");
  const value = await cacheRequest(`/v1/onboarding/cache/${encodeURIComponent(subscription)}/search`, signal, { question, limit }) as import("./features/offline/types").LaptopCacheSearchResponse;
  if (value?.schema_version !== 1 || !validCacheProject(value.project) || value.project.subscription_id !== subscription
    || !Array.isArray(value.hits) || value.hits.length > limit || value.hits.some(h => h.read_only !== true
      || !cacheUuid.test(h.stream_id) || !cacheUuid.test(h.source_record_id) || !/^[a-f0-9]{64}$/i.test(h.digest)
      || [h.author, h.authored_at, h.text, h.summary].some(s => typeof s !== "string")
      || (h.origin === "curated_project" ? h.locator !== undefined || h.shared_version !== undefined : typeof h.locator !== "string" || !Number.isInteger(h.shared_version))
      || !Number.isInteger(h.source_version) || !Array.isArray(h.evidence)
      || h.evidence.some(e => [e.label, e.text, e.reviewer, e.reviewed_at].some(s => typeof s !== "string") || !/^[a-f0-9]{64}$/i.test(e.digest))))
    throw new Error("Invalid synchronized cache result binding/provenance; no results displayed.");
  return value;
}

export async function laptopCacheHistory(subscription: string, canonical: string, afterVersion = 0, signal?: AbortSignal): Promise<import("./features/offline/types").LaptopCacheHistoryResponse> {
  if (!cacheUuid.test(subscription) || !cacheUuid.test(canonical) || !Number.isInteger(afterVersion) || afterVersion < 0 || afterVersion > 4294967295) throw new Error("Exact cache history identity and cursor required.");
  const value = await cacheRequest(`/v1/onboarding/cache/${subscription}/history/${canonical}?after_version=${afterVersion}&limit=16`, signal) as import("./features/offline/types").LaptopCacheHistoryResponse;
  if (value?.schema_version !== 1 || value.read_only !== true || value.canonical_id !== canonical || !validCacheProject(value.project) || value.project.subscription_id !== subscription || !Array.isArray(value.versions) || value.versions.length > 16
    || (value.current_state !== null && (!value.current_state || !Number.isInteger(value.current_state.importance) || !Number.isFinite(value.current_state.confidence) || value.current_state.confidence<0 || value.current_state.confidence>1 || !["active","archived"].includes(value.current_state.status)))
    || value.versions.some((v, i) => !cacheUuid.test(v.id) || !Number.isInteger(v.version) || v.version <= afterVersion || i > 0 && value.versions[i - 1].version >= v.version || typeof v.tombstone !== "boolean" || !/^[a-f0-9]{64}$/i.test(v.digest) || [v.memory_type,v.author,v.authored_at,v.updated_at,v.text,v.summary].some(s => typeof s !== "string") || !Array.isArray(v.unavailable_source_ids) || !v.unavailable_source_ids.every(id => cacheUuid.test(id)))
    || value.next_after_version !== null && value.versions.at(-1)?.version !== value.next_after_version) throw new Error("Invalid cache history identity/provenance; no results displayed.");
  return value;
}

export type LaptopCorrectionRequest = {operation_id:string;source_memory_id:string;expected_digest:string;content:{canonical_text:string;summary:string;memory_type:import("./types").MemoryType;importance:number;confidence:number}};
export type LaptopCorrectionReceipt = {operation_id:string;subscription_id:string;local_memory_id:string;local_canonical_id:string;owner_canonical_id:string;expected_version:number;request_sha256:string;staged_at:string};
export async function submitLaptopCorrection(subscription: string, request: LaptopCorrectionRequest, signal?: AbortSignal): Promise<LaptopCorrectionReceipt> {
  if (!cacheUuid.test(subscription) || !cacheUuid.test(request.operation_id) || !cacheUuid.test(request.source_memory_id) || !/^[a-f0-9]{64}$/.test(request.expected_digest) || !request.content.canonical_text.trim() || new TextEncoder().encode(request.content.canonical_text).length>32768 || !request.content.summary.trim() || new TextEncoder().encode(request.content.summary).length>2048 || !Number.isInteger(request.content.importance) || request.content.importance<1 || request.content.importance>5 || !Number.isFinite(request.content.confidence) || request.content.confidence<0 || request.content.confidence>1) throw new Error("Review a bounded correction for the exact downloaded version.");
  const auth = await getAuthMe(true);
  if (!auth.authenticated || auth.read_only) throw new Error("Local authenticated write access is required to propose a correction.");
  const response = await apiFetch(`/v1/onboarding/cache/${subscription}/corrections`, {method:"POST",signal,cache:"no-store",redirect:"error",headers:{"Content-Type":"application/json"},body:JSON.stringify(request)});
  if (!response.ok) throw new Error(response.status===409?"The downloaded version changed, was withdrawn, or this operation has different inputs. Refresh owner history before creating a new correction.":response.status===403?"Current local correction permission or project contribution policy denied this operation.":"Correction staging failed; retain this operation and retry the same request. Details suppressed.");
  const text = await response.text();
  if (new TextEncoder().encode(text).length>65536) throw new Error("Oversized correction receipt.");
  const value = JSON.parse(text) as LaptopCorrectionReceipt;
  if(value.operation_id!==request.operation_id || value.subscription_id!==subscription || ![value.local_memory_id,value.local_canonical_id,value.owner_canonical_id].every(id=>cacheUuid.test(id)) || !Number.isInteger(value.expected_version) || !/^[a-f0-9]{64}$/.test(value.request_sha256)) throw new Error("Invalid correction receipt; retain the operation for inspection.");
  return value;
}

export function authLoginUrl(returnTo = window.location.pathname + window.location.search): string {
  return `/v1/auth/login?return_to=${encodeURIComponent(returnTo)}`;
}

export async function logout(): Promise<void> {
  await parseJson(await apiFetch("/v1/auth/logout", { method: "POST" }));
  authMePromise = null;
  sessionBootstrapPromise = null;
}

export async function listAuthPrincipals(): Promise<AuthPrincipalResponse[]> {
  return parseJson(await apiFetch("/v1/auth/principals"));
}

export async function listAuthTokens(): Promise<AuthServiceTokenResponse[]> {
  return parseJson(await apiFetch("/v1/auth/tokens"));
}

export async function createAuthToken(request: {
  name: string;
  project: string;
  role: AuthRole;
  expires_in_seconds?: number | null;
}): Promise<AuthServiceTokenResponse> {
  return parseJson(
    await apiFetch("/v1/auth/tokens", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(request),
    }),
  );
}

export async function revokeAuthToken(selector: string): Promise<AuthServiceTokenResponse> {
  return parseJson(
    await apiFetch(`/v1/auth/tokens/${encodeURIComponent(selector)}/revoke`, { method: "POST" }),
  );
}

export async function listAuthMemberships(): Promise<AuthMembershipResponse[]> {
  return parseJson(await apiFetch("/v1/auth/memberships"));
}

export async function grantAuthMembership(request: {
  principal_id: string;
  project: string;
  role: AuthRole;
}): Promise<AuthMembershipResponse> {
  return parseJson(
    await apiFetch("/v1/auth/memberships", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(request),
    }),
  );
}

export async function revokeAuthMembership(id: string): Promise<AuthMembershipResponse> {
  return parseJson(
    await apiFetch(`/v1/auth/memberships/${encodeURIComponent(id)}`, { method: "DELETE" }),
  );
}

/** Company administration shares the current service session and CSRF boundary. */
export class OnboardingRequestError extends Error {
  constructor(message: string, readonly operationId: string | null = null) { super(message); }
}

export async function onboardingRequest<T>(suffix: string, body?: unknown, signal?: AbortSignal): Promise<T> {
  const response = await apiFetch(`/v1/admin/onboarding${suffix}`, {
    method: body === undefined ? "GET" : "POST",
    headers: body === undefined ? undefined : { "content-type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
    signal,
    cache: "no-store",
  });
  if (!response.ok) {
    if (response.status === 503 && body !== undefined
      && (suffix === "/users/invite" || suffix === "/idp/users" || /^\/idp\/users\/\d+\/disable$/.test(suffix))) {
      let operationId: string | null = null;
      try {
        const payload: unknown = await response.json();
        if (payload && typeof payload === "object" && "error" in payload && typeof payload.error === "string") {
          // The service supplies this exact sanitized message. Extract only a canonical
          // v4 operation UUID; never show or retain provider/database response text.
          operationId = /^external effect may have occurred; inspect provider operation ([0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}); outcome could not be recorded$/i.exec(payload.error)?.[1] ?? null;
        }
      } catch { /* An unreadable response still leaves the external effect unconfirmed. */ }
      throw new OnboardingRequestError(`Provider action unconfirmed.${operationId ? ` Operation ${operationId}.` : ""} Refresh provider operations and inspect Authentik before resubmitting; another submission may duplicate an external effect.`, operationId);
    }
    throw new Error(response.status === 401 || response.status === 403
      ? "Access denied. Current global administration and a writable service are required for changes."
      : response.status === 409 ? "Conflict. Refresh and review the current record before saving."
      : response.status === 404 ? "Administration record or capability unavailable."
      : `Administration request failed (${response.status}). Refresh to inspect the outcome before retrying.`);
  }
  return response.json() as Promise<T>;
}

export async function deviceReview(userCode: string, signal?: AbortSignal): Promise<import("./features/onboarding/types").DeviceReview> {
  const response = await apiFetch(`/v1/onboarding/device/review?${new URLSearchParams({ code: userCode })}`, { signal, cache: "no-store" });
  return parseDeviceApprovalResponse(response);
}

async function companyDeviceRequest<T>(path: string, signal?: AbortSignal, body?: import("./features/onboarding/types").CompanyDeviceApprovalRequest): Promise<T> {
  let response: Response;
  try {
    response = await apiFetch(path, { signal, cache: "no-store", redirect: "error",
      ...(body ? { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(body) } : {}),
    });
  } catch { throw new Error("Company device outcome unconfirmed. Keep the reviewed request and retry only its exact confirmed selection; no legacy fallback was attempted."); }
  if (!response.ok) throw new Error(response.status === 401 || response.status === 403
    ? "Company device access denied. A current writable OIDC human session and owner authorization are required."
    : response.status === 404 ? "Company device request or opt-in capability unavailable. No legacy fallback was attempted."
    : response.status === 410 ? "Company device request expired. It cannot activate new access."
    : response.status === 409 ? "Company device review, owner authority, revision or digest changed. The frozen selection was not replaced; inspect the requesting device before a new review."
    : "Company device outcome unconfirmed. Keep the reviewed request and retry only its exact confirmed selection; response details suppressed.");
  try {
    const text = await response.text();
    if (new TextEncoder().encode(text).length > 256 * 1024) throw new Error("oversized");
    return JSON.parse(text) as T;
  } catch { throw new Error("Company device receipt unreadable. Keep the exact request/selection; do not assume approval or automatically submit a different selection."); }
}
export async function companyDeviceReview(code: string, signal?: AbortSignal): Promise<import("./features/onboarding/types").CompanyDeviceReview> {
  if (!/^[A-Z0-9]{4}-[A-Z0-9]{4}$/.test(code)) throw new Error("Invalid public device user code.");
  return companyDeviceRequest(`/v1/onboarding/company/device/review?${new URLSearchParams({ code })}`, signal);
}
export async function approveCompanyDevice(request: import("./features/onboarding/types").CompanyDeviceApprovalRequest, signal?: AbortSignal): Promise<import("./features/onboarding/types").DeviceApprovalReceipt> {
  return companyDeviceRequest("/v1/onboarding/company/device/approve", signal, request);
}

export class CompanyAgentRequestError extends Error {
  constructor(message: string, readonly retryable: boolean) { super(message); }
}
async function companyAgentRequest<T>(device: string, suffix: string, signal?: AbortSignal, mutate = false, body?: import("./features/access/companyAgentTypes").CompanyAgentRequest): Promise<T> {
  if (!/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(device)) throw new CompanyAgentRequestError("Invalid device identity; no request sent.", false);
  let response: Response;
  try {
    const auth = await getAuthMe();
    if (!auth.authenticated || auth.mode !== "multi_user" || auth.principal.kind !== "human_oidc" || (mutate && auth.read_only))
      throw new CompanyAgentRequestError("Agent access requires the exact parent OIDC human browser session; writable service required for changes. Administration is not an override.", false);
    if (signal?.aborted) throw new CompanyAgentRequestError("Cancelled agent request; no request sent.", false);
    response = await apiFetch(`/v1/onboarding/devices/${device}/agents${suffix}`, { signal, cache: "no-store", redirect: "error",
      ...(mutate ? { method: "POST", ...(body ? { headers: { "content-type": "application/json" }, body: JSON.stringify(body) } : {}) } : {}),
    });
  } catch (error) {
    if (error instanceof CompanyAgentRequestError) throw error;
    throw new CompanyAgentRequestError("Agent outcome unconfirmed. Inspect its stable request/agent IDs and retry only the unchanged request; response details suppressed.", true);
  }
  if (!response.ok) throw new CompanyAgentRequestError(response.status === 401 || response.status === 403
    ? "Agent access denied. The exact current parent human/device and owner permissions are required; no administrator override."
    : response.status === 409 ? "Agent request conflicts with reviewed scope/trust or prior IDs. Keep the frozen request; never silently regenerate identities or change disclosure."
    : response.status === 404 ? "Own device agent or capability unavailable. No legacy fallback was attempted."
    : response.status === 410 ? "Agent approval expired; no new activation is confirmed."
    : "Agent outcome unconfirmed. Retry only the exact same request/IDs; response details suppressed.", response.status >= 500);
  try {
    const raw = await response.text();
    if (new TextEncoder().encode(raw).length > 2 * 1024 * 1024) throw new Error();
    return JSON.parse(raw) as T;
  } catch { throw new CompanyAgentRequestError("Agent receipt unreadable. Inspect the original IDs; retry only the unchanged request.", true); }
}
export async function listCompanyAgents(device: string, signal?: AbortSignal): Promise<import("./features/access/companyAgentTypes").CompanyAgentList> {
  return companyAgentRequest(device, "", signal);
}
export async function approveCompanyAgent(device: string, request: import("./features/access/companyAgentTypes").CompanyAgentRequest & { confirmed: true }, signal?: AbortSignal): Promise<import("./features/access/companyAgentTypes").CompanyAgentReceipt> {
  return companyAgentRequest(device, "", signal, true, request);
}
export async function revokeCompanyAgent(device: string, agent: string, signal?: AbortSignal): Promise<import("./features/access/companyAgentTypes").CompanyAgentReceipt> {
  if (!/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(agent)) throw new CompanyAgentRequestError("Invalid agent identity; no request sent.", false);
  return companyAgentRequest(device, `/${agent}/revoke`, signal, true);
}

export async function approveDevice(request: {
  user_code: string;
  projects: import("./features/onboarding/types").DeviceApprovalSelection[];
  confirmed: true;
}): Promise<import("./features/onboarding/types").DeviceApprovalReceipt> {
  const response = await apiFetch("/v1/onboarding/device/approve", {
    method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(request), cache: "no-store",
  });
  return parseDeviceApprovalResponse(response);
}

async function parseDeviceApprovalResponse<T>(response: Response): Promise<T> {
  if (!response.ok) throw new Error(response.status === 401 || response.status === 403
    ? "Device approval denied. Sign in with a current OIDC human browser session; project membership may have changed."
    : response.status === 404 ? "Device request unavailable. Check the user code on the requesting device."
    : response.status === 410 ? "Device request expired. Start a new request on the device."
    : response.status === 409 ? "Device request, project membership or reviewed manifest revision changed. Review the current request again."
    : "Device approval outcome unconfirmed. Review the current request before submitting again.");
  try { return await response.json() as T; }
  catch { throw new Error("Device approval receipt unreadable. Review the current request before submitting again."); }
}

export async function getHealth(): Promise<Record<string, unknown>> {
  return parseJson(await fetch("/healthz"));
}

export async function getOverview(project: string): Promise<ProjectOverviewResponse> {
  return parseJson(await apiFetch(`/v1/projects/${encodeURIComponent(project)}/overview`));
}

export async function getCodeGraphStatus(project: string): Promise<CodeGraphStatusResponse> {
  return parseJson(await apiFetch(`/v1/projects/${encodeURIComponent(project)}/graph/status`));
}

export async function getCodeGraph(
  project: string,
  filters: Partial<CodeGraphViewFilters> = {},
): Promise<CodeGraphResponse> {
  const params = new URLSearchParams();
  if (filters.run_id) params.set("run_id", filters.run_id);
  if (filters.q) params.set("q", filters.q);
  if (filters.file_path) params.set("file_path", filters.file_path);
  if (filters.symbol) params.set("symbol", filters.symbol);
  if (filters.edge_kind) params.set("edge_kind", filters.edge_kind);
  if (filters.depth !== undefined) params.set("depth", String(filters.depth));
  if (filters.limit_nodes !== undefined) params.set("limit_nodes", String(filters.limit_nodes));
  if (filters.limit_edges !== undefined) params.set("limit_edges", String(filters.limit_edges));
  const query = params.toString();
  const suffix = query ? `?${query}` : "";
  return parseJson(await apiFetch(`/v1/projects/${encodeURIComponent(project)}/graph${suffix}`));
}

export async function getMemoryGraph(project: string, limit = 250): Promise<ProjectMemoryGraphResponse> {
  const params = new URLSearchParams({ limit: String(limit) });
  return parseJson(await apiFetch(`/v1/projects/${encodeURIComponent(project)}/memory-graph?${params.toString()}`));
}

export async function getMemories(project: string): Promise<ProjectMemoriesResponse> {
  return parseJson(await apiFetch(`/v1/projects/${encodeURIComponent(project)}/memories`));
}

export async function getMemory(memoryId: string): Promise<MemoryEntryResponse> {
  return parseJson(await apiFetch(`/v1/memory/${encodeURIComponent(memoryId)}`));
}

export async function getMemoryHistory(memoryId: string): Promise<MemoryHistoryResponse> {
  return parseJson(await apiFetch(`/v1/memory/${encodeURIComponent(memoryId)}/history`));
}

export async function getActivities(
  project: string,
  limit = 100,
  kind?: string | null,
): Promise<ActivityListResponse> {
  const params = new URLSearchParams({ limit: String(limit), include_details: "true" });
  if (kind) params.set("kind", kind);
  return parseJson(
    await apiFetch(`/v1/projects/${encodeURIComponent(project)}/activities?${params.toString()}`),
  );
}

export async function getActivityPage(
  project: string,
  after: number,
  signal?: AbortSignal,
): Promise<ActivityListResponse> {
  const params = new URLSearchParams({ after_seq: String(after), limit: "200", include_details: "true" });
  return parseJson(await apiFetch(`/v1/projects/${encodeURIComponent(project)}/activities?${params}`, { signal }));
}

export async function getRuntimeStatus(
  project: string,
  repoRoot?: string | null,
  skillFilter = "memory-layer",
): Promise<RuntimeStatusResponse> {
  const params = new URLSearchParams({ project });
  if (repoRoot) params.set("repo_root", repoRoot);
  params.set("skill_filter", skillFilter);
  return parseJson(await apiFetch(`/v1/runtime/status?${params.toString()}`));
}

export async function getSkills(
  repoRoot: string,
  filter: SkillInventoryFilter,
): Promise<SkillInventoryReport> {
  const params = new URLSearchParams({ repo_root: repoRoot, filter });
  return parseJson(await apiFetch(`/v1/skills?${params.toString()}`));
}

export async function getSkill(
  repoRoot: string,
  skillName: string,
  filter: SkillInventoryFilter,
): Promise<SkillContentResponse> {
  const params = new URLSearchParams({ repo_root: repoRoot, filter });
  return parseJson(await apiFetch(`/v1/skills/${encodeURIComponent(skillName)}?${params.toString()}`));
}

export async function repairSkills(
  repoRoot: string,
  filter: SkillInventoryFilter,
): Promise<SkillUpgradeReport> {
  return parseJson(
    await apiFetch("/v1/skills/repair", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ repo_root: repoRoot, filter }),
    }),
  );
}

export async function getUpToSpeed(request: UpToSpeedRequest): Promise<UpToSpeedResponse> {
  return parseJson(
    await apiFetch(`/v1/projects/${encodeURIComponent(request.project)}/up-to-speed`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(request),
    }),
  );
}

export async function getLlmAuditStatus(): Promise<LlmAuditStatusResponse> {
  return parseJson(await apiFetch("/v1/config/llm-audit"));
}

export async function setLlmAuditEnabled(enabled: boolean): Promise<LlmAuditStatusResponse> {
  return parseJson(
    await apiFetch("/v1/config/llm-audit", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ enabled }),
    }),
  );
}

export async function runQuery(request: QueryRequest): Promise<QueryResponse> {
  return parseJson(
    await apiFetch("/v1/query", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(request),
    }),
  );
}

export async function curate(project: string): Promise<CurateResponse> {
  return parseJson(
    await apiFetch("/v1/curate", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ project, batch_size: null }),
    }),
  );
}

export async function reindex(project: string, backend?: string | null): Promise<ReindexResponse> {
  return parseJson(
    await apiFetch("/v1/reindex", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ project, backend: backend ?? null }),
    }),
  );
}

export async function reembed(project: string, backend?: string | null): Promise<ReembedResponse> {
  return parseJson(
    await apiFetch("/v1/reembed", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ project, backend: backend ?? null }),
    }),
  );
}

export async function archiveProject(project: string): Promise<ArchiveResponse> {
  return parseJson(
    await apiFetch("/v1/archive", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ project, max_confidence: 0.3, max_importance: 1 }),
    }),
  );
}

export async function deleteMemory(memoryId: string): Promise<DeleteMemoryResponse> {
  return parseJson(
    await apiFetch("/v1/memory", {
      method: "DELETE",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ memory_id: memoryId }),
    }),
  );
}

export async function previewExportBundle(
  project: string,
  options: ProjectMemoryExportOptions,
): Promise<ProjectMemoryBundlePreview> {
  return parseJson(
    await apiFetch(`/v1/projects/${encodeURIComponent(project)}/bundle/export/preview`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(options),
    }),
  );
}

export async function exportBundle(
  project: string,
  options: ProjectMemoryExportOptions,
): Promise<Blob> {
  const response = await apiFetch(`/v1/projects/${encodeURIComponent(project)}/bundle/export`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(options),
  });
  if (!response.ok) {
    throw new Error(`${response.status} ${await response.text()}`);
  }
  return response.blob();
}

export async function previewImportBundle(
  project: string,
  file: File,
): Promise<ProjectMemoryImportPreview> {
  return parseJson(
    await apiFetch(`/v1/projects/${encodeURIComponent(project)}/bundle/import/preview`, {
      method: "POST",
      headers: { "content-type": "application/octet-stream" },
      body: file,
    }),
  );
}

export async function importBundle(
  project: string,
  file: File,
): Promise<ProjectMemoryImportResponse> {
  return parseJson(
    await apiFetch(`/v1/projects/${encodeURIComponent(project)}/bundle/import`, {
      method: "POST",
      headers: { "content-type": "application/octet-stream" },
      body: file,
    }),
  );
}

export async function getAgentSnapshot(): Promise<AgentSnapshotResponse> {
  return parseJson(await apiFetch("/v1/agents"));
}

export async function getResume(
  project: string,
  repoRoot?: string | null,
  includeLlmSummary = true,
): Promise<ResumeResponse> {
  return parseJson(
    await apiFetch(`/v1/projects/${encodeURIComponent(project)}/resume`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ project, repo_root: repoRoot || null, include_llm_summary: includeLlmSummary, limit: 20 }),
    }),
  );
}

export async function getEmbeddingBackends(project: string): Promise<EmbeddingBackendsResponse> {
  const params = new URLSearchParams({ project });
  return parseJson(await apiFetch(`/v1/embeddings/backends?${params.toString()}`));
}

export async function activateEmbeddingBackend(name: string): Promise<EmbeddingBackendsResponse> {
  return parseJson(
    await apiFetch("/v1/embeddings/activate", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ name }),
    }),
  );
}

export async function deactivateEmbeddingBackend(): Promise<EmbeddingBackendsResponse> {
  return parseJson(
    await apiFetch("/v1/embeddings/deactivate", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({}),
    }),
  );
}

export async function setEmbeddingCreationEnabled(
  name: string,
  enabled: boolean,
): Promise<EmbeddingBackendsResponse> {
  return parseJson(
    await apiFetch("/v1/embeddings/create-enabled", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ name, enabled }),
    }),
  );
}

export async function listLoopDefinitions(): Promise<LoopDefinitionsResponse> {
  return parseJson(await apiFetch("/v1/loops"));
}

export async function getLoopDefinition(
  loopId: string,
  project?: string | null,
  repoRoot?: string | null,
): Promise<LoopDefinitionResponse> {
  const params = new URLSearchParams();
  if (project) params.set("project", project);
  if (repoRoot) params.set("repo_root", repoRoot);
  const suffix = params.toString() ? `?${params.toString()}` : "";
  return parseJson(await apiFetch(`/v1/loops/${encodeURIComponent(loopId)}${suffix}`));
}

export async function getLoopRuns(options: {
  project?: string | null;
  loopId?: string | null;
  limit?: number;
}): Promise<LoopRunsResponse> {
  const params = new URLSearchParams();
  if (options.project) params.set("project", options.project);
  if (options.loopId) params.set("loop_id", options.loopId);
  if (options.limit) params.set("limit", String(options.limit));
  const suffix = params.toString() ? `?${params.toString()}` : "";
  return parseJson(await apiFetch(`/v1/loops/runs${suffix}`));
}

export async function getLoopRun(runId: string): Promise<LoopRunResponse> {
  return parseJson(await apiFetch(`/v1/loops/runs/${encodeURIComponent(runId)}`));
}

export async function getLoopApprovals(options: {
  project?: string | null;
  runId?: string | null;
  loopId?: string | null;
  status?: string | null;
  limit?: number;
}): Promise<LoopApprovalsResponse> {
  const params = new URLSearchParams();
  if (options.project) params.set("project", options.project);
  if (options.runId) params.set("run_id", options.runId);
  if (options.loopId) params.set("loop_id", options.loopId);
  if (options.status) params.set("status", options.status);
  if (options.limit) params.set("limit", String(options.limit));
  const suffix = params.toString() ? `?${params.toString()}` : "";
  return parseJson(await apiFetch(`/v1/loops/approvals${suffix}`));
}

async function postLoopApprovalDecision(
  approvalId: string,
  action: "approve" | "reject" | "edit",
  request: { reviewer?: string | null; reason?: string | null; edited_action?: unknown },
): Promise<LoopApprovalDecisionResponse> {
  return parseJson(
    await apiFetch(`/v1/loops/approvals/${encodeURIComponent(approvalId)}/${action}`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(request),
    }),
  );
}

export async function approveLoopApproval(
  approvalId: string,
  reason?: string,
): Promise<LoopApprovalDecisionResponse> {
  return postLoopApprovalDecision(approvalId, "approve", {
    reviewer: "web",
    reason: reason || "Approved from the browser UI.",
  });
}

export async function rejectLoopApproval(
  approvalId: string,
  reason?: string,
): Promise<LoopApprovalDecisionResponse> {
  return postLoopApprovalDecision(approvalId, "reject", {
    reviewer: "web",
    reason: reason || "Rejected from the browser UI.",
  });
}

export async function editLoopApproval(
  approvalId: string,
  editedAction: unknown,
  reason?: string,
): Promise<LoopApprovalDecisionResponse> {
  return postLoopApprovalDecision(approvalId, "edit", {
    reviewer: "web",
    reason: reason || "Edited from the browser UI.",
    edited_action: editedAction,
  });
}

async function postLoopMemoryProposalDecision(
  proposalId: string,
  action: "approve" | "reject" | "edit",
  request: {
    reviewer?: string | null;
    reason?: string | null;
    edited_candidate?: unknown;
    edited_evidence?: unknown;
    edited_risk_notes?: string | null;
  },
): Promise<LoopMemoryProposalDecisionResponse> {
  return parseJson(
    await apiFetch(`/v1/loops/memory-proposals/${encodeURIComponent(proposalId)}/${action}`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(request),
    }),
  );
}

export async function approveLoopMemoryProposal(
  proposalId: string,
  reason?: string,
): Promise<LoopMemoryProposalDecisionResponse> {
  return postLoopMemoryProposalDecision(proposalId, "approve", {
    reviewer: "web",
    reason: reason || "Approved from the browser UI.",
  });
}

export async function rejectLoopMemoryProposal(
  proposalId: string,
  reason?: string,
): Promise<LoopMemoryProposalDecisionResponse> {
  return postLoopMemoryProposalDecision(proposalId, "reject", {
    reviewer: "web",
    reason: reason || "Rejected from the browser UI.",
  });
}

export async function editLoopMemoryProposal(
  proposalId: string,
  editedCandidate: unknown,
  reason?: string,
): Promise<LoopMemoryProposalDecisionResponse> {
  return postLoopMemoryProposalDecision(proposalId, "edit", {
    reviewer: "web",
    reason: reason || "Edited from the browser UI.",
    edited_candidate: editedCandidate,
  });
}

export async function getLoopGlobalState(): Promise<LoopGlobalStateResponse> {
  return parseJson(await apiFetch("/v1/loops/global-kill-switch"));
}

export async function updateLoopGlobalState(
  request: LoopGlobalStateUpdateRequest,
): Promise<LoopGlobalStateResponse> {
  return parseJson(
    await apiFetch("/v1/loops/global-kill-switch", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(request),
    }),
  );
}

async function postLoopSetting(
  loopId: string,
  action: "enable" | "disable" | "pause" | "snooze",
  request: LoopSettingsUpdateRequest,
): Promise<LoopSettingResponse> {
  return parseJson(
    await apiFetch(`/v1/loops/${encodeURIComponent(loopId)}/${action}`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(request),
    }),
  );
}

export async function enableLoop(
  loopId: string,
  request: LoopSettingsUpdateRequest,
): Promise<LoopSettingResponse> {
  return postLoopSetting(loopId, "enable", request);
}

export async function disableLoop(
  loopId: string,
  request: LoopSettingsUpdateRequest,
): Promise<LoopSettingResponse> {
  return postLoopSetting(loopId, "disable", request);
}

export async function pauseLoop(
  loopId: string,
  request: LoopSettingsUpdateRequest,
): Promise<LoopSettingResponse> {
  return postLoopSetting(loopId, "pause", request);
}

export async function snoozeLoop(
  loopId: string,
  request: LoopSettingsUpdateRequest,
): Promise<LoopSettingResponse> {
  return postLoopSetting(loopId, "snooze", request);
}

export async function runLoop(loopId: string, request: LoopRunRequest): Promise<LoopRunResponse> {
  return parseJson(
    await apiFetch(`/v1/loops/${encodeURIComponent(loopId)}/run`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(request),
    }),
  );
}

export async function getReplacementPolicy(
  project: string,
  repoRoot?: string | null,
): Promise<ReplacementPolicyResponse> {
  const params = new URLSearchParams();
  if (repoRoot) params.set("repo_root", repoRoot);
  const suffix = params.toString() ? `?${params.toString()}` : "";
  return parseJson(
    await apiFetch(`/v1/projects/${encodeURIComponent(project)}/replacement-policy${suffix}`),
  );
}

export async function saveReplacementPolicy(
  project: string,
  request: ReplacementPolicyRequest,
): Promise<ReplacementPolicyResponse> {
  return parseJson(
    await apiFetch(`/v1/projects/${encodeURIComponent(project)}/replacement-policy`, {
      method: "PUT",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(request),
    }),
  );
}

export async function getReplacementProposals(
  project: string,
): Promise<ReplacementProposalListResponse> {
  return parseJson(
    await apiFetch(`/v1/projects/${encodeURIComponent(project)}/replacement-proposals`),
  );
}

export async function approveProposal(
  project: string,
  proposalId: string,
): Promise<ReplacementProposalResolutionResponse> {
  return parseJson(
    await apiFetch(
      `/v1/projects/${encodeURIComponent(project)}/replacement-proposals/${encodeURIComponent(proposalId)}/approve`,
      { method: "POST" },
    ),
  );
}

export async function rejectProposal(
  project: string,
  proposalId: string,
): Promise<ReplacementProposalResolutionResponse> {
  return parseJson(
    await apiFetch(
      `/v1/projects/${encodeURIComponent(project)}/replacement-proposals/${encodeURIComponent(proposalId)}/reject`,
      { method: "POST" },
    ),
  );
}
