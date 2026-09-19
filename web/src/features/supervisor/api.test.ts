import { afterEach, beforeEach, expect, it, vi } from "vitest";

beforeEach(() => { vi.resetModules(); document.cookie = "memory_csrf=synthetic-csrf; path=/"; });
afterEach(() => { vi.unstubAllGlobals(); document.cookie = "memory_csrf=; max-age=0; path=/"; });
const request = { request_id: "request", instance_id: "instance", policy_revision: "revision", expected_enabled: true, enabled: false };

it("sends JSON with CSRF to the local policy endpoint and binds its receipt", async () => {
  const fetch = vi.fn().mockResolvedValueOnce(Response.json({ mode: "multi_user", read_only: false }))
    .mockResolvedValueOnce(Response.json({ ...request, replayed: false }));
  vi.stubGlobal("fetch", fetch);
  const { supervisorPolicy } = await import("../../api");
  await supervisorPolicy(request);
  const [url, options] = fetch.mock.calls[1];
  expect(url).toBe("/v1/supervisor/policy");
  expect(options.method).toBe("POST");
  expect(options.headers.get("Content-Type")).toBe("application/json");
  expect(options.headers.get("x-csrf-token")).toBe("synthetic-csrf");
  expect(JSON.parse(options.body)).toEqual(request);
  expect(options.credentials).toBe("same-origin");
});

it.each([401, 403, 409, 500, 503])("never displays a private %s control error body", async status => {
  vi.stubGlobal("fetch", vi.fn().mockResolvedValueOnce(Response.json({ mode: "multi_user", read_only: false }))
    .mockResolvedValueOnce(new Response("PRIVATE_DATABASE_DETAILS", { status })));
  const { supervisorPolicy } = await import("../../api");
  const failure = await supervisorPolicy(request).catch(error => error as Error);
  expect(failure).toBeInstanceOf(Error);
  expect(String(failure)).not.toContain("PRIVATE_DATABASE_DETAILS");
});

it("does not treat an unrelated receipt as confirmation", async () => {
  vi.stubGlobal("fetch", vi.fn().mockResolvedValueOnce(Response.json({ mode: "multi_user", read_only: false }))
    .mockResolvedValueOnce(Response.json({ ...request, instance_id: "another", replayed: false })));
  const { supervisorPolicy } = await import("../../api");
  await expect(supervisorPolicy(request)).rejects.toThrow("Receipt mismatch");
});

it("binds paid question receipt and sends only exact consent with CSRF", async () => {
  const question = { request_id: "question", question: "Exact text", profile_digest: "a".repeat(64), request_digest: "b".repeat(64) };
  const fetch = vi.fn().mockResolvedValueOnce(Response.json({ mode: "multi_user", read_only: false }))
    .mockResolvedValueOnce(Response.json({ request_id: "other" }));
  vi.stubGlobal("fetch", fetch);
  const { supervisorQuestionSubmit } = await import("../../api");
  await expect(supervisorQuestionSubmit("memory", question)).rejects.toThrow("receipt mismatch");
  const [url, options] = fetch.mock.calls[1];
  expect(url).toBe("/v1/projects/memory/supervisor/questions");
  expect(JSON.parse(options.body)).toEqual(question);
  expect(options.headers.get("x-csrf-token")).toBe("synthetic-csrf");
  expect(options.cache).toBe("no-store");
});

it.each([401, 403, 409, 429, 500])("sanitizes private question %s failures", async status => {
  vi.stubGlobal("fetch", vi.fn().mockResolvedValueOnce(Response.json({ mode: "multi_user", read_only: false }))
    .mockResolvedValueOnce(new Response("PRIVATE_PROVIDER_OR_STORAGE_BODY", { status })));
  const { supervisorQuestions } = await import("../../api");
  const error = await supervisorQuestions("memory", "").catch(e => e as Error);
  expect(error).toBeInstanceOf(Error);
  expect(String(error)).not.toContain("PRIVATE_PROVIDER_OR_STORAGE_BODY");
});
