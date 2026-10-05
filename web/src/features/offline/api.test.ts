import { afterEach, beforeEach, expect, it, vi } from "vitest";
const uuid = "00000000-0000-4000-8000-000000000001";
const project = { subscription_id: uuid, device_id: uuid, company_id: uuid, owner_instance_id: uuid, project_id: uuid, space_id: uuid, area: "workshop", cursor: 0, initialized: true, observed_at: null, read_only: true };
beforeEach(() => { vi.resetModules(); document.cookie = "memory_csrf=; max-age=0"; });
afterEach(() => { vi.unstubAllGlobals(); document.cookie = "memory_csrf=; max-age=0"; });
function mock(status = 200, body: unknown = { schema_version: 1, area: "workshop", instance_id: uuid, projects: [project] }) {
  const fetcher = vi.fn(async (url: RequestInfo | URL) => {
    if (url === "/v1/auth/me") return new Response(JSON.stringify({ mode: "single_user", read_only: true }), { status: 200 });
    if (url === "/v1/auth/session/bootstrap") { document.cookie = "memory_csrf=synthetic-csrf"; return new Response("{}", { status: 200 }); }
    return new Response(JSON.stringify(body), { status });
  }); vi.stubGlobal("fetch", fetcher); return fetcher;
}
it("bootstraps authenticated local cache reads and uses existing CSRF on read-only search", async () => {
  const fetcher = mock(); const api = await import("../../api"); await api.laptopCacheList();
  expect(fetcher.mock.calls.map(c => c[0])).toEqual(["/v1/auth/me", "/v1/auth/session/bootstrap", "/v1/onboarding/cache"]);
  fetcher.mockResolvedValueOnce(new Response(JSON.stringify({ schema_version: 1, project, hits: [] })));
  await api.laptopCacheSearch(uuid, "Exact question", 8);
  const [url, init] = fetcher.mock.calls.at(-1) as unknown as [string, RequestInit];
  expect(url).toBe(`/v1/onboarding/cache/${uuid}/search`);
  expect(init.method).toBe("POST"); expect(init.credentials).toBe("same-origin"); expect(init.redirect).toBe("error");
  expect(new Headers(init.headers).get("x-csrf-token")).toBe("synthetic-csrf");
  expect(JSON.parse(init.body as string)).toEqual({ question: "Exact question", limit: 8 });
});
it.each([401, 403, 404, 503])("suppresses raw %s bodies without presenting empty success", async status => {
  mock(status, { error: "PRIVATE_DATABASE_DETAILS" }); const api = await import("../../api");
  await expect(api.laptopCacheList()).rejects.toThrow(/Signed out|Forbidden|not enabled|unavailable/);
  await expect(api.laptopCacheList()).rejects.not.toThrow("PRIVATE_DATABASE_DETAILS");
});
it("validates query bytes and UUID before any request", async () => {
  const fetcher = mock(); const api = await import("../../api");
  for (const [id, q, n] of [["slug", "q", 8], [uuid, "é".repeat(2049), 8], [uuid, "q", 33], [uuid, "q\0", 8]] as const)
    await expect(api.laptopCacheSearch(id, q, n)).rejects.toThrow();
  expect(fetcher).not.toHaveBeenCalled();
});
it("rejects changed subscription, mutable responses and invalid provenance", async () => {
  const fetcher = mock(); const api = await import("../../api");
  fetcher.mockImplementation(async url => url === "/v1/auth/me" ? new Response(JSON.stringify({ mode: "multi_user" }))
    : new Response(JSON.stringify({ schema_version: 1, project: { ...project, read_only: false }, hits: [] })));
  await expect(api.laptopCacheSearch(uuid, "q")).rejects.toThrow("binding/provenance");
});
it("rejects wrong subscription receipts and oversized payloads", async () => {
  const fetcher = mock(); const api = await import("../../api");
  fetcher.mockImplementation(async url => url === "/v1/auth/me" ? new Response(JSON.stringify({ mode: "multi_user" }))
    : new Response(JSON.stringify({ schema_version: 1, project: { ...project, subscription_id: "00000000-0000-4000-8000-000000000002" }, hits: [] })));
  await expect(api.laptopCacheSearch(uuid, "q")).rejects.toThrow("binding/provenance");
  fetcher.mockResolvedValueOnce(new Response(" ".repeat(256 * 1024 + 1)));
  await expect(api.laptopCacheList()).rejects.toThrow("oversized");
});
