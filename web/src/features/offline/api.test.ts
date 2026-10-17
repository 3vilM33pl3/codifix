import { afterEach, beforeEach, expect, it, vi } from "vitest";
const uuid = "00000000-0000-4000-8000-000000000001";
const project = { subscription_id: uuid, device_id: uuid, company_id: uuid, owner_instance_id: uuid, project_id: uuid, space_id: uuid, area: "workshop", cursor: 0, initialized: true, observed_at: null, read_only: true };
beforeEach(() => { vi.resetModules(); document.cookie = "codifix_csrf=; max-age=0"; });
afterEach(() => { vi.unstubAllGlobals(); document.cookie = "codifix_csrf=; max-age=0"; });
function mock(status = 200, body: unknown = { schema_version: 1, area: "workshop", instance_id: uuid, projects: [project] }) {
  const fetcher = vi.fn(async (url: RequestInfo | URL) => {
    if (url === "/v1/auth/me") return new Response(JSON.stringify({ mode: "single_user", read_only: true }), { status: 200 });
    if (url === "/v1/auth/session/bootstrap") { document.cookie = "codifix_csrf=synthetic-csrf"; return new Response("{}", { status: 200 }); }
    return new Response(JSON.stringify(body), { status });
  }); vi.stubGlobal("fetch", fetcher); return fetcher;
}
it("accepts curated source metadata without inventing a publication locator", async () => {
  const fetcher = mock(); const api = await import("../../api");
  fetcher.mockImplementation(async url => url === "/v1/auth/me" ? new Response(JSON.stringify({ mode: "multi_user" })) : new Response(JSON.stringify({schema_version:1, project, hits:[{stream_id:uuid, source_record_id:uuid, source_version:3, author:"owner",authored_at:"2026-10-14T10:00:00Z",text:"text",summary:"summary",digest:"a".repeat(64),origin:"curated_project",evidence:[],read_only:true}]})));
  expect((await api.laptopCacheSearch(uuid,"text")).hits[0].locator).toBeUndefined();
});
it("validates exact history scope and ordered page versions", async () => {
  const page = {schema_version:1,project,canonical_id:uuid,read_only:true,next_after_version:null,current_state:null,versions:[{id:uuid,memory_type:"domain_fact",version:2,author:"owner",authored_at:"2026-10-14T10:00:00Z",updated_at:"2026-10-14T10:00:00Z",tombstone:true,text:"old",summary:"old",digest:"a".repeat(64),unavailable_source_ids:[]}]};
  const fetcher = mock(); const api = await import("../../api");
  fetcher.mockImplementation(async url => url === "/v1/auth/me" ? new Response(JSON.stringify({mode:"multi_user"})) : new Response(JSON.stringify(page)));
  expect((await api.laptopCacheHistory(uuid,uuid)).versions[0].tombstone).toBe(true);
  expect(fetcher.mock.calls.at(-1)?.[0]).toBe(`/v1/onboarding/cache/${uuid}/history/${uuid}?after_version=0&limit=16`);
  page.canonical_id = "00000000-0000-4000-8000-000000000002";
  await expect(api.laptopCacheHistory(uuid,uuid)).rejects.toThrow("history identity");
});
it("requires current writable authentication and sends an exact CSRF-protected correction",async()=>{
  const request={operation_id:uuid,source_memory_id:uuid,expected_digest:"a".repeat(64),content:{canonical_text:"correction",summary:"reviewed",memory_type:"domain_fact" as const,importance:3,confidence:0.8}};
  const fetcher=mock();const api=await import("../../api");
  fetcher.mockImplementation(async url=>url==="/v1/auth/me"?new Response(JSON.stringify({authenticated:true,mode:"single_user",read_only:true})):new Response("{}"));
  await expect(api.submitLaptopCorrection(uuid,request)).rejects.toThrow("write access");
  expect(fetcher.mock.calls.some(c=>String(c[0]).includes("/corrections"))).toBe(false);
  document.cookie="codifix_csrf=synthetic-csrf";
  fetcher.mockImplementation(async url=>url==="/v1/auth/me"?new Response(JSON.stringify({authenticated:true,mode:"single_user",read_only:false})):new Response(JSON.stringify({operation_id:uuid,subscription_id:uuid,local_memory_id:uuid,local_canonical_id:uuid,owner_canonical_id:uuid,expected_version:3,request_sha256:"b".repeat(64),staged_at:"2026-10-14T10:01:00Z"})));
  await api.submitLaptopCorrection(uuid,request);
  const [url,options]=fetcher.mock.calls.at(-1) as unknown as [string,RequestInit];
  expect(url).toBe(`/v1/onboarding/cache/${uuid}/corrections`);
  expect(new Headers(options.headers).get("x-csrf-token")).toBe("synthetic-csrf");
  expect(JSON.parse(options.body as string)).toEqual(request);
});
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
