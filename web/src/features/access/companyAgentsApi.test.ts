import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { aid, agentEnvelope, agentHuman } from "./companyAgentFixtures.testdata";
beforeEach(() => { vi.resetModules(); document.cookie = "memory_csrf=agent-csrf; path=/"; });
afterEach(() => { vi.unstubAllGlobals(); document.cookie = "memory_csrf=; max-age=0; path=/"; });
it("uses only owning-device routes, exact public body and CSRF; revoke has no JSON body", async () => {
  const fetcher = vi.fn(async (url: string) => new Response(JSON.stringify(url === "/v1/auth/me" ? agentHuman() : { schema_version: 1, agents: [] })));
  vi.stubGlobal("fetch", fetcher); const api = await import("../../api"); const body = { ...agentEnvelope().request, confirmed: true as const };
  await api.listCompanyAgents(aid(2)); await api.approveCompanyAgent(aid(2), body); await api.revokeCompanyAgent(aid(2), aid(4));
  const calls = fetcher.mock.calls as unknown as [string, RequestInit][];
  const post = calls.find(([url, init]) => url.endsWith("/agents") && init.method === "POST")!;
  expect(post[1]).toMatchObject({ credentials: "same-origin", redirect: "error", cache: "no-store", body: JSON.stringify(body) }); expect(new Headers(post[1].headers).get("x-csrf-token")).toBe("agent-csrf");
  const revoke = calls.find(([url]) => url.endsWith("/revoke"))!; expect(revoke[0]).toBe(`/v1/onboarding/devices/${aid(2)}/agents/${aid(4)}/revoke`); expect(revoke[1].body).toBeUndefined(); expect(new Headers(revoke[1].headers).get("x-csrf-token")).toBe("agent-csrf");
  expect(calls.some(([url]) => url.includes("/admin/") || url.includes("/company/device/"))).toBe(false);
});
it.each([401, 403, 404, 409, 410, 503])("sanitizes %s and only temporary failures permit exact retry, with no fallback", async status => {
  const fetcher = vi.fn(async (url: string) => url === "/v1/auth/me" ? new Response(JSON.stringify(agentHuman())) : new Response("PRIVATE_OWNER_BODY", { status })); vi.stubGlobal("fetch", fetcher);
  const api = await import("../../api"); try { await api.approveCompanyAgent(aid(2), { ...agentEnvelope().request, confirmed: true }); throw new Error("unexpected success"); }
  catch (error) { expect(error).toBeInstanceOf(api.CompanyAgentRequestError); expect((error as InstanceType<typeof api.CompanyAgentRequestError>).retryable).toBe(status === 503); expect((error as Error).message).not.toContain("PRIVATE_OWNER_BODY"); }
  expect(fetcher).toHaveBeenCalledTimes(2);
});
it.each(["read_only", "single_user", "machine", "anonymous"])("does not POST for %s, even with admin permissions", async mode => {
  const auth = agentHuman(); auth.principal.global_permissions = ["system_admin"];
  if (mode === "read_only") auth.read_only = true;
  if (mode === "single_user") auth.mode = "single_user";
  if (mode === "machine") auth.principal.kind = "service_token";
  if (mode === "anonymous") auth.authenticated = false;
  const fetcher = vi.fn(async () => new Response(JSON.stringify(auth))); vi.stubGlobal("fetch", fetcher); const api = await import("../../api");
  await expect(api.approveCompanyAgent(aid(2), { ...agentEnvelope().request, confirmed: true })).rejects.toThrow("exact parent OIDC"); expect(fetcher).toHaveBeenCalledTimes(1);
});
